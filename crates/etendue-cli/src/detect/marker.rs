//! Marker boards: calib-targets' marker board detector, which places the
//! chessboard grid on the board by its three circles; corners are matched to
//! the board by their aligned grid label. Corners the alignment puts outside
//! the board carry no id and match nothing.

use std::collections::HashMap;

use anyhow::{Result, anyhow};
use calib_targets_chessboard::{ChessCorner, GrayImageView};
use calib_targets_marker::{
    MarkerBoardDetectError, MarkerBoardDetector, MarkerBoardParams, MarkerBoardSpec,
};
use calib_targets_print::MarkerBoardTargetSpec;
use etendue_synth::gt::BoardPoint;

use super::{Detection, ViewStatus, labelled};

/// A marker board and its detector.
pub(super) struct MarkerBoard {
    detector: MarkerBoardDetector,
    by_grid: HashMap<[i32; 2], usize>,
}

impl MarkerBoard {
    /// The detector for `spec`, matching to `points` (the board's, with grid
    /// labels).
    ///
    /// # Errors
    ///
    /// If calib-targets rejects the board, or a point has no grid label.
    pub(super) fn new(spec: &MarkerBoardTargetSpec, points: &[BoardPoint]) -> Result<Self> {
        let board = MarkerBoardSpec::new(
            spec.inner_rows,
            spec.inner_cols,
            spec.circles.map(|c| c.to_detector_spec()),
        )
        .with_cell_size(spec.square_size_mm as f32)
        .with_circle_diameter_rel(spec.circle_diameter_rel as f32);
        let detector = MarkerBoardDetector::new(MarkerBoardParams::for_board(board))
            .map_err(|e| anyhow!("marker board: {e}"))?;
        let by_grid = points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                p.grid
                    .map(|g| (g, i))
                    .ok_or_else(|| anyhow!("board point {i} has no grid label"))
            })
            .collect::<Result<_>>()?;
        Ok(Self { detector, by_grid })
    }

    /// Label the corners of one image.
    pub(super) fn label(&self, image: &GrayImageView<'_>, corners: &[ChessCorner]) -> Detection {
        match self.detector.detect_with_corners(image, corners) {
            Ok(found) => labelled(
                &self.by_grid,
                found
                    .corners
                    .iter()
                    .map(|c| (c.id.map(|_| [c.grid.u, c.grid.v]), c.position)),
            ),
            Err(e) => {
                let status = match e {
                    MarkerBoardDetectError::ChessboardNotDetected => ViewStatus::NoBoard,
                    MarkerBoardDetectError::AlignmentAmbiguous { .. } => ViewStatus::Ambiguous,
                    _ => ViewStatus::Undecoded,
                };
                Detection::failed(status, e.to_string())
            }
        }
    }
}
