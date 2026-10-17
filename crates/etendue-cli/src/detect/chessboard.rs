//! Chessboards: calib-targets' chessboard labeller, the rotation decided by
//! square colour.
//!
//! calib-targets labels a plain chessboard only up to the view: the lowest
//! label is `(0, 0)` and `u` runs along the image's +x, with no board size and
//! no colour. A view is kept only when the labels span the whole board, which
//! leaves two rotations; the colour of the squares between the corners (the
//! print's, `etendue_synth::board`) decides between them. A board that looks
//! the same turned by 180° cannot be decided and its views are dropped.

use std::collections::HashMap;

use anyhow::{Result, anyhow};
use calib_targets_chessboard::{ChessCorner, ChessboardDetector, ChessboardParams};
use etendue_synth::board::BoardLayout;

use super::{Detection, Feature, ViewStatus};

/// Fraction of the squares whose colour must agree with the chosen rotation.
const PARITY_AGREEMENT: f64 = 0.9;
/// The two square classes' mean intensities must differ by at least this
/// fraction of the brighter one for the colours to decide anything.
const MIN_CONTRAST: f64 = 0.2;

/// The board as the labeller needs it: points by grid label, and the print.
pub(super) struct Chessboard {
    layout: BoardLayout,
    /// Inner corners `[columns, rows]`.
    size: [i32; 2],
    by_grid: HashMap<[i32; 2], usize>,
    labeller: ChessboardDetector,
}

impl Chessboard {
    /// A chessboard layout (points with grid labels).
    ///
    /// # Errors
    ///
    /// If the layout's points carry no grid labels.
    pub(super) fn new(layout: BoardLayout) -> Result<Self> {
        let mut by_grid = HashMap::new();
        let mut size = [0, 0];
        for (i, p) in layout.points.iter().enumerate() {
            let g = p
                .grid
                .ok_or_else(|| anyhow!("board point {i} has no grid label"))?;
            by_grid.insert(g, i);
            size = [size[0].max(g[0] + 1), size[1].max(g[1] + 1)];
        }
        let labeller = ChessboardDetector::new(ChessboardParams::default())
            .map_err(|e| anyhow!("chessboard params: {e}"))?;
        Ok(Self {
            layout,
            size,
            by_grid,
            labeller,
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

    /// Label the corners of one 8-bit image.
    pub(super) fn label(
        &self,
        gray: &[u8],
        width: usize,
        height: usize,
        corners: &[ChessCorner],
    ) -> Detection {
        let Some(detection) = self.labeller.detect(corners) else {
            return Detection::failed(ViewStatus::NoBoard, "no chessboard grid");
        };
        // calib-targets rebases the labels to start at (0, 0).
        let labels: HashMap<[i32; 2], [f64; 2]> = detection
            .corners
            .iter()
            .map(|c| {
                (
                    [c.grid.u, c.grid.v],
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
            return Detection::failed(ViewStatus::Partial, "no whole square labelled");
        }
        // The squares alternate, so the squares with even `u + v` are one colour
        // and the odd ones the other. Comparing the two classes' mean intensities
        // needs no global threshold, so an illumination gradient or vignetting
        // across the board does not flip squares.
        let mean = |parity: i32| {
            let class: Vec<f64> = quads
                .iter()
                .filter(|(_, [u, v])| (u + v).rem_euclid(2) == parity)
                .map(|(g, _)| *g)
                .collect();
            class.iter().sum::<f64>() / class.len().max(1) as f64
        };
        let (even, odd) = (mean(0), mean(1));
        if (even - odd).abs() < MIN_CONTRAST * even.max(odd) {
            return Detection::failed(ViewStatus::Ambiguous, "the square colours do not separate");
        }
        let dark_parity = if even < odd { 0 } else { 1 };

        // Rotations that fit the board, scored by square colour.
        let mut fits = Vec::new();
        for (map, span) in rotations(w, h) {
            if span != self.size {
                continue;
            }
            let agree = quads
                .iter()
                .filter(|(_, [u, v])| {
                    // The square's centre on the print: the mean of its corners'.
                    let corners = [[*u, *v], [u + 1, *v], [*u, v + 1], [u + 1, v + 1]];
                    let mut centre = [0.0; 2];
                    for [a, b] in corners {
                        let p = self.layout.points[self.by_grid[&map(a, b, w, h)]].position_m;
                        centre = [centre[0] + p[0] / 4.0, centre[1] + p[1] / 4.0];
                    }
                    self.dark_at(centre[0], centre[1]) == Some((u + v).rem_euclid(2) == dark_parity)
                })
                .count();
            fits.push((agree, map));
        }
        if fits.is_empty() {
            return Detection::failed(
                ViewStatus::Partial,
                format!(
                    "labels span {w} × {h} corners, the board has {} × {}",
                    self.size[0], self.size[1]
                ),
            );
        }
        fits.sort_by_key(|f| std::cmp::Reverse(f.0));
        let (best, map) = fits[0];
        let decided = best as f64 >= PARITY_AGREEMENT * quads.len() as f64
            && fits.get(1).is_none_or(|(second, _)| *second < best);
        if !decided {
            return Detection::failed(
                ViewStatus::Ambiguous,
                "the square colours do not decide the rotation",
            );
        }
        let points: Vec<Feature> = detection
            .corners
            .iter()
            .map(|c| Feature {
                point: self.by_grid[&map(c.grid.u, c.grid.v, w, h)],
                pixel: [f64::from(c.position.x), f64::from(c.position.y)],
            })
            .collect();
        Detection::ok(points, 0)
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
