//! Board geometry from calib-targets (P3-3, P7-4,
//! [ADR 0006](https://github.com/VitalyVorobyev/etendue/blob/main/docs/adrs/0006-ground-truth.md)).
//!
//! A board target is what `calib-targets-print` prints: its pattern comes from
//! [`calib_targets_print::board_primitives`] and its feature points from
//! [`calib_targets_print::TargetSpec::resolved_points`], so the drawn board and
//! the ground truth share one source.
//!
//! calib-targets works in board space (millimetres, origin at the board's
//! top-left corner, x right, y down), a frame whose right-handed z points into
//! the print. A planar target frame is centred on the board in the `z = 0`
//! plane and **faces +Z**, so the print reads correctly from in front of it:
//! the print's x is +X and its down is −Y. This module is the only place the
//! two meet:
//!
//! `target = (x / 1000 − w / 2, h / 2 − y / 1000)`
//!
//! Mapping print-down to +Y instead would show the board mirrored from the
//! front: invisible on a chessboard, fatal to ArUco markers.
//!
//! A PuzzlePole's print is a strip wrapped around a cylinder: its surface
//! coordinates are `(u, v) = (x, y) / 1000` (axial, arc), over the seamless
//! window `v ∈ [s/2, s/2 + C)` (`s` the square, `C` the circumference), and
//! [`Surface::Cylinder`] maps them into calib-targets' pole object frame with
//! `θ = 2π v / C`, so the ink faces outward.

use calib_targets_charuco::{MarkerLayout, builtins::builtin_dictionary};
use calib_targets_print::{CharucoTargetSpec, ChessboardTargetSpec, Fill, Primitive, TargetSpec};
use etendue_scene::{PrintedTarget, TargetGeometry};
use serde::{Deserialize, Serialize};
use vision_calibration_dataset::TargetSpec as Board;

use crate::gt::BoardPoint;
use crate::{Error, Result};

/// An axis-aligned patch of a board surface, in surface coordinates (metres;
/// the target frame's `x, y` for a planar target).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cell {
    /// `[x0, y0, x1, y1]` with `x0 < x1`, `y0 < y1`.
    pub rect: [f64; 4],
    /// Ink (`true`) or paper.
    pub dark: bool,
}

/// A disk painted over the cells (a PuzzleBoard dot, a marker board circle),
/// in surface coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Disc {
    /// Centre, metres.
    pub center: [f64; 2],
    /// Radius, metres.
    pub radius: f64,
    /// Ink (`true`) or paper.
    pub dark: bool,
}

/// The shape a board's surface coordinates lie on.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Surface {
    /// The target frame's `z = 0` plane: `(x, y) ↦ (x, y, 0)`, facing +Z.
    Plane,
    /// A cylinder about the target frame's Z axis:
    /// `(u, v) ↦ (r cos θ, r sin θ, u)` with `θ = 2π v / circumference`,
    /// facing outward.
    Cylinder {
        /// Radius, metres.
        radius_m: f64,
        /// Circumference, metres.
        circumference_m: f64,
    },
}

impl Surface {
    /// Position and outward normal at surface coordinates `(u, v)`.
    #[must_use]
    pub fn at(&self, [u, v]: [f64; 2]) -> ([f64; 3], [f64; 3]) {
        match *self {
            Self::Plane => ([u, v, 0.0], [0.0, 0.0, 1.0]),
            Self::Cylinder {
                radius_m,
                circumference_m,
            } => {
                let theta = std::f64::consts::TAU * v / circumference_m;
                let (s, c) = theta.sin_cos();
                ([radius_m * c, radius_m * s, u], [c, s, 0.0])
            }
        }
    }
}

/// A board as calib-targets prints it, on its surface.
#[derive(Clone, Debug, PartialEq)]
pub struct BoardLayout {
    /// The surface its coordinates lie on.
    pub surface: Surface,
    /// Extent `[x, y]` of a planar board, metres (it spans `±size / 2`); a
    /// PuzzlePole's `[length, circumference]`.
    pub size_m: [f64; 2],
    /// Non-overlapping patches that tile the surface exactly, on one grid
    /// (every patch edge is a whole edge of its neighbour), in surface
    /// coordinates.
    pub cells: Vec<Cell>,
    /// Disks painted over the cells, in paint order.
    pub discs: Vec<Disc>,
    /// Feature points (inner corners), with calib-targets' grid labels and ids.
    pub points: Vec<BoardPoint>,
}

/// The calib-targets spec of a scene target, or `None` for a target with no
/// printable source here (a plain rectangle, a `board` puzzleboard named by
/// layout, a ring grid).
///
/// A `printed` target goes to calib-targets as written. A `board` ChArUco
/// board uses OpenCV's modern marker layout and the dictionary named in the
/// scene: the choices calibration-rs's ChArUco detector makes, so the ids here
/// are the ids it reports.
///
/// # Errors
///
/// [`Error::InvalidInput`] for an unknown ArUco dictionary, or a printed
/// target calib-targets does not read.
pub fn printable(geometry: &TargetGeometry) -> Result<Option<TargetSpec>> {
    let board = match geometry {
        TargetGeometry::Printed { target } => return to_calib_targets(target).map(Some),
        TargetGeometry::Board { board } => board,
        TargetGeometry::Rectangle { .. } => return Ok(None),
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

/// A printed target as calib-targets' own type: the two are the same JSON.
///
/// # Errors
///
/// [`Error::InvalidInput`] if calib-targets does not read it (an unknown
/// ArUco dictionary, say).
pub fn to_calib_targets(target: &PrintedTarget) -> Result<TargetSpec> {
    let json = serde_json::to_value(target).map_err(|e| Error::InvalidInput(e.to_string()))?;
    serde_json::from_value(json).map_err(|e| Error::InvalidInput(format!("target: {e}")))
}

/// The layout of a scene target: its patches, disks and feature points on
/// its surface. `None` where [`printable`] is `None`.
///
/// # Errors
///
/// [`Error::InvalidInput`] if calib-targets rejects the target, or paints a
/// rectangle over a disk (no calib-targets target does).
pub fn layout(geometry: &TargetGeometry) -> Result<Option<BoardLayout>> {
    let Some(spec) = printable(geometry)? else {
        return Ok(None);
    };
    let invalid =
        |e: calib_targets_print::PrintableTargetError| Error::InvalidInput(format!("board: {e}"));
    let (w_mm, h_mm) = spec.board_size_mm().map_err(invalid)?;
    let primitives = calib_targets_print::board_primitives(&spec).map_err(invalid)?;
    let mut shapes = Vec::new();
    let mut circles = Vec::new();
    for p in &primitives {
        match p {
            Primitive::Circle {
                cx_mm,
                cy_mm,
                radius_mm,
                fill,
            } => circles.push(([*cx_mm, *cy_mm], *radius_mm, dark(fill)?)),
            other if circles.is_empty() => shapes.push(Shape::from_primitive(other)?),
            _ => {
                return Err(Error::InvalidInput(
                    "board: a rectangle painted over a disk cannot be tiled".into(),
                ));
            }
        }
    }
    let resolved = spec.resolved_points().map_err(invalid)?;

    if let TargetSpec::PuzzlePole(pole) = &spec {
        // The seamless window of the wrapped strip, in print millimetres.
        let s = pole.square_size_mm;
        let c_mm = f64::from(pole.circumference_squares) * s;
        let (v0, v1) = (s / 2.0, s / 2.0 + c_mm);
        let surface = Surface::Cylinder {
            radius_m: c_mm / 1000.0 / std::f64::consts::TAU,
            circumference_m: c_mm / 1000.0,
        };
        let cells = tile([w_mm, h_mm], &shapes, &[v0, v1])
            .into_iter()
            .filter(|(r, _)| r[1] >= v0 - 1e-9 && r[3] <= v1 + 1e-9)
            .map(|(r, dark)| Cell {
                rect: r.map(|c| c / 1000.0),
                dark,
            })
            .collect();
        // A disk on the window's far edge is the near edge's, again.
        let discs = circles
            .into_iter()
            .filter(|(c, _, _)| c[1] >= v0 - 1e-9 && c[1] < v1 - 1e-9)
            .map(|(c, r, dark)| Disc {
                center: c.map(|x| x / 1000.0),
                radius: r / 1000.0,
                dark,
            })
            .collect();
        let points = resolved
            .into_iter()
            .map(|p| {
                let (xyz, normal) = surface.at(p.position_mm.map(|x| x / 1000.0));
                let [axial, cyclic] = p.grid.map_or([0, 0], |g| [g.u, g.v]);
                BoardPoint {
                    // calib-targets' surface position: (z, arc from the seam).
                    position_m: [
                        f64::from(axial) * s / 1000.0,
                        f64::from(cyclic) * s / 1000.0,
                    ],
                    xyz_m: Some(xyz),
                    normal: Some(normal),
                    grid: p.grid.map(|g| [g.u, g.v]),
                    id: p.id,
                }
            })
            .collect();
        return Ok(Some(BoardLayout {
            surface,
            size_m: [w_mm / 1000.0, c_mm / 1000.0],
            cells,
            discs,
            points,
        }));
    }

    let size_m = [w_mm / 1000.0, h_mm / 1000.0];
    let to_target = |[x, y]: [f64; 2]| [x / 1000.0 - size_m[0] / 2.0, size_m[1] / 2.0 - y / 1000.0];
    let cells = tile([w_mm, h_mm], &shapes, &[])
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
    let discs = circles
        .into_iter()
        .map(|(c, r, dark)| Disc {
            center: to_target(c),
            radius: r / 1000.0,
            dark,
        })
        .collect();
    let points = resolved
        .into_iter()
        .map(|p| BoardPoint::planar(to_target(p.position_mm), p.grid.map(|g| [g.u, g.v]), p.id))
        .collect();
    Ok(Some(BoardLayout {
        surface: Surface::Plane,
        size_m,
        cells,
        discs,
        points,
    }))
}

/// The PuzzlePole circumferences calib-targets supports: `(squares,
/// start_row)` pairs, a printed pole's `circumference_squares` and
/// `start_row`.
#[must_use]
pub fn pole_periods() -> Vec<(u32, u32)> {
    calib_targets_puzzleboard::SUPPORTED_PERIODS
        .iter()
        .map(|p| (p.squares, p.start_row))
        .collect()
}

/// Ink (`true`) or paper, for a board pattern's fill.
fn dark(fill: &Fill) -> Result<bool> {
    match fill {
        Fill::Black => Ok(true),
        Fill::White => Ok(false),
        other => Err(Error::InvalidInput(format!(
            "board: unexpected annotation fill {other:?}"
        ))),
    }
}

/// A painted rectangle in board millimetres, with an optional paper hole.
struct Shape {
    rect: [f64; 4],
    hole: Option<[f64; 4]>,
    dark: bool,
}

impl Shape {
    fn from_primitive(p: &Primitive) -> Result<Self> {
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
/// are the grid of every shape edge (and of `y_cuts`), unmerged: neighbours
/// share whole edges, so a mesh built from them has no T-junctions for a ray
/// to slip through.
fn tile([w, h]: [f64; 2], shapes: &[Shape], y_cuts: &[f64]) -> Vec<([f64; 4], bool)> {
    let cuts = |pick: fn(&[f64; 4]) -> [f64; 2], extent: f64, extra: &[f64]| {
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
            .chain(extra.iter().copied())
            .filter(|c| (0.0..=extent).contains(c))
            .collect();
        v.sort_by(f64::total_cmp);
        // Edges computed along different paths can differ in the last bits.
        v.dedup_by(|a, b| (*a - *b).abs() <= 1e-9 * extent);
        v
    };
    let xs = cuts(|r| [r[0], r[2]], w, &[]);
    let ys = cuts(|r| [r[1], r[3]], h, y_cuts);
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
