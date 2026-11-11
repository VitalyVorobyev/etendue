//! The optical model: [`PhysicalCamera`] (ADR 0008), a body (mechanical frame) with a sensor and
//! a thick lens with pupils, focused at a distance. It owns every conversion between the
//! mechanical frame and the lens's optical points, and gives the equivalent calibration-rs
//! pinhole, projection, blur and depth of field.
//!
//! - [`camera`] — [`PhysicalCamera`].
//! - [`paraxial`] — a paraxial ray trace through a [`PhysicalCamera`]: the reference its closed
//!   forms are tested against, and the ray fans for drawing.
//!
//! The thick-lens circle-of-confusion formulas of the egui era (`coc`, `ThickLens`, the Gaussian
//! PSF) were removed in P19f: nothing used them once `PhysicalCamera` replaced them. Their
//! derivation stays in `docs/derivations/scheimpflug_pobf.md`.

pub mod camera;
pub mod paraxial;

pub use camera::{CardinalPoints, DepthOfField, Focusing, Lens, PhysicalCamera, SensorPlane};
