//! Printable calibration targets: every target calib-targets prints (P7-4).
//!
//! [`PrintedTarget`] mirrors `calib_targets_print::TargetSpec` field for
//! field and on the wire (tag `kind`, millimetres, the same defaults), so a
//! scene names a target exactly as `calib-targets-print` does and its JSON
//! goes to calib-targets unchanged (`etendue-synth` converts, and a test there
//! checks the two stay wire-compatible). The mirror lets this crate keep its
//! dependency list and JSON Schema of its own.

use serde::{Deserialize, Serialize};

use crate::validate::Issues;

fn default_border_bits() -> usize {
    1
}

fn default_circle_diameter_rel() -> f64 {
    0.5
}

fn default_dot_diameter_rel() -> f64 {
    1.0 / 3.0
}

/// A target as calib-targets prints it, tagged on `kind`. Sizes are in
/// millimetres, as on the print.
///
/// Planar targets lie in the target frame's `z = 0` plane, centred on its
/// origin, facing +Z, the print's top edge at +Y. A PuzzlePole is a cylinder:
/// its frame is calib-targets' pole object frame (axis +Z, origin on the axis
/// in the plane of axial corner 0, the seam at +X).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PrintedTarget {
    /// A chessboard.
    Chessboard {
        /// Inner corner rows.
        inner_rows: u32,
        /// Inner corner columns.
        inner_cols: u32,
        /// Square side, mm.
        square_size_mm: f64,
        /// White inset in the black squares, as a fraction of the side.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        inner_square_rel: Option<f64>,
    },
    /// A ChArUco board.
    Charuco {
        /// Square rows.
        rows: u32,
        /// Square columns.
        cols: u32,
        /// Square side, mm.
        square_size_mm: f64,
        /// Marker side as a fraction of the square side.
        marker_size_rel: f64,
        /// ArUco dictionary, e.g. `"DICT_4X4_50"`.
        dictionary: String,
        /// Marker placement; `opencv_charuco` (OpenCV's modern layout).
        #[serde(default)]
        marker_layout: CharucoLayout,
        /// Marker border width in bits.
        #[serde(default = "default_border_bits")]
        border_bits: usize,
        /// White inset in the plain black squares, as a fraction of the side.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        inner_square_rel: Option<f64>,
    },
    /// A chessboard with three circles that fix its origin and orientation.
    MarkerBoard {
        /// Inner corner rows.
        inner_rows: u32,
        /// Inner corner columns.
        inner_cols: u32,
        /// Square side, mm.
        square_size_mm: f64,
        /// The three circles.
        circles: [MarkerCircle; 3],
        /// Circle diameter as a fraction of the square side.
        #[serde(default = "default_circle_diameter_rel")]
        circle_diameter_rel: f64,
        /// White inset in the black squares, as a fraction of the side.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        inner_square_rel: Option<f64>,
    },
    /// A PuzzleBoard: a chessboard whose edge dots make every corner
    /// self-identifying.
    PuzzleBoard {
        /// Square rows.
        rows: u32,
        /// Square columns.
        cols: u32,
        /// Square side, mm.
        square_size_mm: f64,
        /// Master row of the board's first square.
        #[serde(default)]
        origin_row: u32,
        /// Master column of the board's first square.
        #[serde(default)]
        origin_col: u32,
        /// Edge-dot diameter as a fraction of the square side.
        #[serde(default = "default_dot_diameter_rel")]
        dot_diameter_rel: f64,
    },
    /// A PuzzlePole: a PuzzleBoard strip wrapped around a cylinder of
    /// circumference `circumference_squares × square_size_mm`.
    #[serde(rename = "puzzlepole")]
    PuzzlePole {
        /// Squares around the circumference: a supported period (12, 18,
        /// 24, 30, 36 or 42).
        circumference_squares: u32,
        /// Master row the strip is cut from (the seam); must match the
        /// period (see calib-targets `PuzzlePolePeriod`).
        start_row: u32,
        /// Master column the strip is cut from.
        #[serde(default)]
        axial_start_col: u32,
        /// Squares along the cylinder axis.
        axial_squares: u32,
        /// Square side, mm.
        square_size_mm: f64,
        /// Edge-dot diameter as a fraction of the square side.
        #[serde(default = "default_dot_diameter_rel")]
        dot_diameter_rel: f64,
    },
}

/// ChArUco marker placement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum CharucoLayout {
    /// OpenCV's modern layout: markers on the white squares, numbered row by
    /// row.
    #[default]
    OpencvCharuco,
}

/// A marker board circle: on square `(i, j)` (column, row), white or black.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct MarkerCircle {
    /// Square column.
    pub i: u32,
    /// Square row.
    pub j: u32,
    /// Disk colour.
    pub polarity: Polarity,
}

/// A disk's colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Polarity {
    /// A white disk on a dark square.
    White,
    /// A black disk on a light square.
    Black,
}

impl PrintedTarget {
    /// The printed board's extent `[x, y]` in metres, for a planar target;
    /// `None` for a PuzzlePole (a cylinder: see [`PrintedTarget::cylinder`]).
    #[must_use]
    pub fn extent_m(&self) -> Option<[f64; 2]> {
        let mm =
            |a: u32, b: u32, s: f64| Some([f64::from(a) * s / 1000.0, f64::from(b) * s / 1000.0]);
        match self {
            Self::Chessboard {
                inner_rows,
                inner_cols,
                square_size_mm,
                ..
            }
            | Self::MarkerBoard {
                inner_rows,
                inner_cols,
                square_size_mm,
                ..
            } => mm(inner_cols + 1, inner_rows + 1, *square_size_mm),
            Self::Charuco {
                rows,
                cols,
                square_size_mm,
                ..
            }
            | Self::PuzzleBoard {
                rows,
                cols,
                square_size_mm,
                ..
            } => mm(*cols, *rows, *square_size_mm),
            Self::PuzzlePole { .. } => None,
        }
    }

    /// For a PuzzlePole, `(radius, length)` of its cylinder in metres.
    #[must_use]
    pub fn cylinder(&self) -> Option<(f64, f64)> {
        match self {
            Self::PuzzlePole {
                circumference_squares,
                axial_squares,
                square_size_mm,
                ..
            } => Some((
                f64::from(*circumference_squares) * square_size_mm / 1000.0 / std::f64::consts::TAU,
                f64::from(*axial_squares) * square_size_mm / 1000.0,
            )),
            _ => None,
        }
    }

    /// Structural checks (sizes and counts; calib-targets checks the rest,
    /// such as a PuzzlePole's period, when the target is built).
    pub(crate) fn check(&self, issues: &mut Issues, path: &str) {
        let (square, at_least) = match self {
            Self::Chessboard {
                inner_rows,
                inner_cols,
                square_size_mm,
                ..
            }
            | Self::MarkerBoard {
                inner_rows,
                inner_cols,
                square_size_mm,
                ..
            } => (
                *square_size_mm,
                [
                    (*inner_rows, "inner_rows", 1),
                    (*inner_cols, "inner_cols", 1),
                ],
            ),
            Self::Charuco {
                rows,
                cols,
                square_size_mm,
                ..
            }
            | Self::PuzzleBoard {
                rows,
                cols,
                square_size_mm,
                ..
            } => (*square_size_mm, [(*rows, "rows", 2), (*cols, "cols", 2)]),
            Self::PuzzlePole {
                circumference_squares,
                axial_squares,
                square_size_mm,
                ..
            } => (
                *square_size_mm,
                [
                    (*circumference_squares, "circumference_squares", 3),
                    (*axial_squares, "axial_squares", 2),
                ],
            ),
        };
        issues.check_positive(&format!("{path}.square_size_mm"), square);
        for (value, name, min) in at_least {
            if value < min {
                issues.push(format!("{path}.{name}"), format!("must be at least {min}"));
            }
        }
        let fractions: &[(&str, Option<f64>)] = match self {
            Self::Chessboard {
                inner_square_rel, ..
            } => &[("inner_square_rel", *inner_square_rel)],
            Self::Charuco {
                marker_size_rel,
                inner_square_rel,
                ..
            } => &[
                ("marker_size_rel", Some(*marker_size_rel)),
                ("inner_square_rel", *inner_square_rel),
            ],
            Self::MarkerBoard {
                circle_diameter_rel,
                inner_square_rel,
                ..
            } => &[
                ("circle_diameter_rel", Some(*circle_diameter_rel)),
                ("inner_square_rel", *inner_square_rel),
            ],
            Self::PuzzleBoard {
                dot_diameter_rel, ..
            }
            | Self::PuzzlePole {
                dot_diameter_rel, ..
            } => &[("dot_diameter_rel", Some(*dot_diameter_rel))],
        };
        for (name, value) in fractions {
            if let Some(v) = value
                && !(v.is_finite() && *v > 0.0 && *v < 1.0)
            {
                issues.push(
                    format!("{path}.{name}"),
                    format!("must be in (0, 1), got {v}"),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_tags_follow_calib_targets() {
        let pole: PrintedTarget = serde_json::from_str(
            r#"{"kind": "puzzlepole", "circumference_squares": 12, "start_row": 0,
                "axial_squares": 6, "square_size_mm": 30.0}"#,
        )
        .unwrap();
        let PrintedTarget::PuzzlePole {
            dot_diameter_rel,
            axial_start_col,
            ..
        } = pole
        else {
            panic!("a pole");
        };
        assert_eq!((dot_diameter_rel, axial_start_col), (1.0 / 3.0, 0));
        let (r, l) = pole.cylinder().unwrap();
        assert!(
            (2.0 * r - 0.11459).abs() < 1e-5,
            "the paper's 11.46 cm pole"
        );
        assert!((l - 0.18).abs() < 1e-15);
        assert_eq!(pole.extent_m(), None);
        let board: PrintedTarget = serde_json::from_str(
            r#"{"kind": "puzzle_board", "rows": 10, "cols": 12, "square_size_mm": 15.0}"#,
        )
        .unwrap();
        assert_eq!(board.extent_m(), Some([0.18, 0.15]));
        let text = serde_json::to_string(&board).unwrap();
        assert!(text.contains(r#""kind":"puzzle_board""#), "{text}");
    }

    #[test]
    fn bad_sizes_are_reported() {
        let bad = PrintedTarget::Charuco {
            rows: 1,
            cols: 7,
            square_size_mm: 0.0,
            marker_size_rel: 1.2,
            dictionary: "DICT_4X4_50".into(),
            marker_layout: CharucoLayout::OpencvCharuco,
            border_bits: 1,
            inner_square_rel: None,
        };
        let mut issues = Issues::default();
        bad.check(&mut issues, "t");
        let paths: Vec<String> = issues
            .finish()
            .unwrap_err()
            .issues
            .into_iter()
            .map(|i| i.path)
            .collect();
        assert_eq!(paths, ["t.square_size_mm", "t.rows", "t.marker_size_rel"]);
    }
}
