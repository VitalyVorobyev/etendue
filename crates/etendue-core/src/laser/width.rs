//! The laser cross-section width model: how thick the laser stripe is at a
//! given distance from the laser origin.
//!
//! A real line laser does not project an infinitely thin line — the beam has a
//! finite cross-section thickness that varies with distance. [`WidthModel`] is
//! the trait the intersection layer ([`super::intersect`]) consults for that
//! thickness; [`GaussianBeamWidth`] is the default, physically-motivated
//! implementation.
//!
//! # The Gaussian-beam model
//!
//! A focused Gaussian beam has a waist (its narrowest cross-section) and
//! diverges on either side of it. Its `1/e²` radius `w` as a function of the
//! axial distance `z` from the waist is
//!
//! ```text
//! w(z) = w0 · sqrt(1 + (z / zR)²)
//! ```
//!
//! where `w0` is the **waist radius** and `zR` the **Rayleigh range**
//!
//! ```text
//! zR = π · w0² / λ
//! ```
//!
//! with `λ` the wavelength (in the same length unit as `w0`). For a line laser
//! the relevant cross-section is the *across-stripe* thickness; this model
//! treats that thickness with the standard Gaussian-beam law, the waist placed
//! **at the laser origin** (`z = 0`), so the distance `d` from the laser
//! origin used by the intersection layer is exactly the axial coordinate `z`.
//!
//! The width reported by [`WidthModel::width_at`] is the full cross-section
//! **diameter** `2·w(d)` — the figure the projection layer needs to draw a
//! band of finite thickness, and the natural pairing for the
//! circle-of-confusion *diameter* it is combined with in quadrature.
//!
//! # Honest scope
//!
//! Only the **across-stripe** width is modelled — the dimension that, combined
//! with defocus, sets the imaged line's thickness. The *along-fan* intensity /
//! width profile is explicitly out of scope for the MVP (development plan, M5
//! "Out of scope"). The model is also the ideal Gaussian-beam law: no
//! truncation by the laser aperture, no astigmatism.
//!
//! # User override
//!
//! [`WidthModel`] is a trait precisely so a component can override the default
//! per the handoff: a measured line-width table, a collimated (constant-width)
//! beam, or a different divergence law can all implement it and be passed to
//! the intersection layer in place of [`GaussianBeamWidth`].

/// Nanometres per metre — converts a wavelength given in nm to metres.
const NM_PER_M: f64 = 1.0e9;

/// A model of a laser stripe's cross-section thickness versus distance.
///
/// The intersection layer ([`super::intersect`]) samples the laser stripe and
/// asks, at each sample, "how thick is the beam here?" — that is exactly
/// [`WidthModel::width_at`]. Implement this trait to override the default
/// [`GaussianBeamWidth`] with a measured or alternative profile.
///
/// The reported width is a full cross-section **diameter**, in metres.
pub trait WidthModel {
    /// The laser stripe's full cross-section thickness, in **metres**, at a
    /// distance `distance_m` (metres) from the laser origin.
    ///
    /// `distance_m` is the straight-line distance from the laser apex to the
    /// sample point. Implementations should accept any finite, non-negative
    /// distance and return a finite, strictly-positive thickness.
    fn width_at(&self, distance_m: f64) -> f64;
}

/// The default Gaussian-beam cross-section width model.
///
/// Implements `w(d) = w0 · sqrt(1 + (d / zR)²)` with the Rayleigh range
/// `zR = π · w0² / λ`, the waist at the laser origin. See the module
/// doc-comment for the physics. [`GaussianBeamWidth::width_at`] returns the
/// full **diameter** `2·w(d)`.
///
/// Construct it from a [`LaserEntity`](crate::scene::LaserEntity) with
/// [`GaussianBeamWidth::from_laser`] (which reads the entity's beam waist and
/// converts its `wavelength_nm` to metres), or directly with
/// [`GaussianBeamWidth::new`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GaussianBeamWidth {
    /// Beam-waist radius `w0`, in metres — the `1/e²` radius at the waist.
    waist_radius_m: f64,
    /// Rayleigh range `zR`, in metres — the axial distance over which the
    /// beam cross-section area doubles.
    rayleigh_range_m: f64,
}

impl GaussianBeamWidth {
    /// Build a Gaussian-beam width model from the waist radius and wavelength.
    ///
    /// `waist_radius_m` is the `1/e²` waist radius `w0` and `wavelength_m` the
    /// emission wavelength, **both in metres**. The Rayleigh range is computed
    /// as `zR = π · w0² / λ`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidInput`](crate::Error::InvalidInput) if
    /// `waist_radius_m` or `wavelength_m` is not finite and strictly positive.
    pub fn new(waist_radius_m: f64, wavelength_m: f64) -> crate::Result<Self> {
        if !(waist_radius_m.is_finite() && waist_radius_m > 0.0) {
            return Err(crate::Error::InvalidInput {
                reason: format!(
                    "beam-waist radius must be finite and positive, got {waist_radius_m}"
                ),
            });
        }
        if !(wavelength_m.is_finite() && wavelength_m > 0.0) {
            return Err(crate::Error::InvalidInput {
                reason: format!("wavelength must be finite and positive, got {wavelength_m}"),
            });
        }
        let rayleigh_range_m =
            std::f64::consts::PI * waist_radius_m * waist_radius_m / wavelength_m;
        Ok(Self {
            waist_radius_m,
            rayleigh_range_m,
        })
    }

    /// Build a Gaussian-beam width model from a [`LaserEntity`](crate::scene::LaserEntity).
    ///
    /// Reads the entity's [`beam_waist_m`](crate::scene::LaserEntity::beam_waist_m)
    /// as the waist radius `w0` and converts its
    /// [`wavelength_nm`](crate::scene::LaserEntity::wavelength_nm) to metres.
    ///
    /// # Errors
    ///
    /// Propagates [`GaussianBeamWidth::new`]'s validation. Cannot fail for a
    /// [`LaserEntity`](crate::scene::LaserEntity) built through
    /// [`LaserEntity::new`](crate::scene::LaserEntity::new) — the same
    /// quantities are validated there.
    pub fn from_laser(laser: &crate::scene::LaserEntity) -> crate::Result<Self> {
        Self::new(laser.beam_waist_m, laser.wavelength_nm / NM_PER_M)
    }

    /// The beam-waist radius `w0`, in metres.
    #[must_use]
    pub fn waist_radius_m(&self) -> f64 {
        self.waist_radius_m
    }

    /// The Rayleigh range `zR`, in metres.
    #[must_use]
    pub fn rayleigh_range_m(&self) -> f64 {
        self.rayleigh_range_m
    }
}

impl WidthModel for GaussianBeamWidth {
    /// The full cross-section **diameter** `2·w(d)`, in metres, at distance
    /// `distance_m` from the laser origin (the waist).
    ///
    /// `w(d) = w0 · sqrt(1 + (d / zR)²)`. A negative distance is treated as
    /// its absolute value — the Gaussian-beam law is symmetric about the
    /// waist, and the intersection layer never produces one anyway.
    fn width_at(&self, distance_m: f64) -> f64 {
        let d = distance_m.abs();
        let ratio = d / self.rayleigh_range_m;
        let radius = self.waist_radius_m * (1.0 + ratio * ratio).sqrt();
        2.0 * radius
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    // A red machine-vision line laser: 660 nm, 0.25 mm waist radius.
    const LAMBDA_M: f64 = 660.0e-9;
    const W0_M: f64 = 0.25e-3;

    #[test]
    fn rayleigh_range_matches_the_closed_form() {
        let model = GaussianBeamWidth::new(W0_M, LAMBDA_M).unwrap();
        // zR = pi * w0^2 / lambda.
        let expected = std::f64::consts::PI * W0_M * W0_M / LAMBDA_M;
        assert_relative_eq!(model.rayleigh_range_m(), expected, epsilon = 1e-18);
        // For these numbers zR is on the order of 0.3 m — sensible for a
        // machine-vision standoff.
        assert!(model.rayleigh_range_m() > 0.1 && model.rayleigh_range_m() < 1.0);
    }

    #[test]
    fn width_at_the_waist_is_the_waist_diameter() {
        let model = GaussianBeamWidth::new(W0_M, LAMBDA_M).unwrap();
        // At d = 0 the beam is at its waist: width = 2 * w0.
        assert_relative_eq!(model.width_at(0.0), 2.0 * W0_M, epsilon = 1e-15);
    }

    #[test]
    fn width_at_one_rayleigh_range_is_sqrt2_times_the_waist() {
        let model = GaussianBeamWidth::new(W0_M, LAMBDA_M).unwrap();
        // At d = zR, w = w0 * sqrt(2), so the diameter is 2 * w0 * sqrt(2).
        let zr = model.rayleigh_range_m();
        let expected = 2.0 * W0_M * std::f64::consts::SQRT_2;
        assert_relative_eq!(model.width_at(zr), expected, epsilon = 1e-15);
    }

    #[test]
    fn width_grows_monotonically_with_distance() {
        let model = GaussianBeamWidth::new(W0_M, LAMBDA_M).unwrap();
        let mut last = 0.0;
        for d in [0.0, 0.05, 0.1, 0.3, 0.6, 1.0, 2.0] {
            let w = model.width_at(d);
            assert!(w.is_finite() && w > 0.0);
            assert!(w >= last, "width must not shrink with distance");
            last = w;
        }
    }

    #[test]
    fn width_is_symmetric_about_the_waist() {
        let model = GaussianBeamWidth::new(W0_M, LAMBDA_M).unwrap();
        // The Gaussian-beam law depends only on |d|.
        assert_relative_eq!(model.width_at(-0.4), model.width_at(0.4), epsilon = 1e-15);
    }

    #[test]
    fn far_from_the_waist_the_width_is_asymptotically_linear() {
        // Far past zR, w(d) -> w0 * d / zR, i.e. the divergence is linear.
        let model = GaussianBeamWidth::new(W0_M, LAMBDA_M).unwrap();
        let zr = model.rayleigh_range_m();
        let d = 100.0 * zr;
        let asymptote = 2.0 * W0_M * d / zr;
        // Within 0.01% at 100 Rayleigh ranges.
        assert_relative_eq!(model.width_at(d), asymptote, max_relative = 1e-4);
    }

    #[test]
    fn new_rejects_bad_parameters() {
        assert!(GaussianBeamWidth::new(0.0, LAMBDA_M).is_err());
        assert!(GaussianBeamWidth::new(-1e-3, LAMBDA_M).is_err());
        assert!(GaussianBeamWidth::new(W0_M, 0.0).is_err());
        assert!(GaussianBeamWidth::new(W0_M, f64::NAN).is_err());
        assert!(GaussianBeamWidth::new(W0_M, LAMBDA_M).is_ok());
    }

    #[test]
    fn from_laser_reads_the_entity_waist_and_wavelength() {
        use nalgebra::Isometry3;
        // LaserEntity carries wavelength in nm; from_laser converts to metres.
        let laser = crate::scene::LaserEntity::new(
            Isometry3::identity(),
            0.3,
            1.0,
            660.0, // nm
            W0_M,
        )
        .unwrap();
        let model = GaussianBeamWidth::from_laser(&laser).unwrap();
        let direct = GaussianBeamWidth::new(W0_M, LAMBDA_M).unwrap();
        assert_relative_eq!(
            model.rayleigh_range_m(),
            direct.rayleigh_range_m(),
            epsilon = 1e-18
        );
        assert_relative_eq!(model.waist_radius_m(), W0_M, epsilon = 1e-15);
    }

    #[test]
    fn a_custom_width_model_can_override_the_default() {
        // The trait is the override hook: a collimated (constant-width) beam.
        struct Collimated(f64);
        impl WidthModel for Collimated {
            fn width_at(&self, _distance_m: f64) -> f64 {
                self.0
            }
        }
        let model = Collimated(1.0e-3);
        assert_relative_eq!(model.width_at(0.0), 1.0e-3, epsilon = 1e-15);
        assert_relative_eq!(model.width_at(5.0), 1.0e-3, epsilon = 1e-15);
    }
}
