//! `etendue detect` — detect the chessboard in a rendered dataset (P5-1b).
//!
//! Reads a dataset directory (`dataset.json` and `gt.json` from `etendue gt`,
//! images from `etendue render`), finds the corners of every image with
//! chess-corners' Radon detector (the G4.2 default), labels them with
//! calib-targets' chessboard detector, and writes `features.json`: per view,
//! the detected pixel of each board point. `tools/closed-loop` calibrates from
//! it.
//!
//! calib-targets labels a plain chessboard only up to the view: the lowest
//! label is `(0, 0)` and `u` runs along the image's +x, with no board size and
//! no colour. A view is kept only when the labels span the whole board, which
//! leaves two rotations; the colour of the squares between the corners (the
//! print's, `etendue_synth::board`) decides between them. A board that looks
//! the same turned by 180° cannot be decided and its views are dropped.
//!
//! The analytic ground truth is used **only to check** the labels: every
//! labelled corner must lie within [`MATCH_PX`] of its own point's analytic
//! pixel and nearer to it than to any other. Mismatches are reported, never
//! corrected, so the loop calibrates from exactly what the detector says.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use calib_targets_chessboard::{
    ChessboardDetector, ChessboardParams, DetectorConfig, GrayImageView, detect_corners,
};
use etendue_scene::TargetGeometry;
use etendue_synth::board::BoardLayout;
use etendue_synth::dataset::GroundTruth;
use etendue_synth::images::read_png_raw;
use serde::{Deserialize, Serialize};

/// A detection within this distance of a ground-truth corner is its match
/// (also the G4.2 matching radius).
pub const MATCH_PX: f64 = 1.5;
/// `features.json` format version.
pub const FEATURES_VERSION: u32 = 1;
/// Fraction of the squares whose colour must agree with the chosen rotation.
const PARITY_AGREEMENT: f64 = 0.9;

/// The index and distance of the point in `points` nearest to `p`.
pub fn nearest(points: impl IntoIterator<Item = [f64; 2]>, p: [f64; 2]) -> Option<(usize, f64)> {
    points
        .into_iter()
        .enumerate()
        .map(|(i, q)| (i, (q[0] - p[0]).hypot(q[1] - p[1])))
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

/// Why a view has no features.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewStatus {
    /// Features found and labelled.
    Ok,
    /// The image was not rendered (e.g. `etendue render --camera` chose others).
    NoImage,
    /// The detector found no board.
    NoBoard,
    /// The labelled corners do not span the whole board.
    Partial,
    /// The square colours do not decide the board's rotation.
    Ambiguous,
}

/// One detected board point.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Feature {
    /// Index into the target's points (`gt.json` `target.points`).
    pub point: usize,
    /// Detected pixel `[u, v]`, in the dataset's pixel-centre convention.
    pub pixel: [f64; 2],
}

/// The labels checked against the analytic ground truth.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    /// Features whose nearest analytic pixel is their own point's, within [`MATCH_PX`].
    pub matched: usize,
    /// Features nearer to another point's analytic pixel.
    pub mislabelled: usize,
    /// Features with no analytic pixel within [`MATCH_PX`].
    pub unmatched: usize,
    /// RMS distance of the matched features to their analytic pixels.
    pub rms_px: f64,
    /// Largest such distance.
    pub max_px: f64,
}

/// What the detector found in one image.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureView {
    /// Camera id.
    pub camera: String,
    /// Image path relative to the dataset directory.
    pub image: String,
    /// Outcome.
    pub status: ViewStatus,
    /// Board points found, in detector order.
    pub points: Vec<Feature>,
    /// The labels against the ground truth.
    pub check: Check,
}

/// One capture.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureCapture {
    /// Capture id.
    pub id: String,
    /// One view per camera, in dataset camera order.
    pub views: Vec<FeatureView>,
}

/// `features.json`: detected correspondences of a dataset, laid out like
/// `gt.json`'s captures.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Features {
    /// [`FEATURES_VERSION`].
    pub version: u32,
    /// The corner detector and the board labeller.
    pub detector: String,
    /// One entry per capture, in `gt.json` order.
    pub captures: Vec<FeatureCapture>,
}

/// The board as the labeller needs it: points by grid label, and the print.
pub struct Board {
    layout: BoardLayout,
    /// Inner corners `[columns, rows]`.
    size: [i32; 2],
    by_grid: HashMap<[i32; 2], usize>,
}

impl Board {
    /// A chessboard layout (points with grid labels).
    ///
    /// # Errors
    ///
    /// If the layout's points carry no grid labels.
    pub fn new(layout: BoardLayout) -> Result<Self> {
        let mut by_grid = HashMap::new();
        let mut size = [0, 0];
        for (i, p) in layout.points.iter().enumerate() {
            let g = p
                .grid
                .ok_or_else(|| anyhow!("board point {i} has no grid label"))?;
            by_grid.insert(g, i);
            size = [size[0].max(g[0] + 1), size[1].max(g[1] + 1)];
        }
        Ok(Self {
            layout,
            size,
            by_grid,
        })
    }

    /// Whether the print is ink at `(x, y)` in the target frame.
    fn dark_at(&self, x: f64, y: f64) -> Option<bool> {
        self.layout
            .cells
            .iter()
            .find(|c| (c.rect[0]..c.rect[2]).contains(&x) && (c.rect[1]..c.rect[3]).contains(&y))
            .map(|c| c.dark)
    }
}

/// Detector label `(u, v)` and the labels' span `(w, h)` → board grid label.
type Rotation = fn(i32, i32, i32, i32) -> [i32; 2];

/// The four rotations from detector labels `(u, v)` (spanning `w × h`) to
/// board grid labels `[column, row]`, each with the span it gives. Detector
/// and print are both seen from the front, so no mirror is needed.
fn rotations(w: i32, h: i32) -> [(Rotation, [i32; 2]); 4] {
    [
        (|u, v, _, _| [u, v], [w, h]),
        (|u, v, w, h| [w - 1 - u, h - 1 - v], [w, h]),
        (|u, v, w, _| [v, w - 1 - u], [h, w]),
        (|u, v, _, h| [h - 1 - v, u], [h, w]),
    ]
}

fn sample(gray: &[u8], width: usize, height: usize, x: f64, y: f64) -> f64 {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let at = |i: f64, j: f64| {
        let i = (i as i64).clamp(0, width as i64 - 1) as usize;
        let j = (j as i64).clamp(0, height as i64 - 1) as usize;
        f64::from(gray[j * width + i])
    };
    let top = at(x0, y0) * (1.0 - fx) + at(x0 + 1.0, y0) * fx;
    let bottom = at(x0, y0 + 1.0) * (1.0 - fx) + at(x0 + 1.0, y0 + 1.0) * fx;
    top * (1.0 - fy) + bottom * fy
}

/// Detect and label the board in one 8-bit image. Pixels are in chess-corners'
/// convention (pixel `i` at coordinate `i`).
pub fn detect_view(
    gray: &[u8],
    width: usize,
    height: usize,
    board: &Board,
    config: &DetectorConfig,
    labeller: &ChessboardDetector,
) -> (ViewStatus, Vec<Feature>) {
    let corners = detect_corners(
        &GrayImageView {
            width,
            height,
            data: gray,
        },
        config,
    );
    let Some(detection) = labeller.detect(&corners) else {
        return (ViewStatus::NoBoard, vec![]);
    };
    let (u0, v0) = detection
        .corners
        .iter()
        .fold((i32::MAX, i32::MAX), |(u, v), c| {
            (u.min(c.grid.u), v.min(c.grid.v))
        });
    let labels: HashMap<[i32; 2], [f64; 2]> = detection
        .corners
        .iter()
        .map(|c| {
            (
                [c.grid.u - u0, c.grid.v - v0],
                [f64::from(c.position.x), f64::from(c.position.y)],
            )
        })
        .collect();
    let w = labels.keys().map(|g| g[0]).max().unwrap_or(0) + 1;
    let h = labels.keys().map(|g| g[1]).max().unwrap_or(0) + 1;

    // Squares between four detected corners: image intensity at the centre,
    // and the four corners' labels.
    let quads: Vec<(f64, [i32; 2])> = labels
        .iter()
        .filter_map(|(&[u, v], &p00)| {
            let p10 = labels.get(&[u + 1, v])?;
            let p01 = labels.get(&[u, v + 1])?;
            let p11 = labels.get(&[u + 1, v + 1])?;
            let x = (p00[0] + p10[0] + p01[0] + p11[0]) / 4.0;
            let y = (p00[1] + p10[1] + p01[1] + p11[1]) / 4.0;
            Some((sample(gray, width, height, x, y), [u, v]))
        })
        .collect();
    if quads.is_empty() {
        return (ViewStatus::Partial, vec![]);
    }
    let (lo, hi) = quads.iter().fold((f64::MAX, f64::MIN), |(lo, hi), (g, _)| {
        (lo.min(*g), hi.max(*g))
    });
    let threshold = 0.5 * (lo + hi);

    // Rotations that fit the board, scored by square colour.
    let mut fits = Vec::new();
    for (map, span) in rotations(w, h) {
        if span != board.size {
            continue;
        }
        let agree = quads
            .iter()
            .filter(|(g, [u, v])| {
                // The square's centre on the print: the mean of its corners'.
                let corners = [[*u, *v], [u + 1, *v], [*u, v + 1], [u + 1, v + 1]];
                let mut centre = [0.0; 2];
                for [a, b] in corners {
                    let p = board.layout.points[board.by_grid[&map(a, b, w, h)]].position_m;
                    centre = [centre[0] + p[0] / 4.0, centre[1] + p[1] / 4.0];
                }
                board.dark_at(centre[0], centre[1]) == Some(*g < threshold)
            })
            .count();
        fits.push((agree, map));
    }
    if fits.is_empty() {
        return (ViewStatus::Partial, vec![]);
    }
    fits.sort_by_key(|f| std::cmp::Reverse(f.0));
    let (best, map) = fits[0];
    let decided = best as f64 >= PARITY_AGREEMENT * quads.len() as f64
        && fits.get(1).is_none_or(|(second, _)| *second < best);
    if !decided {
        return (ViewStatus::Ambiguous, vec![]);
    }
    let mut points: Vec<Feature> = detection
        .corners
        .iter()
        .map(|c| {
            let g = map(c.grid.u - u0, c.grid.v - v0, w, h);
            Feature {
                point: board.by_grid[&g],
                pixel: [f64::from(c.position.x), f64::from(c.position.y)],
            }
        })
        .collect();
    points.sort_by_key(|f| f.point);
    (ViewStatus::Ok, points)
}

/// Check labelled features against a view's analytic pixels
/// (`truth[point]`, `None` for points behind the camera).
pub fn check(points: &[Feature], truth: &[Option<[f64; 2]>]) -> Check {
    let mut out = Check::default();
    let mut sq = 0.0;
    let candidates: Vec<(usize, [f64; 2])> = truth
        .iter()
        .enumerate()
        .filter_map(|(i, p)| Some((i, (*p)?)))
        .collect();
    for f in points {
        match nearest(candidates.iter().map(|c| c.1), f.pixel) {
            Some((k, d)) if d <= MATCH_PX => {
                if candidates[k].0 == f.point {
                    out.matched += 1;
                    sq += d * d;
                    out.max_px = out.max_px.max(d);
                } else {
                    out.mislabelled += 1;
                }
            }
            _ => out.unmatched += 1,
        }
    }
    out.rms_px = (sq / out.matched.max(1) as f64).sqrt();
    out
}

/// The detector configuration: chess-corners' Radon preset (G4.2).
pub fn radon() -> DetectorConfig {
    DetectorConfig::radon()
}

/// `etendue detect <dir>`: detect every image of a dataset, write
/// `<dir>/features.json`, and print a summary.
pub fn run(dir: &Path) -> Result<()> {
    let read = |name: &str| -> Result<String> {
        let path = dir.join(name);
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))
    };
    let dataset: vision_calibration_dataset::DatasetSpec =
        serde_json::from_str(&read("dataset.json")?).context("parsing dataset.json")?;
    let gt: GroundTruth = serde_json::from_str(&read("gt.json")?).context("parsing gt.json")?;
    let geometry = TargetGeometry::Board {
        board: dataset.target.clone(),
    };
    let layout = etendue_synth::board::layout(&geometry)?
        .ok_or_else(|| anyhow!("the dataset's target has no printable layout"))?;
    if !matches!(
        dataset.target,
        vision_calibration_dataset::TargetSpec::Chessboard { .. }
    ) {
        return Err(anyhow!("etendue detect handles chessboards only"));
    }
    let board = Board::new(layout)?;
    if board.layout.points.len() != gt.target.points.len() {
        return Err(anyhow!(
            "dataset.json's board has {} points, gt.json's {}",
            board.layout.points.len(),
            gt.target.points.len()
        ));
    }
    // Detections are in chess-corners' convention (pixel `i` at `i`); shift
    // them into the dataset's.
    let offset = gt.pixel_centre.offset();
    let config = radon();
    let labeller = ChessboardDetector::new(ChessboardParams::default())
        .map_err(|e| anyhow!("chessboard params: {e}"))?;

    let mut captures = Vec::with_capacity(gt.captures.len());
    for capture in &gt.captures {
        let mut views = Vec::with_capacity(capture.views.len());
        for view in &capture.views {
            let path = dir.join(&view.image);
            if !path.is_file() {
                views.push(FeatureView {
                    camera: view.camera.clone(),
                    image: view.image.clone(),
                    status: ViewStatus::NoImage,
                    points: vec![],
                    check: Check::default(),
                });
                continue;
            }
            let image = read_png_raw(&path)?;
            let gray: Vec<u8> = match image.bits {
                8 => image.dn.iter().map(|&v| v as u8).collect(),
                16 => image.dn.iter().map(|&v| (v >> 8) as u8).collect(),
                b => return Err(anyhow!("{}: {b}-bit image", view.image)),
            };
            let (status, mut points) = detect_view(
                &gray,
                image.width as usize,
                image.height as usize,
                &board,
                &config,
                &labeller,
            );
            for p in &mut points {
                p.pixel = [p.pixel[0] + offset, p.pixel[1] + offset];
            }
            let mut truth = vec![None; gt.target.points.len()];
            for p in &view.points {
                truth[p.point] = p.pixel;
            }
            let check = check(&points, &truth);
            views.push(FeatureView {
                camera: view.camera.clone(),
                image: view.image.clone(),
                status,
                points,
                check,
            });
        }
        captures.push(FeatureCapture {
            id: capture.id.clone(),
            views,
        });
    }
    let features = Features {
        version: FEATURES_VERSION,
        detector: "chess-corners Radon (DetectorConfig::radon), calib-targets-chessboard labels"
            .into(),
        captures,
    };
    let path = dir.join("features.json");
    std::fs::write(&path, serde_json::to_string_pretty(&features)? + "\n")
        .with_context(|| format!("writing {}", path.display()))?;
    summarise(&features, &gt, &path);
    Ok(())
}

fn summarise(features: &Features, gt: &GroundTruth, path: &Path) {
    println!(
        "| camera | views ok | no board | partial | ambiguous | points | mislabelled | unmatched | RMS vs GT px | max px |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|");
    for (i, camera) in gt.cameras.iter().enumerate() {
        let views: Vec<&FeatureView> = features
            .captures
            .iter()
            .map(|c| &c.views[i])
            .filter(|v| v.status != ViewStatus::NoImage)
            .collect();
        if views.is_empty() {
            println!("| {} | no images |", camera.id);
            continue;
        }
        let count = |s: ViewStatus| views.iter().filter(|v| v.status == s).count();
        let matched: usize = views.iter().map(|v| v.check.matched).sum();
        let sq: f64 = views
            .iter()
            .map(|v| v.check.rms_px.powi(2) * v.check.matched as f64)
            .sum();
        println!(
            "| {} | {}/{} | {} | {} | {} | {} | {} | {} | {:.4} | {:.4} |",
            camera.id,
            count(ViewStatus::Ok),
            views.len(),
            count(ViewStatus::NoBoard),
            count(ViewStatus::Partial),
            count(ViewStatus::Ambiguous),
            views.iter().map(|v| v.points.len()).sum::<usize>(),
            views.iter().map(|v| v.check.mislabelled).sum::<usize>(),
            views.iter().map(|v| v.check.unmatched).sum::<usize>(),
            (sq / matched.max(1) as f64).sqrt(),
            views.iter().map(|v| v.check.max_px).fold(0.0, f64::max),
        );
    }
    let wrong: usize = features
        .captures
        .iter()
        .flat_map(|c| &c.views)
        .map(|v| v.check.mislabelled + v.check.unmatched)
        .sum();
    if wrong > 0 {
        eprintln!(
            "warning: {wrong} feature(s) disagree with the ground truth; they are kept as detected"
        );
    }
    println!("features → {}", path.display());
}

#[cfg(test)]
mod tests {
    use super::*;
    use etendue_synth::board::layout;
    use vision_calibration_dataset::TargetSpec;

    /// A board with `cols × rows` inner corners of `square` px, drawn with its
    /// print's top-left at `origin`, turned by `angle` (radians, image axes),
    /// area-sampled 4 × 4 per pixel: background 240, paper 204, ink 7 (the
    /// G4.2 radiances on the 240 scale). Returns the image, the board and the
    /// true pixel of every point.
    fn synthetic(
        cols: u32,
        rows: u32,
        square: f64,
        angle: f64,
        size: [usize; 2],
    ) -> (Vec<u8>, Board, Vec<Option<[f64; 2]>>) {
        let geometry = TargetGeometry::Board {
            board: TargetSpec::Chessboard {
                rows,
                cols,
                square_size_m: 0.02,
            },
        };
        let board = Board::new(layout(&geometry).unwrap().unwrap()).unwrap();
        let [bw, bh] = board.layout.size_m;
        let scale = square / 0.02;
        let (s, c) = angle.sin_cos();
        let centre = [size[0] as f64 / 2.0, size[1] as f64 / 2.0];
        // Target frame (x right, y up) → image (x right, y down), turned.
        let to_image = |x: f64, y: f64| {
            let (px, py) = (x * scale, -y * scale);
            [centre[0] + c * px - s * py, centre[1] + s * px + c * py]
        };
        let to_target = |u: f64, v: f64| {
            let (dx, dy) = (u - centre[0], v - centre[1]);
            let (px, py) = (c * dx + s * dy, -s * dx + c * dy);
            (px / scale, -py / scale)
        };
        let n = 4;
        let mut gray = vec![0_u8; size[0] * size[1]];
        for j in 0..size[1] {
            for i in 0..size[0] {
                let mut acc = 0.0;
                for b in 0..n {
                    for a in 0..n {
                        let u = i as f64 - 0.5 + (f64::from(a) + 0.5) / f64::from(n);
                        let v = j as f64 - 0.5 + (f64::from(b) + 0.5) / f64::from(n);
                        let (x, y) = to_target(u, v);
                        acc += if x.abs() < bw / 2.0 && y.abs() < bh / 2.0 {
                            if board.dark_at(x, y).unwrap() {
                                7.0
                            } else {
                                204.0
                            }
                        } else {
                            240.0
                        };
                    }
                }
                gray[j * size[0] + i] = (acc / f64::from(n * n)).round() as u8;
            }
        }
        let truth = board
            .layout
            .points
            .iter()
            .map(|p| {
                let q = to_image(p.position_m[0], p.position_m[1]);
                let inside =
                    (0.0..size[0] as f64).contains(&q[0]) && (0.0..size[1] as f64).contains(&q[1]);
                inside.then_some(q)
            })
            .collect();
        (gray, board, truth)
    }

    fn run_view(gray: &[u8], size: [usize; 2], board: &Board) -> (ViewStatus, Vec<Feature>) {
        let labeller = ChessboardDetector::new(ChessboardParams::default()).unwrap();
        detect_view(gray, size[0], size[1], board, &radon(), &labeller)
    }

    #[test]
    fn labels_the_board_in_every_rotation() {
        let size = [480, 480];
        for degrees in [0.0_f64, 12.0, 90.0, 105.0, 180.0, 200.0, 270.0, 290.0] {
            let (gray, board, truth) = synthetic(9, 6, 30.0, degrees.to_radians(), size);
            let (status, points) = run_view(&gray, size, &board);
            assert_eq!(status, ViewStatus::Ok, "{degrees}°");
            assert_eq!(points.len(), 54, "{degrees}°");
            let c = check(&points, &truth);
            assert_eq!((c.mislabelled, c.unmatched), (0, 0), "{degrees}°: {c:?}");
            assert!(c.max_px < 0.2, "{degrees}°: {c:?}");
        }
    }

    #[test]
    fn drops_a_partial_board() {
        // The board spans x = 90 … 390 with its last inner corners at 360:
        // blank everything right of 345, as if the image ended there.
        let size = [480, 480];
        let (gray, board, _) = synthetic(9, 6, 30.0, 0.0, size);
        let mut cut = gray.clone();
        for row in cut.chunks_exact_mut(size[0]) {
            row[345..].fill(240);
        }
        let (status, points) = run_view(&cut, size, &board);
        assert_eq!(status, ViewStatus::Partial);
        assert!(points.is_empty());
    }

    #[test]
    fn a_half_turn_symmetric_board_is_ambiguous() {
        // 8 × 6 inner corners: 9 × 7 squares, the same after a half turn.
        let size = [480, 480];
        let (gray, board, _) = synthetic(8, 6, 30.0, 0.2, size);
        assert_eq!(run_view(&gray, size, &board).0, ViewStatus::Ambiguous);
    }

    #[test]
    fn check_counts_mislabels_and_strays() {
        let truth = vec![Some([10.0, 10.0]), Some([40.0, 10.0]), None];
        let points = [
            Feature {
                point: 0,
                pixel: [10.1, 10.0],
            },
            Feature {
                point: 0,
                pixel: [40.0, 10.2],
            },
            Feature {
                point: 2,
                pixel: [90.0, 90.0],
            },
        ];
        let c = check(&points, &truth);
        assert_eq!((c.matched, c.mislabelled, c.unmatched), (1, 1, 1));
        assert!((c.rms_px - 0.1).abs() < 1e-9);
    }
}
