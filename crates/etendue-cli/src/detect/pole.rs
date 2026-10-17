//! PuzzlePoles: calib-targets' PuzzlePole detector, the PuzzleBoard decode
//! with the pole's circumference as the code's period; corners are matched to
//! the pole by their pole id (`cyclic · (axial_squares + 1) + axial`).

use std::collections::HashMap;

use anyhow::{Result, anyhow};
use calib_targets_chessboard::{ChessCorner, GrayImageView};
use calib_targets_print::PuzzlePoleTargetSpec;
use calib_targets_puzzleboard::{
    PuzzlePoleDetectError, PuzzlePoleDetector, PuzzlePoleParams, PuzzlePoleSpec,
};
use etendue_synth::gt::BoardPoint;

use super::{Detection, ViewStatus, by_id, labelled};

/// A PuzzlePole and its detector.
pub(super) struct PuzzlePole {
    detector: PuzzlePoleDetector,
    by_id: HashMap<u32, usize>,
}

impl PuzzlePole {
    /// The detector for `spec`, matching to `points` (the pole's, with ids).
    ///
    /// # Errors
    ///
    /// If calib-targets rejects the pole, or a point has no id.
    pub(super) fn new(spec: &PuzzlePoleTargetSpec, points: &[BoardPoint]) -> Result<Self> {
        let pole = PuzzlePoleSpec::with_placement(
            spec.circumference_squares,
            spec.start_row,
            spec.axial_start_col,
            spec.axial_squares,
            spec.square_size_mm as f32,
        )
        .map_err(|e| anyhow!("PuzzlePole: {e}"))?;
        let detector = PuzzlePoleDetector::new(PuzzlePoleParams::for_pole(pole))
            .map_err(|e| anyhow!("PuzzlePole: {e}"))?;
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
                    PuzzlePoleDetectError::ChessboardNotDetected => ViewStatus::NoBoard,
                    PuzzlePoleDetectError::NotEnoughEdges { .. }
                    | PuzzlePoleDetectError::FragmentTooSmall { .. }
                    | PuzzlePoleDetectError::NotEnoughLogicalBits { .. } => ViewStatus::Partial,
                    PuzzlePoleDetectError::InconsistentPosition { .. } => ViewStatus::Ambiguous,
                    _ => ViewStatus::Undecoded,
                };
                Detection::failed(status, e.to_string())
            }
        }
    }
}
