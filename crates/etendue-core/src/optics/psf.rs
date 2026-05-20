//! Gaussian point-spread function model — the smooth, physical alternative to
//! the hard-disc geometric circle of confusion.
//!
//! [`crate::optics::coc`] computes the geometric circle of confusion — a
//! sharp-edged disc of diameter `c` at the sensor. That is the textbook
//! paraxial result for an ideal (aberration-free) lens, and it is what the
//! M4 / M5 / M6 / M9 analyses use. A real defocused image is not a hard disc:
//! diffraction at the aperture and residual lens aberrations smear the blur
//! out, producing a profile much closer to a 2D Gaussian. For visualisation
//! (the M8 simulated-image panel) and for smooth-objective optimisation
//! (potential M9 follow-up: log-sum-exp soft-max over Gaussian widths) we
//! need a Gaussian-PSF model with a defensible sigma.
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
//! - **Soft max** ([`log_sum_exp`]). A smooth, differentiable approximation
//!   to `max(x_i)` over a finite set, parameterised by a temperature; used
//!   as a smoothed objective for the Scheimpflug solver when a gradient
//!   pass is needed.
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
/// `sigma = c / 2.3548`. The reverse is
/// [`gaussian_sigma_to_coc`].
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

/// Convert a Gaussian sigma into the geometric CoC diameter with the same
/// FWHM. The inverse of [`coc_to_gaussian_sigma`].
///
/// Useful for round-trip consistency in tests and for tools that report
/// blur in either unit.
#[must_use]
pub fn gaussian_sigma_to_coc(sigma: f64) -> f64 {
    if !(sigma.is_finite() && sigma > 0.0) {
        return 0.0;
    }
    sigma * GAUSSIAN_FWHM_PER_SIGMA
}

/// Numerically stable smooth maximum: `(1/β) · log Σ exp(β·x_i)`.
///
/// As `β → ∞` this approaches `max(x_i)` from above; for finite `β` it is
/// a smooth, differentiable approximation. For the Scheimpflug solver this
/// gives a gradient-friendly cost when paired with a gradient-based optimiser
/// (L-BFGS, BFGS) — the hard-max version is non-smooth at sample-rank
/// transitions, and Nelder-Mead is then the only practical choice.
///
/// The "stable" part: we factor out the maximum before the exp / log so the
/// exponentials never overflow. Algorithmically equivalent to the naive
/// `(1/β) · log Σ exp(β·x_i)`.
///
/// Returns `f64::NEG_INFINITY` for an empty input (the convention that
/// `max` over an empty set is `-∞`).
///
/// # Panics
///
/// Panics if `beta` is not finite and strictly positive — the smoothness
/// parameter must be a usable scale.
#[must_use]
pub fn log_sum_exp(values: &[f64], beta: f64) -> f64 {
    assert!(
        beta.is_finite() && beta > 0.0,
        "log_sum_exp temperature must be finite and positive, got {beta}"
    );
    if values.is_empty() {
        return f64::NEG_INFINITY;
    }
    let m = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if !m.is_finite() {
        // All inputs are `-∞` (or any are `NaN` — propagated by `max`); the
        // max is meaningless and the soft-max can't shift to it.
        return m;
    }
    let sum_exp: f64 = values.iter().map(|x| ((x - m) * beta).exp()).sum();
    m + sum_exp.ln() / beta
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

    #[test]
    fn round_trip_coc_sigma_coc_is_identity_for_positive_inputs() {
        for c in [0.5, 1.0, 3.45, 10.0, 100.0] {
            let sigma = coc_to_gaussian_sigma(c);
            let back = gaussian_sigma_to_coc(sigma);
            assert_relative_eq!(back, c, epsilon = 1e-12);
        }
    }

    #[test]
    fn log_sum_exp_approaches_hard_max_for_high_beta() {
        // With beta = 100 the soft-max is within < 0.05 of the hard max for
        // values separated by O(1).
        let v = [1.0, 2.0, 5.0, 3.0, 4.5];
        let hard = 5.0;
        let soft = log_sum_exp(&v, 100.0);
        assert!(
            (soft - hard).abs() < 0.05,
            "soft-max {soft} should be near hard max {hard} at beta=100"
        );
    }

    #[test]
    fn log_sum_exp_is_strictly_above_the_hard_max() {
        // Mathematically, log-sum-exp ≥ max for any finite β, with equality
        // only as β → ∞ on a singleton-maximum input.
        let v = [1.0, 2.0, 3.0];
        let soft = log_sum_exp(&v, 1.0);
        assert!(soft > 3.0, "soft-max {soft} should exceed hard max 3");
    }

    #[test]
    fn log_sum_exp_is_stable_with_huge_values() {
        // Naive (1/β)·log Σ exp(β·x) overflows when β·x > 700.
        // Our shifted form stays finite for any input — exp inputs are ≤ 0.
        let huge = [1000.0, 1001.0, 1002.0];
        let soft = log_sum_exp(&huge, 1.0);
        assert!(
            soft.is_finite(),
            "log_sum_exp must not overflow, got {soft}"
        );
        // Soft-max is at least the max (which is 1002), and at most a little above it.
        assert!(soft >= 1002.0);
        assert!(soft < 1010.0);
    }

    #[test]
    fn log_sum_exp_empty_is_neg_infinity() {
        let v: [f64; 0] = [];
        assert!(log_sum_exp(&v, 1.0).is_infinite());
        assert!(log_sum_exp(&v, 1.0) < 0.0);
    }

    #[test]
    #[should_panic]
    fn log_sum_exp_rejects_zero_beta() {
        let v = [1.0];
        let _ = log_sum_exp(&v, 0.0);
    }

    #[test]
    #[should_panic]
    fn log_sum_exp_rejects_negative_beta() {
        let v = [1.0];
        let _ = log_sum_exp(&v, -1.0);
    }
}
