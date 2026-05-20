//! [`ThickLens`] — a camera-aware wrapper over the geometric defocus physics.
//!
//! [`crate::optics::coc`] holds the bare physics as free functions. `ThickLens`
//! is the *usable* layer on top: it bundles a camera's [`CameraParams`] spec
//! with the thick-lens optical parameters (`f`, `N`, `s_o`, `g`) and exposes a
//! clean [`ThickLens::coc_diameter`] / [`ThickLens::plane_of_best_focus`] API.
//!
//! # Why it holds `CameraParams`, not a built `CameraModel`
//!
//! The Scheimpflug tilt angles `(tau_x, tau_y)` are the physical input the
//! plane-of-best-focus computation needs. `CameraParams::build()` compiles a
//! `SensorParams::Scheimpflug` into an `AnySensor::Homography` — the built
//! [`CameraModel`](vision_calibration_core::CameraModel) keeps the homography
//! but **discards the tilt angles**. So `ThickLens` retains the
//! `CameraParams` and reads the tilt straight out of its `sensor` field via
//! [`scheimpflug_tilt`]. Consumers that need the projection chain still call
//! `params().build()` to obtain a `CameraModel` on demand.
//!
//! # Upstream-promotion shape
//!
//! [`ThickLens::coc_diameter`] is deliberately a small, self-contained method
//! taking a 3D object point. It is the candidate to become an
//! `ApertureModel` trait method upstreamed into `calibration-rs` once the
//! model stabilises (see the development plan, deferred section). Taking a 3D
//! point — rather than a scalar `(depth, off_axis)` pair — is the right
//! signature: under Scheimpflug tilt the circle of confusion depends on the
//! point's *full* position relative to the tilted plane of best focus, and a
//! lateral coordinate cannot be reconstructed from a scalar.

use serde::{Deserialize, Serialize};
use vision_calibration_core::{CameraParams, SensorParams};

use crate::optics::coc::{
    PlaneOfBestFocus, coc_diameter_at_sensor, coc_to_pixels, sensor_distance,
};
use nalgebra::Point3;

/// The Scheimpflug tilt `(tau_x, tau_y)` of a camera, in radians.
///
/// Reads the tilt straight out of a [`CameraParams`] spec. Returns `(0, 0)`
/// for any sensor model that is not [`SensorParams::Scheimpflug`] — an
/// identity or a raw-homography sensor is treated as fronto-parallel, because
/// only the `Scheimpflug` variant carries recoverable tilt angles. (A raw
/// homography *might* encode a tilt, but the angles cannot be recovered from
/// it unambiguously, so it is correctly treated as having no known tilt.)
#[must_use]
pub fn scheimpflug_tilt(params: &CameraParams) -> (f64, f64) {
    match &params.sensor {
        SensorParams::Scheimpflug { params } => (params.tilt_x, params.tilt_y),
        SensorParams::Identity | SensorParams::Homography { .. } => (0.0, 0.0),
    }
}

/// A thick-lens optical design: a camera spec plus its physical lens
/// parameters.
///
/// This is the "user-chosen design" the M4 defocus model operates on. It pairs
/// the [`CameraParams`] projection/sensor spec (which retains the Scheimpflug
/// tilt — see the module doc-comment) with the four scalar thick-lens
/// parameters and the sensor pixel pitch.
///
/// All lengths are SI metres. The coordinate and reference-plane conventions
/// are those of [`crate::optics::coc`]: the optical axis is camera-local
/// `+z` toward the object, and the camera-local origin is the rear principal
/// plane `H'`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThickLens {
    /// The camera projection / distortion / sensor / intrinsics spec.
    /// Retained as `CameraParams` (not a built `CameraModel`) so the
    /// Scheimpflug tilt angles survive — see the module doc-comment.
    params: CameraParams,
    /// Effective focal length `f`, in metres.
    focal_length_m: f64,
    /// f-number `N` (dimensionless). Aperture diameter is `f / N`.
    f_number: f64,
    /// Object-side focus distance `s_o`, in metres, measured from the rear
    /// principal plane `H'`.
    focus_distance_m: f64,
    /// Inter-principal gap `g = H - H'`, in metres, `>= 0`. Zero makes the
    /// lens thin (the two principal planes coincide).
    principal_gap_m: f64,
    /// Sensor pixel pitch (centre-to-centre pixel spacing), in metres.
    pixel_pitch_m: f64,
}

impl ThickLens {
    /// Build a thick-lens design, validating every physical parameter.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidInput`](crate::Error::InvalidInput) if any of:
    /// - `focal_length_m` is not finite and strictly positive;
    /// - `f_number` is not finite and strictly positive;
    /// - `focus_distance_m` is not finite, or not strictly greater than
    ///   `focal_length_m` (an object at or inside the front focal point forms
    ///   no real image);
    /// - `principal_gap_m` is not finite or is negative;
    /// - `pixel_pitch_m` is not finite and strictly positive.
    ///
    /// The combination is additionally checked: `focus_distance_m + gap` must
    /// exceed `focal_length_m` so the focus plane has a real conjugate.
    pub fn new(
        params: CameraParams,
        focal_length_m: f64,
        f_number: f64,
        focus_distance_m: f64,
        principal_gap_m: f64,
        pixel_pitch_m: f64,
    ) -> crate::Result<Self> {
        if !(focal_length_m.is_finite() && focal_length_m > 0.0) {
            return Err(crate::Error::InvalidInput {
                reason: format!("focal length must be finite and positive, got {focal_length_m}"),
            });
        }
        if !(f_number.is_finite() && f_number > 0.0) {
            return Err(crate::Error::InvalidInput {
                reason: format!("f-number must be finite and positive, got {f_number}"),
            });
        }
        if !(principal_gap_m.is_finite() && principal_gap_m >= 0.0) {
            return Err(crate::Error::InvalidInput {
                reason: format!(
                    "inter-principal gap must be finite and non-negative, got {principal_gap_m}"
                ),
            });
        }
        if !focus_distance_m.is_finite() {
            return Err(crate::Error::InvalidInput {
                reason: format!("focus distance must be finite, got {focus_distance_m}"),
            });
        }
        if focus_distance_m + principal_gap_m <= focal_length_m {
            return Err(crate::Error::InvalidInput {
                reason: format!(
                    "focus distance from H (s_o + gap = {} m) must exceed the focal length \
                     ({focal_length_m} m) to form a real image",
                    focus_distance_m + principal_gap_m
                ),
            });
        }
        if !(pixel_pitch_m.is_finite() && pixel_pitch_m > 0.0) {
            return Err(crate::Error::InvalidInput {
                reason: format!("pixel pitch must be finite and positive, got {pixel_pitch_m}"),
            });
        }
        Ok(Self {
            params,
            focal_length_m,
            f_number,
            focus_distance_m,
            principal_gap_m,
            pixel_pitch_m,
        })
    }

    /// The camera spec (projection / distortion / sensor / intrinsics).
    ///
    /// Call `.build()` on the returned value to obtain a runtime
    /// [`CameraModel`](vision_calibration_core::CameraModel) for the projection
    /// chain.
    #[must_use]
    pub fn params(&self) -> &CameraParams {
        &self.params
    }

    /// Effective focal length `f`, in metres.
    #[must_use]
    pub fn focal_length_m(&self) -> f64 {
        self.focal_length_m
    }

    /// f-number `N` (dimensionless).
    #[must_use]
    pub fn f_number(&self) -> f64 {
        self.f_number
    }

    /// Object-side focus distance `s_o`, in metres (from the rear principal
    /// plane `H'`).
    #[must_use]
    pub fn focus_distance_m(&self) -> f64 {
        self.focus_distance_m
    }

    /// Inter-principal gap `g = H - H'`, in metres.
    #[must_use]
    pub fn principal_gap_m(&self) -> f64 {
        self.principal_gap_m
    }

    /// Sensor pixel pitch, in metres.
    #[must_use]
    pub fn pixel_pitch_m(&self) -> f64 {
        self.pixel_pitch_m
    }

    /// Aperture (entrance-pupil) diameter `D = f / N`, in metres.
    #[must_use]
    pub fn aperture_diameter_m(&self) -> f64 {
        self.focal_length_m / self.f_number
    }

    /// The Scheimpflug tilt `(tau_x, tau_y)` of this lens's sensor, in radians.
    ///
    /// Read straight from the retained [`CameraParams`] — see
    /// [`scheimpflug_tilt`].
    #[must_use]
    pub fn sensor_tilt(&self) -> (f64, f64) {
        scheimpflug_tilt(&self.params)
    }

    /// The on-axis sensor distance from the rear principal plane `H'`, metres.
    ///
    /// This is the image-side conjugate of the focus plane — where the centre
    /// of the sensor sits.
    ///
    /// # Errors
    ///
    /// Propagates [`sensor_distance`]'s errors (degenerate focus geometry).
    pub fn sensor_distance_m(&self) -> crate::Result<f64> {
        sensor_distance(
            self.focus_distance_m,
            self.principal_gap_m,
            self.focal_length_m,
        )
    }

    /// The object-side plane of best focus.
    ///
    /// Fronto-parallel at depth `s_o` when the sensor is untilted; a tilted
    /// plane through `(0, 0, s_o)` under Scheimpflug tilt. See
    /// [`PlaneOfBestFocus`].
    ///
    /// # Errors
    ///
    /// Propagates [`PlaneOfBestFocus::new`]'s errors.
    pub fn plane_of_best_focus(&self) -> crate::Result<PlaneOfBestFocus> {
        let (tau_x, tau_y) = self.sensor_tilt();
        PlaneOfBestFocus::new(
            tau_x,
            tau_y,
            self.focus_distance_m,
            self.principal_gap_m,
            self.focal_length_m,
        )
    }

    /// The geometric circle-of-confusion diameter **at the sensor**, in metres,
    /// for an object point in the camera frame.
    ///
    /// `point_camera` is in camera-local coordinates (metres): `+z` toward the
    /// object, origin at the rear principal plane `H'`. The result is `0` for
    /// a point on the plane of best focus and grows with defocus; under
    /// Scheimpflug tilt the in-focus locus is the tilted PoBF, so the CoC then
    /// depends on the point's full 3D position. With an untilted sensor it
    /// depends on `point_camera.z` alone.
    ///
    /// This is the upstream-promotion-candidate method — see the module
    /// doc-comment.
    ///
    /// # Errors
    ///
    /// Propagates [`coc_diameter_at_sensor`]'s errors — chiefly an object
    /// point at or inside the front focal point (no real image), or a
    /// non-finite coordinate.
    pub fn coc_diameter(&self, point_camera: &Point3<f64>) -> crate::Result<f64> {
        let (tau_x, tau_y) = self.sensor_tilt();
        coc_diameter_at_sensor(
            point_camera,
            self.focal_length_m,
            self.f_number,
            self.focus_distance_m,
            self.principal_gap_m,
            tau_x,
            tau_y,
        )
    }

    /// The geometric circle-of-confusion diameter **in pixels** for an object
    /// point in the camera frame.
    ///
    /// Equal to [`ThickLens::coc_diameter`] divided by the pixel pitch. This is
    /// the quantity a focus heatmap and a depth-of-field criterion consume.
    ///
    /// # Errors
    ///
    /// Propagates [`ThickLens::coc_diameter`]'s and [`coc_to_pixels`]'s errors.
    pub fn coc_diameter_px(&self, point_camera: &Point3<f64>) -> crate::Result<f64> {
        let coc_m = self.coc_diameter(point_camera)?;
        coc_to_pixels(coc_m, self.pixel_pitch_m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use vision_calibration_core::{
        DistortionParams, FxFyCxCySkew, IntrinsicsParams, ProjectionParams, ScheimpflugParams,
    };

    // The same realistic machine-vision lens used in `coc.rs`'s tests:
    //   f = 25 mm, N = 4, s_o = 0.30 m, pixel pitch p = 3.45 um.
    const F: f64 = 0.025;
    const N: f64 = 4.0;
    const S_O: f64 = 0.30;
    const PIX: f64 = 3.45e-6;

    /// `CameraParams` with a Scheimpflug sensor at the given tilt (radians).
    fn params_with_tilt(tau_x: f64, tau_y: f64) -> CameraParams {
        // Intrinsics consistent with the physical lens: fx = fy = f / pitch.
        let fx = F / PIX;
        CameraParams {
            projection: ProjectionParams::Pinhole,
            distortion: DistortionParams::None,
            sensor: SensorParams::Scheimpflug {
                params: ScheimpflugParams {
                    tilt_x: tau_x,
                    tilt_y: tau_y,
                },
            },
            intrinsics: IntrinsicsParams::FxFyCxCySkew {
                params: FxFyCxCySkew {
                    fx,
                    fy: fx,
                    cx: 640.0,
                    cy: 512.0,
                    skew: 0.0,
                },
            },
        }
    }

    fn lens(tau_x: f64, tau_y: f64, gap: f64) -> ThickLens {
        ThickLens::new(params_with_tilt(tau_x, tau_y), F, N, S_O, gap, PIX)
            .expect("valid thick-lens design")
    }

    #[test]
    fn scheimpflug_tilt_is_read_from_camera_params() {
        let p = params_with_tilt(0.03, -0.02);
        assert_eq!(scheimpflug_tilt(&p), (0.03, -0.02));

        // A non-Scheimpflug sensor reports no tilt.
        let identity = CameraParams {
            sensor: SensorParams::Identity,
            ..params_with_tilt(0.0, 0.0)
        };
        assert_eq!(scheimpflug_tilt(&identity), (0.0, 0.0));
    }

    #[test]
    fn new_validates_physical_parameters() {
        let p = params_with_tilt(0.0, 0.0);
        // Bad focal length.
        assert!(ThickLens::new(p.clone(), 0.0, N, S_O, 0.0, PIX).is_err());
        // Bad f-number.
        assert!(ThickLens::new(p.clone(), F, -1.0, S_O, 0.0, PIX).is_err());
        // Focus distance inside the focal point.
        assert!(ThickLens::new(p.clone(), F, N, 0.5 * F, 0.0, PIX).is_err());
        // Negative inter-principal gap.
        assert!(ThickLens::new(p.clone(), F, N, S_O, -0.01, PIX).is_err());
        // Bad pixel pitch.
        assert!(ThickLens::new(p.clone(), F, N, S_O, 0.0, 0.0).is_err());
        // A valid design.
        assert!(ThickLens::new(p, F, N, S_O, 0.0, PIX).is_ok());
    }

    #[test]
    fn accessors_and_aperture() {
        let l = lens(0.0, 0.0, 0.0);
        assert_relative_eq!(l.focal_length_m(), F, epsilon = 1e-15);
        assert_relative_eq!(l.f_number(), N, epsilon = 1e-15);
        assert_relative_eq!(l.focus_distance_m(), S_O, epsilon = 1e-15);
        assert_relative_eq!(l.pixel_pitch_m(), PIX, epsilon = 1e-15);
        // D = f / N = 0.025 / 4 = 0.00625 m.
        assert_relative_eq!(l.aperture_diameter_m(), 0.00625, epsilon = 1e-15);
    }

    #[test]
    fn coc_diameter_delegates_to_the_physics_with_the_retained_tilt() {
        // tau = 0: defocused point at z = 0.33 must give the textbook CoC
        // (the same 51.65 um the coc.rs regime-b test hand-computes).
        let flat = lens(0.0, 0.0, 0.0);
        let p = Point3::new(0.0, 0.0, 0.33);
        let c = flat.coc_diameter(&p).unwrap();
        assert_relative_eq!(c, 5.165_289_256_198_392e-5, epsilon = 1e-15);
        // In pixels: 14.9719 px.
        assert_relative_eq!(
            flat.coc_diameter_px(&p).unwrap(),
            14.971_852_916_517_08,
            epsilon = 1e-11
        );

        // A point on the focus plane is sharp.
        assert_relative_eq!(
            flat.coc_diameter(&Point3::new(0.0, 0.0, S_O)).unwrap(),
            0.0,
            epsilon = 1e-15
        );
    }

    #[test]
    fn tilted_lens_uses_the_tilt_for_the_plane_of_best_focus() {
        // A 5-deg tilted sensor: a point on the tilted PoBF must be sharp,
        // proving ThickLens actually pulls the tilt out of CameraParams.
        let tau_x = 5.0_f64.to_radians();
        let tilted = lens(tau_x, 0.0, 0.0);
        assert_eq!(tilted.sensor_tilt(), (tau_x, 0.0));

        let pobf = tilted.plane_of_best_focus().unwrap();
        let y = 0.05;
        let z_on = pobf.depth_at(0.0, y);
        let c_on = tilted.coc_diameter(&Point3::new(0.0, y, z_on)).unwrap();
        assert_relative_eq!(c_on, 0.0, epsilon = 1e-12);

        // The same point is blurred for an untilted lens.
        let flat = lens(0.0, 0.0, 0.0);
        let c_flat = flat.coc_diameter(&Point3::new(0.0, y, z_on)).unwrap();
        assert!(c_flat > 1e-6);
    }

    #[test]
    fn sensor_distance_and_pobf_are_consistent_with_coc_module() {
        let l = lens(0.0, 0.0, 0.0);
        let v0 = l.sensor_distance_m().unwrap();
        assert_relative_eq!(v0, F * S_O / (S_O - F), epsilon = 1e-15);
        let pobf = l.plane_of_best_focus().unwrap();
        // Untilted: PoBF is fronto-parallel at s_o.
        assert_relative_eq!(pobf.depth_at(0.3, -0.2), S_O, epsilon = 1e-15);
    }

    #[test]
    fn round_trips_through_json() {
        let l = lens(0.04, -0.01, 0.008);
        let json = serde_json::to_string(&l).expect("serialize");
        let back: ThickLens = serde_json::from_str(&json).expect("deserialize");
        assert_relative_eq!(back.focal_length_m(), F, epsilon = 1e-15);
        assert_relative_eq!(back.principal_gap_m(), 0.008, epsilon = 1e-15);
        assert_eq!(back.sensor_tilt(), (0.04, -0.01));
    }
}
