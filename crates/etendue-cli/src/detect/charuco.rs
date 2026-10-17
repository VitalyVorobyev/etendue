//! ChArUco boards: calib-targets' ChArUco detector, which identifies every
//! corner by the markers around it; corners are matched to the board by their
//! ChArUco id.

use std::collections::HashMap;

use anyhow::{Result, anyhow};
use calib_targets_charuco::{CharucoDetectError, CharucoDetector, CharucoParams};
use calib_targets_chessboard::{ChessCorner, GrayImageView};
use calib_targets_print::CharucoTargetSpec;
use etendue_synth::gt::BoardPoint;

use super::{Detection, ViewStatus, by_id, labelled};

/// A ChArUco board and its detector.
pub(super) struct Charuco {
    detector: CharucoDetector,
    by_id: HashMap<u32, usize>,
}

impl Charuco {
    /// The detector for `spec`, matching to `points` (the board's, with ids).
    ///
    /// # Errors
    ///
    /// If calib-targets rejects the board, or a point has no id.
    pub(super) fn new(spec: &CharucoTargetSpec, points: &[BoardPoint]) -> Result<Self> {
        let detector = CharucoDetector::new(CharucoParams::for_board(spec.to_board_spec()))
            .map_err(|e| anyhow!("ChArUco board: {e}"))?;
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
                    CharucoDetectError::ChessboardNotDetected => ViewStatus::NoBoard,
                    _ => ViewStatus::Undecoded,
                };
                Detection::failed(status, e.to_string())
            }
        }
    }
}
