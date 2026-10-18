//! PuzzleBoards: calib-targets' PuzzleBoard detector, which reads every
//! corner's position on the master pattern from the edge dots; corners are
//! matched to the board by their master id. The search is restricted to the
//! printed board (`FixedBoard`): the board is known, so a decode outside it
//! can only be wrong.

use std::collections::HashMap;

use anyhow::{Result, anyhow};
use calib_targets_chessboard::{ChessCorner, GrayImageView};
use calib_targets_print::PuzzleBoardTargetSpec;
use calib_targets_puzzleboard::{
    PuzzleBoardDetectError, PuzzleBoardDetector, PuzzleBoardParams, PuzzleBoardSearchMode,
    PuzzleBoardSpec,
};
use etendue_synth::gt::BoardPoint;

use super::{Detection, ViewStatus, by_id, labelled};

/// A PuzzleBoard and its detector.
pub(super) struct PuzzleBoard {
    detector: PuzzleBoardDetector,
    by_id: HashMap<u32, usize>,
}

impl PuzzleBoard {
    /// The detector for `spec`, matching to `points` (the board's, with
    /// master ids).
    ///
    /// # Errors
    ///
    /// If calib-targets rejects the board, or a point has no id.
    pub(super) fn new(spec: &PuzzleBoardTargetSpec, points: &[BoardPoint]) -> Result<Self> {
        let board = PuzzleBoardSpec::with_origin(
            spec.rows,
            spec.cols,
            spec.square_size_mm as f32,
            spec.origin_row,
            spec.origin_col,
        )
        .map_err(|e| anyhow!("PuzzleBoard: {e}"))?;
        let mut params = PuzzleBoardParams::for_board(board);
        params.decode.search_mode = PuzzleBoardSearchMode::FixedBoard;
        let detector = PuzzleBoardDetector::new(params).map_err(|e| anyhow!("PuzzleBoard: {e}"))?;
        Ok(Self {
            detector,
            by_id: by_id(points)?,
        })
    }

    /// Label the corners of one image.
    pub(super) fn label(&self, image: &GrayImageView<'_>, corners: &[ChessCorner]) -> Detection {
        match self.detector.detect_with_corners(image, corners) {
            Ok(found) => labelled(
                &self.by_id,
                found.corners.iter().map(|c| (Some(c.id), c.position)),
            ),
            Err(e) => {
                let status = match e {
                    PuzzleBoardDetectError::ChessboardNotDetected => ViewStatus::NoBoard,
                    PuzzleBoardDetectError::NotEnoughEdges { .. }
                    | PuzzleBoardDetectError::WindowTooThin { .. }
                    | PuzzleBoardDetectError::NotEnoughLogicalBits { .. } => ViewStatus::Partial,
                    PuzzleBoardDetectError::InconsistentPosition => ViewStatus::Ambiguous,
                    _ => ViewStatus::Undecoded,
                };
                Detection::failed(status, e.to_string())
            }
        }
    }
}
