//! Synthetic-image support for **etendue** (`docs/pivot/PLAN.md` P3).
//!
//! - [`remap`] — the canonical render camera and the remap LUT of
//!   [ADR 0004](https://github.com/VitalyVorobyev/etendue/blob/main/docs/adrs/0004-canonical-render-camera.md):
//!   every renderer draws a plain square-pixel pinhole, and the target
//!   calibration-rs camera model is applied afterwards by resampling through
//!   the LUT.
//!
//! - [`gt`] — analytic ground truth of
//!   [ADR 0006](https://github.com/VitalyVorobyev/etendue/blob/main/docs/adrs/0006-ground-truth.md):
//!   target points projected through the calibration-rs model, with visibility.
//!
//! - [`dataset`] — the synthetic dataset bundle: a calibration-rs
//!   `DatasetSpec`, robot poses, and `gt.json`.
//!
//! No camera-model math lives here. Every projection and back-projection is
//! `vision-calibration-core`'s (`CameraModel::project_point_c`,
//! `CameraModel::backproject_pixel`); this crate only chooses the canonical
//! camera and tabulates the composition.

pub mod dataset;
#[doc(hidden)]
pub mod gate;
pub mod gt;
#[cfg(feature = "images")]
pub mod images;
pub mod job;
pub mod remap;

pub use remap::{CanonicalCamera, CanonicalSpec, PixelCentre, RemapLut, remap_lut};

/// Why a synthesis step failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A camera model did not build (singular sensor homography).
    #[error("camera model: {0}")]
    Camera(#[from] vision_calibration_core::Error),
    /// A parameter is out of range.
    #[error("{0}")]
    InvalidInput(String),
}

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;
