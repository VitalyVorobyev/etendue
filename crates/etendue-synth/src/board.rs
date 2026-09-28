//! Board geometry from calib-targets (P3-3,
//! [ADR 0006](https://github.com/VitalyVorobyev/etendue/blob/main/docs/adrs/0006-ground-truth.md)).
//!
//! A board target is what `calib-targets-print` prints: its pattern comes from
//! [`calib_targets_print::board_primitives`] and its feature points from
//! [`calib_targets_print::TargetSpec::resolved_points`], so the drawn board and
//! the ground truth share one source.
//!
//! calib-targets works in board space (millimetres, origin at the board's
//! top-left corner, x right, y down), a frame whose right-handed z points into
//! the print. The target frame is centred on the board in the `z = 0` plane and
//! **faces +Z**, so the print reads correctly from in front of it: the print's
//! x is +X and its down is −Y. This module is the only place the two meet:
//!
//! `target = (x / 1000 − w / 2, h / 2 − y / 1000)`
//!
//! Mapping print-down to +Y instead would show the board mirrored from the
//! front: invisible on a chessboard, fatal to ArUco markers.

use calib_targets_charuco::{MarkerLayout, builtins::builtin_dictionary};
use calib_targets_print::{CharucoTargetSpec, ChessboardTargetSpec, Fill, Primitive, TargetSpec};
use etendue_scene::TargetGeometry;
use serde::{Deserialize, Serialize};
use vision_calibration_dataset::TargetSpec as Board;

use crate::gt::BoardPoint;
use crate::{Error, Result};

/// An axis-aligned patch of a board surface in the target frame (metres).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cell {
    /// `[x0, y0, x1, y1]` with `x0 < x1`, `y0 < y1`.
    pub rect: [f64; 4],
    /// Ink (`true`) or paper.
    pub dark: bool,
}

/// A board as calib-targets prints it, in the target frame.
#[derive(Clone, Debug, PartialEq)]
pub struct BoardLayout {
    /// Extent `[x, y]`, metres; the board spans `±size / 2`.
    pub size_m: [f64; 2],
    /// Non-overlapping patches that tile the board exactly, on one grid (every
    /// patch edge is a whole edge of its neighbour).
    pub cells: Vec<Cell>,
    /// Feature points (inner corners), with calib-targets' grid labels and ids.
    pub points: Vec<BoardPoint>,
}

/// The calib-targets spec of a scene board, or `None` for a target with no
/// printable source here (a plain rectangle, a puzzleboard until its named
/// layouts are mapped, a ring grid).
///
/// ChArUco boards use OpenCV's modern marker layout and the dictionary named
/// in the scene: the choices calibration-rs's ChArUco detector makes, so the
/// ids here are the ids it reports.
///
/// # Errors
///
/// [`Error::InvalidInput`] for an unknown ArUco dictionary.
pub fn printable(geometry: &TargetGeometry) -> Result<Option<TargetSpec>> {
    let TargetGeometry::Board { board } = geometry else {
        return Ok(None);
    };
    Ok(match board {
        Board::Chessboard {
            rows,
            cols,
            square_size_m,
        } => Some(TargetSpec::Chessboard(ChessboardTargetSpec::new(
            *rows,
            *cols,
            square_size_m * 1000.0,
        ))),
        Board::Charuco {
            rows,
            cols,
            square_size_m,
            marker_size_m,
            dictionary,
        } => {
            let dict = builtin_dictionary(dictionary).ok_or_else(|| {
                Error::InvalidInput(format!("unknown ArUco dictionary `{dictionary}`"))
            })?;
            Some(TargetSpec::Charuco(
                CharucoTargetSpec::new(
                    *rows,
                    *cols,
                    square_size_m * 1000.0,
                    marker_size_m / square_size_m,
                    dict,
                )
                .with_marker_layout(MarkerLayout::OpenCvCharuco),
            ))
        }
        _ => None,
    })
}

/// The layout of a scene board: its patches and feature points in the target
/// frame. `None` where [`printable`] is `None`.
///
/// # Errors
///
/// [`Error::InvalidInput`] if calib-targets rejects the board, or draws it with
/// a shape other than rectangles (no chessboard or ChArUco board does).
pub fn layout(geometry: &TargetGeometry) -> Result<Option<BoardLayout>> {
    let Some(spec) = printable(geometry)? else {
        return Ok(None);
    };
    let invalid =
        |e: calib_targets_print::PrintableTargetError| Error::InvalidInput(format!("board: {e}"));
    let (w_mm, h_mm) = spec.board_size_mm().map_err(invalid)?;
    let size_m = [w_mm / 1000.0, h_mm / 1000.0];
    let to_target = |[x, y]: [f64; 2]| [x / 1000.0 - size_m[0] / 2.0, size_m[1] / 2.0 - y / 1000.0];

    let shapes = calib_targets_print::board_primitives(&spec)
        .map_err(invalid)?
        .iter()
        .map(Shape::from_primitive)
        .collect::<Result<Vec<_>>>()?;
    let cells = tile([w_mm, h_mm], &shapes)
        .into_iter()
        .map(|(rect, dark)| {
            // The print's top edge (smaller y) becomes the cell's larger Y.
            let [x0, y1] = to_target([rect[0], rect[1]]);
            let [x1, y0] = to_target([rect[2], rect[3]]);
            Cell {
                rect: [x0, y0, x1, y1],
                dark,
            }
        })
        .collect();
    let points = spec
        .resolved_points()
        .map_err(invalid)?
        .into_iter()
        .map(|p| BoardPoint {
            position_m: to_target(p.position_mm),
            grid: p.grid.map(|g| [g.u, g.v]),
            id: p.id,
        })
        .collect();
    Ok(Some(BoardLayout {
        size_m,
        cells,
        points,
    }))
}

/// A painted rectangle in board millimetres, with an optional paper hole.
struct Shape {
    rect: [f64; 4],
    hole: Option<[f64; 4]>,
    dark: bool,
}

impl Shape {
    fn from_primitive(p: &Primitive) -> Result<Self> {
        let dark = |fill: &Fill| match fill {
            Fill::Black => Ok(true),
            Fill::White => Ok(false),
            other => Err(Error::InvalidInput(format!(
                "board: unexpected annotation fill {other:?}"
            ))),
        };
        match p {
            Primitive::Rect {
                x_mm,
                y_mm,
                width_mm,
                height_mm,
                fill,
            } => Ok(Self {
                rect: [*x_mm, *y_mm, x_mm + width_mm, y_mm + height_mm],
                hole: None,
                dark: dark(fill)?,
            }),
            Primitive::RectWithHole {
                x_mm,
                y_mm,
                width_mm,
                height_mm,
                hole_x_mm,
                hole_y_mm,
                hole_width_mm,
                hole_height_mm,
                fill,
            } => Ok(Self {
                rect: [*x_mm, *y_mm, x_mm + width_mm, y_mm + height_mm],
                hole: Some([
                    *hole_x_mm,
                    *hole_y_mm,
                    hole_x_mm + hole_width_mm,
                    hole_y_mm + hole_height_mm,
                ]),
                dark: dark(fill)?,
            }),
            other => Err(Error::InvalidInput(format!(
                "board: shape {other:?} cannot be tiled into rectangles"
            ))),
        }
    }

    /// Paint at `(x, y)`: `None` outside, else ink or paper.
    fn paint(&self, x: f64, y: f64) -> Option<bool> {
        let inside = |[x0, y0, x1, y1]: [f64; 4]| x0 < x && x < x1 && y0 < y && y < y1;
        if !inside(self.rect) {
            None
        } else if self.hole.is_some_and(inside) {
            Some(false)
        } else {
            Some(self.dark)
        }
    }
}

/// Tile `[0, w] × [0, h]` into non-overlapping rectangles coloured as the
/// shapes paint it (later shapes on top, paper where none paints). The cells
/// are the grid of every shape edge, unmerged: neighbours share whole edges, so
/// a mesh built from them has no T-junctions for a ray to slip through.
fn tile([w, h]: [f64; 2], shapes: &[Shape]) -> Vec<([f64; 4], bool)> {
    let cuts = |pick: fn(&[f64; 4]) -> [f64; 2], extent: f64| {
        let mut v: Vec<f64> = shapes
            .iter()
            .flat_map(|s| {
                let mut e = pick(&s.rect).to_vec();
                if let Some(hole) = &s.hole {
                    e.extend(pick(hole));
                }
                e
            })
            .chain([0.0, extent])
            .filter(|c| (0.0..=extent).contains(c))
            .collect();
        v.sort_by(f64::total_cmp);
        // Edges computed along different paths can differ in the last bits.
        v.dedup_by(|a, b| (*a - *b).abs() <= 1e-9 * extent);
        v
    };
    let xs = cuts(|r| [r[0], r[2]], w);
    let ys = cuts(|r| [r[1], r[3]], h);
    let mut out = Vec::new();
    for yw in ys.windows(2) {
        let yc = (yw[0] + yw[1]) / 2.0;
        for xw in xs.windows(2) {
            let xc = (xw[0] + xw[1]) / 2.0;
            let dark = shapes
                .iter()
                .rev()
                .find_map(|s| s.paint(xc, yc))
                .unwrap_or(false);
            out.push(([xw[0], yw[0], xw[1], yw[1]], dark));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chessboard() -> TargetGeometry {
        TargetGeometry::Board {
            board: Board::Chessboard {
                rows: 6,
                cols: 9,
                square_size_m: 0.02,
            },
        }
    }

    fn charuco() -> TargetGeometry {
        TargetGeometry::Board {
            board: Board::Charuco {
                rows: 5,
                cols: 7,
                square_size_m: 0.03,
                marker_size_m: 0.0225,
                dictionary: "DICT_4X4_50".into(),
            },
        }
    }

    /// Ink or paper of the layout at `(x, y)`, from its cells.
    fn dark_at(l: &BoardLayout, x: f64, y: f64) -> bool {
        let hits: Vec<bool> = l
            .cells
            .iter()
            .filter(|c| c.rect[0] <= x && x < c.rect[2] && c.rect[1] <= y && y < c.rect[3])
            .map(|c| c.dark)
            .collect();
        assert_eq!(hits.len(), 1, "({x}, {y}) is in {} cells", hits.len());
        hits[0]
    }

    #[test]
    fn cells_tile_the_board_at_the_scene_extent() {
        for g in [chessboard(), charuco()] {
            let l = layout(&g).unwrap().unwrap();
            // The viewers draw the target at `extent_m`; the print must agree.
            let extent = g.extent_m().unwrap();
            assert!((l.size_m[0] - extent[0]).abs() < 1e-12);
            assert!((l.size_m[1] - extent[1]).abs() < 1e-12);
            let area: f64 = l
                .cells
                .iter()
                .map(|c| (c.rect[2] - c.rect[0]) * (c.rect[3] - c.rect[1]))
                .sum();
            assert!((area - extent[0] * extent[1]).abs() < 1e-12);
            // Non-overlap: a grid of probes lands in exactly one cell each.
            for i in 0..97 {
                for j in 0..83 {
                    let x = -extent[0] / 2.0 + extent[0] * (f64::from(i) + 0.5) / 97.0;
                    let y = -extent[1] / 2.0 + extent[1] * (f64::from(j) + 0.5) / 83.0;
                    dark_at(&l, x, y);
                }
            }
        }
    }

    #[test]
    fn chessboard_points_run_from_the_print_top_left() {
        // calib-targets' order: row by row from the print's top-left, which is
        // −X/+Y in the target frame; `grid = [column, row]`. It numbers only
        // ChArUco corners, so a chessboard point has a grid label and no id.
        let board = TargetGeometry::Board {
            board: Board::Chessboard {
                rows: 6,
                cols: 9,
                square_size_m: 0.025,
            },
        };
        let points = layout(&board).unwrap().unwrap().points;
        assert_eq!(points.len(), 54);
        let close =
            |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).abs() < 1e-12 && (a[1] - b[1]).abs() < 1e-12;
        assert!(close(points[0].position_m, [-0.1, 0.0625]));
        assert!(close(points[53].position_m, [0.1, -0.0625]));
        for (k, p) in points.iter().enumerate() {
            let (c, r) = (k % 9, k / 9);
            assert_eq!(p.grid, Some([c as i32, r as i32]));
            assert_eq!(p.id, None);
        }
    }

    #[test]
    fn the_print_top_left_square_is_dark_at_minus_x_plus_y() {
        let l = layout(&chessboard()).unwrap().unwrap();
        let [w, h] = l.size_m;
        assert!(dark_at(&l, -w / 2.0 + 0.001, h / 2.0 - 0.001));
        assert!(!dark_at(&l, -w / 2.0 + 0.021, h / 2.0 - 0.001));
    }

    #[test]
    fn the_board_faces_plus_z() {
        // Seen from +Z the print is not mirrored: its x runs along +X and its
        // down along −Y, so print-x × print-down (the print's z, into the paper)
        // is −Z.
        let l = layout(&chessboard()).unwrap().unwrap();
        let at = |g: [i32; 2]| {
            l.points
                .iter()
                .find(|p| p.grid == Some(g))
                .unwrap()
                .position_m
        };
        let (o, x, down) = (at([0, 0]), at([1, 0]), at([0, 1]));
        let (ax, ay) = (x[0] - o[0], x[1] - o[1]);
        let (bx, by) = (down[0] - o[0], down[1] - o[1]);
        assert!(ax > 0.0 && ay.abs() < 1e-12, "print x is +X");
        assert!(by < 0.0 && bx.abs() < 1e-12, "print down is −Y");
        assert!(ax * by - ay * bx < 0.0, "print z is −Z");
    }

    #[test]
    fn every_point_is_a_corner_of_the_drawn_squares() {
        // ADR 0006: the points are checked against the drawn board. Around each
        // point the four squares alternate: equal along diagonals, not across.
        for g in [chessboard(), charuco()] {
            let l = layout(&g).unwrap().unwrap();
            assert!(!l.points.is_empty());
            let d = 1e-4;
            for p in &l.points {
                let [x, y] = p.position_m;
                let (a, b) = (dark_at(&l, x - d, y - d), dark_at(&l, x + d, y - d));
                let (c, e) = (dark_at(&l, x - d, y + d), dark_at(&l, x + d, y + d));
                assert!(a == e && b == c && a != b, "{p:?} is not a checker corner");
            }
        }
    }

    #[test]
    fn charuco_draws_markers_and_numbers_corners() {
        let l = layout(&charuco()).unwrap().unwrap();
        // 7 × 5 squares: 6 × 4 inner corners with ids 0..24.
        let ids: Vec<u32> = l.points.iter().filter_map(|p| p.id).collect();
        assert_eq!(ids, (0..24).collect::<Vec<_>>());
        // A marker is ink inside a paper square: the square at column 1 of the
        // print's top row (at +Y) carries marker 0, so it is not plain paper.
        let [w, h] = l.size_m;
        let s = 0.03;
        let (x0, y0) = (-w / 2.0 + s, h / 2.0 - s);
        let dark = (0..30)
            .flat_map(|i| (0..30).map(move |j| (i, j)))
            .filter(|&(i, j)| {
                dark_at(
                    &l,
                    x0 + s * (f64::from(i) + 0.5) / 30.0,
                    y0 + s * (f64::from(j) + 0.5) / 30.0,
                )
            })
            .count();
        assert!(
            dark > 100 && dark < 800,
            "{dark} dark probes in a marker square"
        );
        assert!(
            l.cells.len() > 200,
            "markers split the board into many cells"
        );
    }

    #[test]
    fn other_targets_have_no_layout_and_bad_dictionaries_fail() {
        let plain = TargetGeometry::Rectangle {
            width: 0.2,
            height: 0.1,
        };
        assert!(layout(&plain).unwrap().is_none());
        let bad = TargetGeometry::Board {
            board: Board::Charuco {
                rows: 5,
                cols: 7,
                square_size_m: 0.03,
                marker_size_m: 0.0225,
                dictionary: "DICT_NOPE".into(),
            },
        };
        assert!(layout(&bad).is_err());
    }
}
