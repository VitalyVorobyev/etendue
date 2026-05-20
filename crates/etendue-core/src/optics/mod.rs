//! Optical model: thick-lens conjugates, the Scheimpflug plane of best focus,
//! and the geometric circle of confusion.
//!
//! This is the M4 defocus physics — the single most correctness-critical part
//! of `etendue-core`. It is headless: pure `f64` functions, no UI, no
//! rendering. The analysis layer ([`crate::analysis`]) samples it over a
//! target; the UI renders the result.
//!
//! # Contents
//!
//! - [`coc`] — the bare physics as documented free functions: the thick-lens
//!   conjugate relation, the tilted plane of best focus, and the geometric
//!   circle-of-confusion diameter. Derived from first principles; the written
//!   derivation is `docs/derivations/scheimpflug_pobf.md`.
//! - [`thick_lens`] — [`ThickLens`], a camera-aware wrapper that pairs a
//!   `vision-calibration-core` `CameraParams` spec with the four thick-lens
//!   parameters and exposes a clean `coc_diameter` API.
//! - [`psf`] — Gaussian point-spread function model: smooth Gaussian-sigma
//!   conversion from the geometric CoC diameter and a numerically stable
//!   smooth-max helper for gradient-based optimisation. This is the M9
//!   follow-up: a smoother, physically motivated blur model that complements
//!   the geometric CoC and feeds the simulated-image panel's Gaussian-band
//!   visualisation.
//!
//! # The model, in brief
//!
//! An **ideal** (aberration-free, paraxial) thick lens characterised by an
//! effective focal length `f`, an f-number `N`, an object-side focus distance
//! `s_o`, and the inter-principal gap `g = H - H'`. The optical axis is
//! camera-local `+z`; the camera-local origin is the rear principal plane
//! `H'`. An object point off the plane of best focus images to a blur disc
//! whose diameter — the circle of confusion — the model computes, both at the
//! sensor (metres) and in pixels.
//!
//! Because the lens is ideal, a fronto-parallel sensor produces a CoC that
//! depends on a point's axial defocus *only*; field/lateral dependence enters
//! *exclusively* through Scheimpflug sensor tilt. See [`coc`] for the full
//! statement of scope.

pub mod coc;
pub mod psf;
pub mod thick_lens;

pub use coc::{
    PlaneOfBestFocus, coc_diameter_at_sensor, coc_to_pixels, image_distance, pobf_tilt_tan,
    sensor_distance,
};
pub use psf::{GAUSSIAN_FWHM_PER_SIGMA, coc_to_gaussian_sigma, gaussian_sigma_to_coc, log_sum_exp};
pub use thick_lens::{ThickLens, scheimpflug_tilt};
