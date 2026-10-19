//! `etendue detect` — detect the target in a rendered dataset (P5-1b, P7-5).
//!
//! Reads a dataset directory (`gt.json` from `etendue gt`, images from
//! `etendue render`), finds the corners of every image with chess-corners'
//! Radon detector (the G4.2 default), labels them with calib-targets' detector
//! for the target's kind, and writes `features.json`: per view, the detected
//! pixel of each target point. `tools/closed-loop` calibrates from it.
//!
//! The target is `gt.json`'s `target.geometry`; a dataset written before it
//! was recorded takes its board from `dataset.json`. One corner detection per
//! image feeds the labeller of the kind:
//!
//! - chessboard: calib-targets' chessboard labeller, the rotation decided by
//!   square colour (`chessboard`);
//! - ChArUco: the ChArUco detector, corners matched by ChArUco id;
//! - marker board: the marker board detector, its grid placed by the three
//!   circles, corners matched by grid label;
//! - PuzzleBoard: the PuzzleBoard detector, corners matched by master id;
//! - PuzzlePole: the PuzzlePole detector, corners matched by pole id.
//!
//! The analytic ground truth is used **only to check** the labels: every
//! labelled corner must lie within [`MATCH_PX`] of its own point's analytic
//! pixel and nearer to it than to any other. Mismatches are reported, never
//! corrected, so the loop calibrates from exactly what the detector says. A
//! label that names no point of the target (a corner the detector places off
//! the board) cannot be a feature; it is counted as `off_board`.

mod charuco;
mod chessboard;
pub mod laser;
mod marker;
mod pole;
mod puzzleboard;

use std::collections::HashMap;
use std::hash::Hash;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use calib_targets_chessboard::{
    ChessCorner, DetectorConfig, GrayImageView, default_chess_config, detect_corners,
};
use calib_targets_print::TargetSpec;
use etendue_scene::TargetGeometry;
use etendue_synth::dataset::GroundTruth;
use etendue_synth::gt::{BoardPoint, Occlusion};
use etendue_synth::images::read_png_luma8;
use serde::{Deserialize, Serialize};

use crate::progress::{Control, Stage};

/// A detection within this distance of a ground-truth corner is its match
/// (also the G4.2 matching radius).
pub const MATCH_PX: f64 = 1.5;
/// `features.json` format version.
pub const FEATURES_VERSION: u32 = 1;

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
    /// Too little of the board is seen: a chessboard's labels do not span the
    /// whole board, or a self-identifying target's fragment is too small to
    /// decode.
    Partial,
    /// The board's rotation or position is not decided: a chessboard's square
    /// colours, a marker board's circles that fit two frames, a PuzzleBoard
    /// whose pieces decode to different places.
    Ambiguous,
    /// A grid was found but its markers, circles or code did not identify it.
    Undecoded,
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
    /// Labelled corners whose label names no point of the target (not
    /// features).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub off_board: usize,
    /// RMS distance of the matched features to their analytic pixels.
    pub rms_px: f64,
    /// Largest such distance.
    pub max_px: f64,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
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
    /// Why the view has no features, in the detector's words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Board points found, by point index.
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
    /// The target's kind, as a printed target names it (`chessboard`,
    /// `charuco`, `marker_board`, `puzzle_board`, `puzzlepole`); empty in
    /// files written before it was recorded (chessboards).
    #[serde(default)]
    pub kind: String,
    /// The corner detector and the board labeller.
    pub detector: String,
    /// One entry per capture, in `gt.json` order.
    pub captures: Vec<FeatureCapture>,
}

/// What the labeller made of one image.
#[derive(Clone, Debug, PartialEq)]
pub struct Detection {
    /// Outcome.
    pub status: ViewStatus,
    /// Why there are no features, in the detector's words.
    pub reason: Option<String>,
    /// Board points found, by point index.
    pub points: Vec<Feature>,
    /// Labelled corners whose label names no point of the target.
    pub off_board: usize,
}

impl Detection {
    fn ok(mut points: Vec<Feature>, off_board: usize) -> Self {
        points.sort_by_key(|f| f.point);
        Self {
            status: ViewStatus::Ok,
            reason: None,
            points,
            off_board,
        }
    }

    fn failed(status: ViewStatus, reason: impl Into<String>) -> Self {
        Self {
            status,
            reason: Some(reason.into()),
            points: vec![],
            off_board: 0,
        }
    }
}

/// Point index by id, for the targets that number their points.
fn by_id(points: &[BoardPoint]) -> Result<HashMap<u32, usize>> {
    points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            p.id.map(|id| (id, i))
                .ok_or_else(|| anyhow!("board point {i} has no id"))
        })
        .collect()
}

/// Labelled corners (`label`, image position) as features: a label `index`
/// knows names its point; any other (`None` included) is off the board.
fn labelled<K: Eq + Hash>(
    index: &HashMap<K, usize>,
    // Plain arrays: calib-targets' points are its own nalgebra's (the detector stack's, which
    // may differ from the solvers'; ADR 0001 amendment).
    corners: impl IntoIterator<Item = (Option<K>, [f32; 2])>,
) -> Detection {
    let mut points = Vec::new();
    let mut off_board = 0;
    for (label, p) in corners {
        match label.and_then(|k| index.get(&k)) {
            Some(&point) => points.push(Feature {
                point,
                pixel: [f64::from(p[0]), f64::from(p[1])],
            }),
            None => off_board += 1,
        }
    }
    Detection::ok(points, off_board)
}

/// The labeller of a target kind.
enum Labeller {
    Chessboard(chessboard::Chessboard),
    Charuco(Box<charuco::Charuco>),
    MarkerBoard(marker::MarkerBoard),
    PuzzleBoard(puzzleboard::PuzzleBoard),
    PuzzlePole(Box<pole::PuzzlePole>),
}

/// A ChESS corner is located at the Radon corner within this distance.
const LOCATE_PX: f32 = 1.0;
/// Radon's response floor, relative to the frame's strongest corner, when it
/// only locates corners ChESS found (its preset's 0.28 drops the corners of a
/// dimly lit part of the board, which ChESS's absolute floor keeps).
const LOCATE_THRESHOLD_REL: f32 = 0.05;

/// A target's detector: corners at chess-corners' Radon positions, labelled
/// by calib-targets' detector for the target's kind and matched to the
/// target's points (`etendue_synth::board::layout` order, as in `gt.json`).
///
/// A chessboard is labelled from Radon's corners alone (G4.2, P5-1b). The
/// other kinds' labellers are tuned on calib-targets' ChESS front end: its
/// absolute response floor and their corner-strength floors (on ChESS's
/// scale) keep marker bits and edges out of the grid, where Radon's
/// threshold, relative to the frame's strongest corner, lets a turned board's
/// edges through by the thousand. So ChESS finds those corners and Radon
/// locates them: each ChESS corner moves to the Radon corner within
/// `LOCATE_PX` (Radon run with the floor `LOCATE_THRESHOLD_REL`), keeping
/// ChESS's axes and strength, and one Radon does not confirm is dropped.
pub struct Detector {
    kind: &'static str,
    points: Vec<BoardPoint>,
    radon: DetectorConfig,
    /// The corner finder Radon locates, when it is not Radon itself.
    finder: Option<DetectorConfig>,
    labeller: Labeller,
}

impl Detector {
    /// The detector for a scene target.
    ///
    /// # Errors
    ///
    /// If the target has nothing to detect (a plain rectangle), is not one
    /// calib-targets prints (a `board` PuzzleBoard named by layout, a ring
    /// grid), or calib-targets rejects it.
    pub fn new(geometry: &TargetGeometry) -> Result<Self> {
        if matches!(geometry, TargetGeometry::Rectangle { .. }) {
            return Err(anyhow!(
                "the target is a plain rectangle: it has no features to detect"
            ));
        }
        let unsupported = || {
            anyhow!(
                "the target is not one calib-targets prints, so etendue detect has no \
                 detector for it"
            )
        };
        let spec = etendue_synth::board::printable(geometry)?.ok_or_else(unsupported)?;
        let layout = etendue_synth::board::layout(geometry)?.ok_or_else(unsupported)?;
        let points = layout.points.clone();
        let (kind, labeller) = match &spec {
            TargetSpec::Chessboard(_) => (
                "chessboard",
                Labeller::Chessboard(chessboard::Chessboard::new(layout)?),
            ),
            TargetSpec::Charuco(s) => (
                "charuco",
                Labeller::Charuco(Box::new(charuco::Charuco::new(s, &points)?)),
            ),
            TargetSpec::MarkerBoard(s) => (
                "marker_board",
                Labeller::MarkerBoard(marker::MarkerBoard::new(s, &points)?),
            ),
            TargetSpec::PuzzleBoard(s) => (
                "puzzle_board",
                Labeller::PuzzleBoard(puzzleboard::PuzzleBoard::new(s, &points)?),
            ),
            TargetSpec::PuzzlePole(s) => (
                "puzzlepole",
                Labeller::PuzzlePole(Box::new(pole::PuzzlePole::new(s, &points)?)),
            ),
            other => {
                return Err(anyhow!(
                    "etendue detect has no detector for a {} target",
                    other.kind_name()
                ));
            }
        };
        // chess-corners' Radon preset (G4.2). `detect_corners` reports a config
        // chess-corners rejects as "no corners" in every view, so check it here.
        let finder = (kind != "chessboard").then(default_chess_config);
        let radon = if finder.is_some() {
            DetectorConfig::radon().with_threshold(LOCATE_THRESHOLD_REL)
        } else {
            DetectorConfig::radon()
        };
        chess_corners::Detector::new(radon).map_err(|e| anyhow!("Radon detector config: {e}"))?;
        if let Some(c) = finder {
            chess_corners::Detector::new(c).map_err(|e| anyhow!("ChESS detector config: {e}"))?;
        }
        Ok(Self {
            kind,
            points,
            radon,
            finder,
            labeller,
        })
    }

    /// The target's kind, as a printed target names it.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        self.kind
    }

    /// The target's points, which [`Feature::point`] indexes.
    #[must_use]
    pub fn points(&self) -> &[BoardPoint] {
        &self.points
    }

    /// The corner detector and the labeller, for `features.json`.
    #[must_use]
    pub fn describe(&self) -> String {
        let labeller = match self.labeller {
            Labeller::Chessboard(_) => "calib-targets-chessboard labels, rotation by square colour",
            Labeller::Charuco(_) => "calib-targets-charuco labels, matched by ChArUco id",
            Labeller::MarkerBoard(_) => "calib-targets-marker labels, matched by grid label",
            Labeller::PuzzleBoard(_) => {
                "calib-targets-puzzleboard labels (fixed board), matched by master id"
            }
            Labeller::PuzzlePole(_) => "calib-targets-puzzleboard pole labels, matched by pole id",
        };
        let corners = if self.finder.is_some() {
            format!(
                "chess-corners ChESS (calib-targets default_chess_config) located by Radon \
                 (DetectorConfig::radon, threshold {LOCATE_THRESHOLD_REL}) within {LOCATE_PX} px"
            )
        } else {
            "chess-corners Radon (DetectorConfig::radon)".to_owned()
        };
        format!("{corners}, {labeller}")
    }

    /// Detect and label the target in one 8-bit image. Pixels are in
    /// chess-corners' convention (pixel `i` at coordinate `i`).
    #[must_use]
    pub fn detect(&self, gray: &[u8], width: usize, height: usize) -> Detection {
        let image = GrayImageView {
            width,
            height,
            data: gray,
        };
        let radon = detect_corners(&image, &self.radon);
        let corners = match &self.finder {
            Some(finder) => locate(&detect_corners(&image, finder), &radon),
            None => radon,
        };
        match &self.labeller {
            Labeller::Chessboard(l) => l.label(gray, width, height, &corners),
            Labeller::Charuco(l) => l.label(&image, &corners),
            Labeller::MarkerBoard(l) => l.label(&image, &corners),
            Labeller::PuzzleBoard(l) => l.label(&image, &corners),
            Labeller::PuzzlePole(l) => l.label(&image, &corners),
        }
    }
}

/// `found` at the positions of the `radon` corners within [`LOCATE_PX`]
/// (the nearest); those with none are dropped.
fn locate(found: &[ChessCorner], radon: &[ChessCorner]) -> Vec<ChessCorner> {
    found
        .iter()
        .filter_map(|c| {
            let near = radon
                .iter()
                .map(|r| (r, (r.position - c.position).norm()))
                .filter(|(_, d)| *d <= LOCATE_PX)
                .min_by(|a, b| a.1.total_cmp(&b.1))?;
            Some(ChessCorner::new(near.0.position, c.axes, c.strength))
        })
        .collect()
}

/// Check labelled features against a view's analytic pixels
/// (`truth[point]`, `None` for points behind the camera or facing away).
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

/// The target of the dataset in `dir`: `gt.json`'s, or for a dataset written
/// before `gt.json` recorded it, `dataset.json`'s board.
fn dataset_target(dir: &Path, gt: &GroundTruth) -> Result<TargetGeometry> {
    if let Some(geometry) = &gt.target.geometry {
        return Ok(geometry.clone());
    }
    let path = dir.join("dataset.json");
    let text = std::fs::read_to_string(&path).with_context(|| {
        format!(
            "gt.json names no target geometry; reading {} for it",
            path.display()
        )
    })?;
    let dataset: vision_calibration_dataset::DatasetSpec =
        serde_json::from_str(&text).context("parsing dataset.json")?;
    Ok(TargetGeometry::Board {
        board: dataset.target,
    })
}

/// Detect every image of the dataset in `dir` (`gt.json` from `etendue gt`,
/// images from `etendue render`) and write `<dir>/features.json`; with laser
/// images, also extract the stripes into `<dir>/laser_features.json`
/// ([`laser`]). Reports a [`Stage::Detect`] step per image.
///
/// # Errors
///
/// If a dataset file does not read or parse, the target has no detector (see
/// [`Detector::new`]) or is not `gt.json`'s, an image does not read, or the
/// run is cancelled ([`crate::progress::Cancelled`]).
/// The laser-stripe file [`run`] writes next to `features.json`.
pub const LASER_FEATURES: &str = "laser_features.json";

pub fn run(dir: &Path, ctl: &Control) -> Result<Features> {
    let path = dir.join("gt.json");
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let gt: GroundTruth = serde_json::from_str(&text).context("parsing gt.json")?;
    let detector = Detector::new(&dataset_target(dir, &gt)?)?;
    // Features index gt.json's points, which closed_loop.py reads: they must
    // be the layout's, in its order.
    if detector.points() != gt.target.points.as_slice() {
        return Err(anyhow!(
            "the dataset's target ({} points) is not gt.json's ({} points): \
             regenerate the dataset with `etendue gt`",
            detector.points().len(),
            gt.target.points.len()
        ));
    }
    // Detections are in chess-corners' convention (pixel `i` at `i`); shift
    // them into the dataset's.
    let offset = gt.pixel_centre.offset();

    let total: usize = gt.captures.iter().map(|c| c.views.len()).sum();
    let mut done = 0;
    let mut captures = Vec::with_capacity(gt.captures.len());
    for capture in &gt.captures {
        let mut views = Vec::with_capacity(capture.views.len());
        for view in &capture.views {
            ctl.check()?;
            ctl.step(Stage::Detect, done, total);
            done += 1;
            let path = dir.join(&view.image);
            if !path.is_file() {
                views.push(FeatureView {
                    camera: view.camera.clone(),
                    image: view.image.clone(),
                    status: ViewStatus::NoImage,
                    reason: None,
                    points: vec![],
                    check: Check::default(),
                });
                continue;
            }
            // Raw grayscale (`render --sensor`) or sRGB (`render`'s default).
            let image = read_png_luma8(&path)?;
            let mut found =
                detector.detect(&image.data, image.width as usize, image.height as usize);
            for p in &mut found.points {
                p.pixel = [p.pixel[0] + offset, p.pixel[1] + offset];
            }
            // A point on the far side of a pole projects among the near
            // side's: only points facing the camera can be what it saw.
            let mut truth = vec![None; gt.target.points.len()];
            for p in &view.points {
                if p.occluded != Some(Occlusion::BackFacing) {
                    truth[p.point] = p.pixel;
                }
            }
            let mut check = check(&found.points, &truth);
            check.off_board = found.off_board;
            views.push(FeatureView {
                camera: view.camera.clone(),
                image: view.image.clone(),
                status: found.status,
                reason: found.reason,
                points: found.points,
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
        kind: detector.kind().into(),
        detector: detector.describe(),
        captures,
    };
    ctl.step(Stage::Detect, total, total);
    let path = dir.join("features.json");
    std::fs::write(&path, serde_json::to_string_pretty(&features)? + "\n")
        .with_context(|| format!("writing {}", path.display()))?;
    // The laser stripes, when the dataset has laser images (P7-7).
    let path = dir.join(LASER_FEATURES);
    match laser::extract_dataset(dir, &gt)? {
        Some(stripes) => {
            for line in laser::summary_table(&stripes) {
                ctl.log(line);
            }
            std::fs::write(&path, serde_json::to_string_pretty(&stripes)? + "\n")
                .with_context(|| format!("writing {}", path.display()))?;
        }
        // A stale file would pair with the wrong ground truth.
        None if path.is_file() => std::fs::remove_file(&path)?,
        None => {}
    }
    Ok(features)
}

/// Detection results of one camera.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct CameraSummary {
    /// Camera id.
    pub camera: String,
    /// Views with an image.
    pub views: usize,
    /// Views by outcome.
    pub ok: usize,
    /// Views where no board was found.
    pub no_board: usize,
    /// Views where too little of the board was seen.
    pub partial: usize,
    /// Views whose rotation or position was not decided.
    pub ambiguous: usize,
    /// Views whose grid was found but not identified.
    pub undecoded: usize,
    /// Features found.
    pub points: usize,
    /// Features nearer another point's analytic pixel.
    pub mislabelled: usize,
    /// Features with no analytic pixel near.
    pub unmatched: usize,
    /// Labelled corners that name no point of the target.
    pub off_board: usize,
    /// RMS distance of matched features to their analytic pixels.
    pub rms_px: f64,
    /// Largest such distance.
    pub max_px: f64,
}

impl Features {
    /// Per camera, in dataset order: what was found. Cameras without images
    /// have `views == 0`.
    #[must_use]
    pub fn summary(&self) -> Vec<CameraSummary> {
        let Some(first) = self.captures.first() else {
            return vec![];
        };
        (0..first.views.len())
            .map(|i| {
                let views: Vec<&FeatureView> = self
                    .captures
                    .iter()
                    .filter_map(|c| c.views.get(i))
                    .filter(|v| v.status != ViewStatus::NoImage)
                    .collect();
                let count = |s: ViewStatus| views.iter().filter(|v| v.status == s).count();
                let matched: usize = views.iter().map(|v| v.check.matched).sum();
                let sq: f64 = views
                    .iter()
                    .map(|v| v.check.rms_px.powi(2) * v.check.matched as f64)
                    .sum();
                CameraSummary {
                    camera: first.views[i].camera.clone(),
                    views: views.len(),
                    ok: count(ViewStatus::Ok),
                    no_board: count(ViewStatus::NoBoard),
                    partial: count(ViewStatus::Partial),
                    ambiguous: count(ViewStatus::Ambiguous),
                    undecoded: count(ViewStatus::Undecoded),
                    points: views.iter().map(|v| v.points.len()).sum(),
                    mislabelled: views.iter().map(|v| v.check.mislabelled).sum(),
                    unmatched: views.iter().map(|v| v.check.unmatched).sum(),
                    off_board: views.iter().map(|v| v.check.off_board).sum(),
                    rms_px: (sq / matched.max(1) as f64).sqrt(),
                    max_px: views.iter().map(|v| v.check.max_px).fold(0.0, f64::max),
                }
            })
            .collect()
    }
}

/// [`Features::summary`] as a Markdown table, plus a warning line if any
/// label disagrees with the ground truth.
#[must_use]
pub fn summary_table(summary: &[CameraSummary]) -> Vec<String> {
    let mut lines = vec![
        "| camera | views ok | no board | partial | ambiguous | undecoded | points | mislabelled | unmatched | off board | RMS vs GT px | max px |".to_owned(),
        "|---|---|---|---|---|---|---|---|---|---|---|---|".to_owned(),
    ];
    for c in summary {
        lines.push(if c.views == 0 {
            format!("| {} | no images |", c.camera)
        } else {
            format!(
                "| {} | {}/{} | {} | {} | {} | {} | {} | {} | {} | {} | {:.4} | {:.4} |",
                c.camera,
                c.ok,
                c.views,
                c.no_board,
                c.partial,
                c.ambiguous,
                c.undecoded,
                c.points,
                c.mislabelled,
                c.unmatched,
                c.off_board,
                c.rms_px,
                c.max_px
            )
        });
    }
    let wrong: usize = summary
        .iter()
        .map(|c| c.mislabelled + c.unmatched + c.off_board)
        .sum();
    if wrong > 0 {
        lines.push(format!(
            "warning: {wrong} label(s) disagree with the ground truth; they are kept as detected"
        ));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn labels_off_the_board_are_counted_not_kept() {
        let index: HashMap<u32, usize> = [(7, 0), (9, 1)].into();
        let p = |x| [x, 0.0_f32];
        let d = labelled(
            &index,
            [
                (Some(9), p(1.0)),
                (Some(3), p(2.0)),
                (None, p(3.0)),
                (Some(7), p(4.0)),
            ],
        );
        assert_eq!(d.status, ViewStatus::Ok);
        assert_eq!(d.off_board, 2);
        let found: Vec<(usize, f64)> = d.points.iter().map(|f| (f.point, f.pixel[0])).collect();
        assert_eq!(found, [(0, 4.0), (1, 1.0)]);
    }

    #[test]
    fn a_rectangle_has_nothing_to_detect() {
        let err = Detector::new(&TargetGeometry::Rectangle {
            width: 0.2,
            height: 0.1,
        })
        .err()
        .unwrap();
        assert!(err.to_string().contains("rectangle"), "{err}");
    }

    #[test]
    fn old_features_files_still_read() {
        // P5-1b's features.json: no kind, no reason, no off_board.
        let text = r#"{"version": 1, "detector": "d", "captures": [{"id": "c0", "views": [
            {"camera": "a", "image": "i.png", "status": "ok", "points": [{"point": 0, "pixel": [1, 2]}],
             "check": {"matched": 1, "mislabelled": 0, "unmatched": 0, "rms_px": 0.1, "max_px": 0.1}}]}]}"#;
        let f: Features = serde_json::from_str(text).unwrap();
        assert_eq!(f.kind, "");
        assert_eq!(f.summary()[0].ok, 1);
    }
}
