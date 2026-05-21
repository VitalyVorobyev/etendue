//! Gaussian point-spread function model — the smooth, physical alternative to
//! the hard-disc geometric circle of confusion.
//!
//! [`crate::optics::coc`] computes the geometric circle of confusion — a
//! sharp-edged disc of diameter `c` at the sensor. That is the textbook
//! paraxial result for an ideal (aberration-free) lens, and it is what the
//! M4 / M5 / M6 / M9 analyses use. A real defocused image is not a hard disc:
//! diffraction at the aperture and residual lens aberrations smear the blur
//! out, producing a profile much closer to a 2D Gaussian. For visualisation
//! (the simulated-image panel) we need a Gaussian-PSF model with a defensible
//! sigma.
//!
//! # The width convention — FWHM matching
//!
//! Given a geometric CoC diameter `c`, the **Gaussian sigma** that has the
//! same full width at half-maximum (FWHM) is
//!
//! ```text
//! sigma = c / (2 * sqrt(2 * ln 2))  ≈  c / 2.3548
//! ```
//!
//! because for a 1D Gaussian `exp(-x² / (2·sigma²))`, the FWHM is
//! `2·sigma·sqrt(2·ln 2)`. FWHM matching is the convention used in
//! astronomy, microscopy, and machine vision when reporting "effective
//! blur": two profiles with the same FWHM look subjectively comparable
//! even when their tails differ. (A volume-equivalent Gaussian would give
//! `sigma = c / 4`, smaller; an `1/e²` equivalent gives `sigma = c / 2`,
//! larger. FWHM is the conservative middle ground.)
//!
//! # Scope
//!
//! - **Per-vertex sigma** ([`coc_to_gaussian_sigma`]). The unit conversion.
//!
//! Out of scope (deliberately):
//!
//! - 2D rendering of a Gaussian image. That belongs to the UI's
//!   simulated-image panel, which composes per-vertex sigma with the
//!   stripe polyline.
//! - Diffraction-limited PSF or the Airy pattern. The Gaussian approximation
//!   is good enough for designer-facing visualisation; the Airy is the
//!   correct limit at small apertures and we will add it only if it
//!   becomes the bottleneck for a real design decision.

/// `2·sqrt(2·ln 2)` — the FWHM of a unit-sigma Gaussian.
///
/// Pre-computed so [`coc_to_gaussian_sigma`] is allocation- and trig-free.
pub const GAUSSIAN_FWHM_PER_SIGMA: f64 = 2.354_820_045_030_949_4; // 2·sqrt(2·ln 2)

/// Convert a geometric circle-of-confusion **diameter** into a
/// **Gaussian-equivalent sigma** with the same FWHM.
///
/// This is the central conversion of the PSF model. The geometric CoC
/// diameter `c` is what [`crate::optics::coc::coc_diameter_at_sensor`] returns
/// (in metres) or [`crate::optics::coc::coc_to_pixels`] returns (in pixels).
/// The output `sigma` is in the **same units** as the input.
///
/// `sigma = c / 2.3548`.
///
/// Returns `0.0` for a non-positive or non-finite input; defocus must be at
/// least zero, and the Gaussian profile collapses to a delta at zero blur.
#[must_use]
pub fn coc_to_gaussian_sigma(coc: f64) -> f64 {
    if !(coc.is_finite() && coc > 0.0) {
        return 0.0;
    }
    coc / GAUSSIAN_FWHM_PER_SIGMA
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn fwhm_per_sigma_matches_the_textbook_value() {
        // 2*sqrt(2*ln 2) ≈ 2.3548200450309493
        let computed = 2.0 * (2.0 * 2.0_f64.ln()).sqrt();
        assert_relative_eq!(computed, GAUSSIAN_FWHM_PER_SIGMA, epsilon = 1e-15);
    }

    #[test]
    fn coc_to_gaussian_sigma_matches_the_fwhm_definition() {
        // For c = 4.7096 µm, sigma should be 2.0 µm.
        let c = 2.0 * GAUSSIAN_FWHM_PER_SIGMA;
        assert_relative_eq!(coc_to_gaussian_sigma(c), 2.0, epsilon = 1e-12);
    }

    #[test]
    fn coc_to_gaussian_sigma_rejects_bad_inputs() {
        assert_eq!(coc_to_gaussian_sigma(0.0), 0.0);
        assert_eq!(coc_to_gaussian_sigma(-1.0), 0.0);
        assert_eq!(coc_to_gaussian_sigma(f64::NAN), 0.0);
        assert_eq!(coc_to_gaussian_sigma(f64::INFINITY), 0.0);
    }
}
