//! The three scene entity types: camera, laser, and target.
//!
//! Each entity is a *posed* optical or geometric object. A [`CameraEntity`] is
//! a simulated triangulation sensor, a [`LaserEntity`] a line-projecting laser,
//! and a [`TargetEntity`] the planar surface being inspected. All three carry
//! the parameters that the UI binds sliders to in M3 and that the optics,
//! laser, and analysis layers consume in M4–M6.
//!
//! # Coordinate conventions
//!
//! - **World frame.** Right-handed, `+Z`-up. This matches the viewport's
//!   `OrbitCamera` (`etendue-ui`) and the ground grid lying in the `z = 0`
//!   plane. Entity poses place objects in this frame.
//! - **Entity-local frame.** Every entity owns a `pose: Isometry3<f64>` that
//!   maps a point expressed in the entity's *local* frame into the world
//!   frame (`p_world = pose * p_local`). The inverse, `pose.inverse()`, takes
//!   world points into the entity's local frame.
//! - **Camera-local frame.** A [`CameraEntity`] uses the `vision-calibration-core`
//!   camera convention: the camera looks down its local **`+z`** axis, `+x`
//!   points right in the image, `+y` points down. `Camera::project_point_c`
//!   rejects points with `z <= 0` (behind the camera) and
//!   `Camera::backproject_pixel` returns a ray as a point on the local `z = 1`
//!   plane. `CameraEntity::pose` is therefore "world ← camera-local".
//! - **Laser-local frame.** A [`LaserEntity`] emits a fan in its local
//!   `x = 0` plane (the *fan plane*): the fan opens symmetrically about the
//!   local `+z` axis (the fan's central ray), spreading within the `y–z`
//!   plane. `LaserEntity::pose` is "world ← laser-local".
//! - **Target-local frame.** A [`TargetEntity`] is a rectangle in its local
//!   `z = 0` plane, centred on the local origin, with its outward normal along
//!   local `+z`. `TargetEntity::pose` is "world ← target-local".
//!
//! Units are SI: metres for lengths, radians for angles. Wavelength is the one
//! exception and is stored in nanometres (the unit lasers are specified in).

use nalgebra::Isometry3;
use serde::{Deserialize, Serialize};
use vision_calibration_core::{CameraParams, IntrinsicsParams};

use crate::optics::ThickLens;

/// A simulated triangulation camera placed in the world.
///
/// # Why `CameraParams`, not `CameraModel`
///
/// This entity stores the *serializable parameter spec* ([`CameraParams`]) as
/// the projection source of truth — **not** a built
/// [`CameraModel`](vision_calibration_core::CameraModel). The reason is
/// Scheimpflug tilt: `CameraParams::build()` compiles a `SensorParams::Scheimpflug`
/// into an `AnySensor::Homography`, and the built model no longer carries the
/// tilt angles `(tau_x, tau_y)`. The M4 defocus model needs those angles to
/// place the tilted plane of best focus, so they must be retained here.
/// Consumers that only need the projection chain call [`CameraParams::build`]
/// to obtain a `CameraModel` on demand.
///
/// # Physical optics parameters are the source of truth
///
/// M4 adds five **physical** optics fields ([`effective_focal_length_m`],
/// [`f_number`], [`focus_distance_m`], [`principal_gap_m`], [`pixel_pitch_m`]).
/// These — not the pixel-unit intrinsics — are authoritative for the lens.
/// The pinhole focal length in `params.intrinsics` is *derived* from them:
///
/// ```text
/// fx = fy = effective_focal_length_m / pixel_pitch_m
/// ```
///
/// [`CameraEntity::new`] establishes this consistency at construction (via
/// [`CameraEntity::sync_intrinsics_from_physical`]), and
/// [`CameraEntity::thick_lens`] hands the physical parameters to the M4
/// defocus model. A UI that edits the physical fields must call
/// [`CameraEntity::sync_intrinsics_from_physical`] afterwards to keep the
/// derived intrinsics in step.
///
/// [`effective_focal_length_m`]: CameraEntity::effective_focal_length_m
/// [`f_number`]: CameraEntity::f_number
/// [`focus_distance_m`]: CameraEntity::focus_distance_m
/// [`principal_gap_m`]: CameraEntity::principal_gap_m
/// [`pixel_pitch_m`]: CameraEntity::pixel_pitch_m
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CameraEntity {
    /// Pose of the camera in the world: maps camera-local coordinates into
    /// world coordinates (`world ← camera-local`). The camera looks down its
    /// local `+z` axis.
    pub pose: Isometry3<f64>,
    /// The camera's projection/distortion/sensor/intrinsics parameter spec.
    /// Retained (rather than a built `CameraModel`) so Scheimpflug tilt angles
    /// survive for the M4 defocus model — see the type-level note above.
    ///
    /// The pinhole focal length here is **derived** from the physical optics
    /// fields; treat the physical fields as authoritative and call
    /// [`CameraEntity::sync_intrinsics_from_physical`] after editing them.
    pub params: CameraParams,
    /// Image sensor resolution in pixels, `(width, height)`.
    pub resolution: (u32, u32),
    /// Effective focal length of the lens, in **metres** (e.g. `0.025` for a
    /// 25 mm lens). Physical source of truth — see the type-level note. The
    /// pixel-unit `params.intrinsics.fx`/`fy` are derived from this and
    /// [`pixel_pitch_m`](CameraEntity::pixel_pitch_m).
    pub effective_focal_length_m: f64,
    /// Lens f-number `N` (dimensionless, e.g. `4.0`). The aperture diameter is
    /// `effective_focal_length_m / f_number`. Used by the M4 defocus model.
    pub f_number: f64,
    /// Object-side focus distance, in metres — the distance at which the lens
    /// is focused, measured from the rear principal plane. Used by the M4
    /// defocus model to place the plane of best focus.
    pub focus_distance_m: f64,
    /// Inter-principal-plane separation `H - H'`, in metres, `>= 0`. Zero
    /// makes the lens optically thin. Used by the M4 thick-lens model.
    pub principal_gap_m: f64,
    /// Sensor pixel pitch (centre-to-centre pixel spacing), in metres (e.g.
    /// `3.45e-6` for a 3.45 µm pixel). Couples the physical focal length to
    /// the pixel-unit intrinsics, and converts the circle of confusion to
    /// pixels in the M4 model.
    pub pixel_pitch_m: f64,
    /// Near clip distance for the rendered frustum, in metres (camera-local
    /// `+z`). Visualization only — not an optical aperture.
    pub frustum_near: f64,
    /// Far clip distance for the rendered frustum, in metres (camera-local
    /// `+z`).
    pub frustum_far: f64,
}

impl CameraEntity {
    /// Build a camera entity from a pose, a projection spec, and the physical
    /// optics parameters.
    ///
    /// The pinhole focal length in `params.intrinsics` is **overwritten** so
    /// it is consistent with `effective_focal_length_m` and `pixel_pitch_m`
    /// (`fx = fy = focal / pitch`) — the physical parameters are the source of
    /// truth (see the type-level note). Any `fx`/`fy` already in `params` is
    /// therefore discarded; `cx`, `cy`, and `skew` are preserved.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidInput`](crate::Error::InvalidInput) if any of:
    /// - either resolution dimension is zero;
    /// - `effective_focal_length_m` is not finite and strictly positive;
    /// - `f_number` is not finite and strictly positive;
    /// - `focus_distance_m` is not finite, or not strictly greater than
    ///   `effective_focal_length_m` (an object inside the focal point forms no
    ///   real image);
    /// - `principal_gap_m` is not finite or is negative;
    /// - `pixel_pitch_m` is not finite and strictly positive;
    /// - `frustum_near` is not finite and strictly positive;
    /// - `frustum_far` is not strictly greater than `frustum_near`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pose: Isometry3<f64>,
        params: CameraParams,
        resolution: (u32, u32),
        effective_focal_length_m: f64,
        f_number: f64,
        focus_distance_m: f64,
        principal_gap_m: f64,
        pixel_pitch_m: f64,
        frustum_near: f64,
        frustum_far: f64,
    ) -> crate::Result<Self> {
        if resolution.0 == 0 || resolution.1 == 0 {
            return Err(crate::Error::InvalidInput {
                reason: format!(
                    "camera resolution must be non-zero in both axes, got {}x{}",
                    resolution.0, resolution.1
                ),
            });
        }
        if !(effective_focal_length_m.is_finite() && effective_focal_length_m > 0.0) {
            return Err(crate::Error::InvalidInput {
                reason: format!(
                    "effective focal length must be finite and positive, got \
                     {effective_focal_length_m}"
                ),
            });
        }
        if !(f_number.is_finite() && f_number > 0.0) {
            return Err(crate::Error::InvalidInput {
                reason: format!("f-number must be finite and positive, got {f_number}"),
            });
        }
        if !focus_distance_m.is_finite() {
            return Err(crate::Error::InvalidInput {
                reason: format!("focus distance must be finite, got {focus_distance_m}"),
            });
        }
        if focus_distance_m <= effective_focal_length_m {
            return Err(crate::Error::InvalidInput {
                reason: format!(
                    "focus distance ({focus_distance_m} m) must exceed the focal length \
                     ({effective_focal_length_m} m) to form a real image"
                ),
            });
        }
        if !(principal_gap_m.is_finite() && principal_gap_m >= 0.0) {
            return Err(crate::Error::InvalidInput {
                reason: format!(
                    "inter-principal gap must be finite and non-negative, got {principal_gap_m}"
                ),
            });
        }
        if !(pixel_pitch_m.is_finite() && pixel_pitch_m > 0.0) {
            return Err(crate::Error::InvalidInput {
                reason: format!("pixel pitch must be finite and positive, got {pixel_pitch_m}"),
            });
        }
        if !(frustum_near.is_finite() && frustum_near > 0.0) {
            return Err(crate::Error::InvalidInput {
                reason: format!("frustum near must be finite and positive, got {frustum_near}"),
            });
        }
        if !(frustum_far.is_finite() && frustum_far > frustum_near) {
            return Err(crate::Error::InvalidInput {
                reason: format!(
                    "frustum far ({frustum_far}) must be finite and greater than near \
                     ({frustum_near})"
                ),
            });
        }
        let mut entity = Self {
            pose,
            params,
            resolution,
            effective_focal_length_m,
            f_number,
            focus_distance_m,
            principal_gap_m,
            pixel_pitch_m,
            frustum_near,
            frustum_far,
        };
        // Make the derived pixel-unit intrinsics consistent with the physical
        // optics parameters — the physical fields are authoritative.
        entity.sync_intrinsics_from_physical();
        Ok(entity)
    }

    /// Image resolution as `(width, height)` in `f64`, for arithmetic against
    /// pixel coordinates.
    #[must_use]
    pub fn resolution_f64(&self) -> (f64, f64) {
        (f64::from(self.resolution.0), f64::from(self.resolution.1))
    }

    /// The pinhole focal length in pixels derived from the physical optics
    /// parameters: `effective_focal_length_m / pixel_pitch_m`.
    ///
    /// This is the single conversion that ties the physical lens to the
    /// pixel-unit `vision-calibration-core` intrinsics. With a 25 mm lens on a
    /// 3.45 µm pixel it is `0.025 / 3.45e-6 ≈ 7246.4` pixels.
    #[must_use]
    pub fn focal_length_px(&self) -> f64 {
        self.effective_focal_length_m / self.pixel_pitch_m
    }

    /// Overwrite the pinhole focal length in `params.intrinsics` so it agrees
    /// with the physical optics parameters (`fx = fy = `
    /// [`focal_length_px`](CameraEntity::focal_length_px)).
    ///
    /// The physical fields ([`effective_focal_length_m`] /
    /// [`pixel_pitch_m`]) are the source of truth; this helper propagates an
    /// edit of either of them into the derived intrinsics. `cx`, `cy`, and
    /// `skew` are left untouched — only `fx` and `fy` are rewritten.
    ///
    /// [`CameraEntity::new`] calls this once at construction; a UI editing the
    /// physical fields must call it after each edit.
    ///
    /// [`effective_focal_length_m`]: CameraEntity::effective_focal_length_m
    /// [`pixel_pitch_m`]: CameraEntity::pixel_pitch_m
    pub fn sync_intrinsics_from_physical(&mut self) {
        let fx = self.focal_length_px();
        let IntrinsicsParams::FxFyCxCySkew { params } = &mut self.params.intrinsics;
        params.fx = fx;
        params.fy = fx;
    }

    /// Build the M4 [`ThickLens`] defocus model for this camera.
    ///
    /// Bundles the retained [`CameraParams`] (which carries the Scheimpflug
    /// tilt) with this entity's physical optics parameters. Use it for the
    /// circle-of-confusion / plane-of-best-focus computations.
    ///
    /// # Errors
    ///
    /// Propagates [`ThickLens::new`]'s validation errors. These cannot fire
    /// for an entity built through [`CameraEntity::new`] (the same physical
    /// parameters are validated there), but a hand-mutated entity with an
    /// inconsistent field can still be rejected here.
    pub fn thick_lens(&self) -> crate::Result<ThickLens> {
        ThickLens::new(
            self.params.clone(),
            self.effective_focal_length_m,
            self.f_number,
            self.focus_distance_m,
            self.principal_gap_m,
            self.pixel_pitch_m,
        )
    }
}

/// A line-projecting laser placed in the world.
///
/// The MVP laser is a flat fan with a half-angle and a finite extent, plus the
/// one physical-optics parameter the M5 line model needs: the Gaussian-beam
/// waist. The geometric line model itself — a [`LaserPlane`] with a
/// Gaussian-beam cross-section width — lives in [`crate::laser`]; this entity
/// only carries the parameters it consumes.
///
/// [`LaserPlane`]: crate::laser::LaserPlane
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LaserEntity {
    /// Pose of the laser in the world: maps laser-local coordinates into world
    /// coordinates (`world ← laser-local`). The fan lies in the local `x = 0`
    /// plane and opens about the local `+z` axis.
    pub pose: Isometry3<f64>,
    /// Half-angle of the fan, in radians: the fan spans `±fan_half_angle`
    /// about the local `+z` axis within the local `y–z` plane.
    pub fan_half_angle: f64,
    /// Length of the fan along its central `+z` ray, in metres — how far the
    /// rendered fan reaches from the laser origin.
    pub fan_length: f64,
    /// Emission wavelength, in nanometres (e.g. `660.0` for a red diode).
    pub wavelength_nm: f64,
    /// Beam-waist radius `w0`, in **metres** — the `1/e²` radius of the laser
    /// beam's cross-section at its narrowest point. The M5
    /// [`GaussianBeamWidth`](crate::laser::GaussianBeamWidth) model places the
    /// waist at the laser origin and grows the cross-section away from it via
    /// `w(d) = w0·sqrt(1 + (d/zR)²)`, `zR = π·w0²/λ`. A typical machine-vision
    /// line laser focuses to a waist of a few hundred micrometres (e.g.
    /// `2.5e-4` m for a 0.25 mm waist).
    pub beam_waist_m: f64,
}

impl LaserEntity {
    /// Build a laser entity, validating the fan geometry, wavelength, and
    /// beam waist.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidInput`](crate::Error::InvalidInput) if
    /// `fan_half_angle` is not finite or not in the open interval
    /// `(0, π/2)`, if `fan_length` is not finite and strictly positive, if
    /// `wavelength_nm` is not finite and strictly positive, or if
    /// `beam_waist_m` is not finite and strictly positive.
    pub fn new(
        pose: Isometry3<f64>,
        fan_half_angle: f64,
        fan_length: f64,
        wavelength_nm: f64,
        beam_waist_m: f64,
    ) -> crate::Result<Self> {
        if !(fan_half_angle.is_finite()
            && fan_half_angle > 0.0
            && fan_half_angle < std::f64::consts::FRAC_PI_2)
        {
            return Err(crate::Error::InvalidInput {
                reason: format!(
                    "laser fan half-angle must be finite and in (0, π/2), got {fan_half_angle}"
                ),
            });
        }
        if !(fan_length.is_finite() && fan_length > 0.0) {
            return Err(crate::Error::InvalidInput {
                reason: format!("laser fan length must be finite and positive, got {fan_length}"),
            });
        }
        if !(wavelength_nm.is_finite() && wavelength_nm > 0.0) {
            return Err(crate::Error::InvalidInput {
                reason: format!(
                    "laser wavelength must be finite and positive, got {wavelength_nm}"
                ),
            });
        }
        if !(beam_waist_m.is_finite() && beam_waist_m > 0.0) {
            return Err(crate::Error::InvalidInput {
                reason: format!(
                    "laser beam-waist radius must be finite and positive, got {beam_waist_m}"
                ),
            });
        }
        Ok(Self {
            pose,
            fan_half_angle,
            fan_length,
            wavelength_nm,
            beam_waist_m,
        })
    }
}

/// A planar inspection target placed in the world.
///
/// The MVP target is a finite rectangle. Parametric sphere / cylinder / step
/// primitives are post-MVP (`geom/primitives.rs`) and are not modelled here.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TargetEntity {
    /// Pose of the target in the world: maps target-local coordinates into
    /// world coordinates (`world ← target-local`). The target rectangle lies
    /// in the local `z = 0` plane with its outward normal along local `+z`.
    pub pose: Isometry3<f64>,
    /// Width of the target rectangle (local `x` extent), in metres.
    pub width: f64,
    /// Height of the target rectangle (local `y` extent), in metres.
    pub height: f64,
}

impl TargetEntity {
    /// Build a target entity, validating the rectangle extent.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidInput`](crate::Error::InvalidInput) if `width`
    /// or `height` is not finite and strictly positive.
    pub fn new(pose: Isometry3<f64>, width: f64, height: f64) -> crate::Result<Self> {
        if !(width.is_finite() && width > 0.0) {
            return Err(crate::Error::InvalidInput {
                reason: format!("target width must be finite and positive, got {width}"),
            });
        }
        if !(height.is_finite() && height > 0.0) {
            return Err(crate::Error::InvalidInput {
                reason: format!("target height must be finite and positive, got {height}"),
            });
        }
        Ok(Self {
            pose,
            width,
            height,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use nalgebra::{Point3, Translation3, UnitQuaternion, Vector3};
    use vision_calibration_core::{
        DistortionParams, FxFyCxCySkew, IntrinsicsParams, ProjectionParams, SensorParams,
    };

    /// A trivial pinhole `CameraParams` for entity tests. The `fx`/`fy` here
    /// are deliberately wrong values — `CameraEntity::new` overwrites them
    /// from the physical focal length and pitch.
    fn pinhole_params() -> CameraParams {
        CameraParams {
            projection: ProjectionParams::Pinhole,
            distortion: DistortionParams::None,
            sensor: SensorParams::Identity,
            intrinsics: IntrinsicsParams::FxFyCxCySkew {
                params: FxFyCxCySkew {
                    fx: 1200.0,
                    fy: 1200.0,
                    cx: 640.0,
                    cy: 360.0,
                    skew: 0.0,
                },
            },
        }
    }

    /// A realistic machine-vision lens: 25 mm, f/4, focused at 0.3 m, thin
    /// (zero principal gap), on a 3.45 µm-pitch sensor.
    const TEST_F_M: f64 = 0.025;
    const TEST_FNUM: f64 = 4.0;
    const TEST_FOCUS_M: f64 = 0.30;
    const TEST_GAP_M: f64 = 0.0;
    const TEST_PITCH_M: f64 = 3.45e-6;

    /// Build a camera entity with the realistic test lens parameters.
    fn camera_entity(
        params: CameraParams,
        resolution: (u32, u32),
        near: f64,
        far: f64,
    ) -> crate::Result<CameraEntity> {
        CameraEntity::new(
            Isometry3::identity(),
            params,
            resolution,
            TEST_F_M,
            TEST_FNUM,
            TEST_FOCUS_M,
            TEST_GAP_M,
            TEST_PITCH_M,
            near,
            far,
        )
    }

    #[test]
    fn camera_entity_accepts_a_valid_spec() {
        let cam =
            camera_entity(pinhole_params(), (1280, 720), 0.1, 1.0).expect("valid camera entity");
        assert_eq!(cam.resolution, (1280, 720));
        let (w, h) = cam.resolution_f64();
        assert_relative_eq!(w, 1280.0, epsilon = 1e-12);
        assert_relative_eq!(h, 720.0, epsilon = 1e-12);
    }

    #[test]
    fn camera_entity_derives_intrinsics_from_physical_focal_length() {
        // fx = fy = focal / pitch = 0.025 / 3.45e-6 ≈ 7246.3768 px, regardless
        // of the (deliberately wrong) 1200.0 in `pinhole_params`.
        let cam = camera_entity(pinhole_params(), (1280, 720), 0.1, 1.0).unwrap();
        let IntrinsicsParams::FxFyCxCySkew { params } = &cam.params.intrinsics;
        let expected_fx = TEST_F_M / TEST_PITCH_M;
        assert_relative_eq!(params.fx, expected_fx, epsilon = 1e-9);
        assert_relative_eq!(params.fy, expected_fx, epsilon = 1e-9);
        assert_relative_eq!(cam.focal_length_px(), expected_fx, epsilon = 1e-9);
        // cx / cy must be preserved untouched.
        assert_relative_eq!(params.cx, 640.0, epsilon = 1e-12);
        assert_relative_eq!(params.cy, 360.0, epsilon = 1e-12);
    }

    #[test]
    fn sync_intrinsics_from_physical_tracks_an_edit() {
        let mut cam = camera_entity(pinhole_params(), (1280, 720), 0.1, 1.0).unwrap();
        // Edit the physical focal length, then re-sync.
        cam.effective_focal_length_m = 0.050;
        cam.sync_intrinsics_from_physical();
        let IntrinsicsParams::FxFyCxCySkew { params } = &cam.params.intrinsics;
        assert_relative_eq!(params.fx, 0.050 / TEST_PITCH_M, epsilon = 1e-9);
        assert_relative_eq!(params.fy, 0.050 / TEST_PITCH_M, epsilon = 1e-9);
    }

    #[test]
    fn camera_entity_builds_a_thick_lens() {
        let cam = camera_entity(pinhole_params(), (1280, 720), 0.1, 1.0).unwrap();
        let lens = cam.thick_lens().expect("thick lens from valid entity");
        assert_relative_eq!(lens.focal_length_m(), TEST_F_M, epsilon = 1e-15);
        assert_relative_eq!(lens.f_number(), TEST_FNUM, epsilon = 1e-15);
        assert_relative_eq!(lens.focus_distance_m(), TEST_FOCUS_M, epsilon = 1e-15);
    }

    #[test]
    fn camera_entity_rejects_zero_resolution() {
        let err = camera_entity(pinhole_params(), (0, 720), 0.1, 1.0).unwrap_err();
        assert!(matches!(err, crate::Error::InvalidInput { .. }));
    }

    #[test]
    fn camera_entity_rejects_far_not_beyond_near() {
        let err = camera_entity(pinhole_params(), (1280, 720), 1.0, 1.0).unwrap_err();
        assert!(matches!(err, crate::Error::InvalidInput { .. }));
    }

    #[test]
    fn camera_entity_rejects_non_positive_near() {
        let err = camera_entity(pinhole_params(), (1280, 720), 0.0, 1.0).unwrap_err();
        assert!(matches!(err, crate::Error::InvalidInput { .. }));
    }

    #[test]
    fn camera_entity_rejects_bad_physical_optics() {
        let p = pinhole_params();
        // Focus distance inside the focal point.
        assert!(
            CameraEntity::new(
                Isometry3::identity(),
                p.clone(),
                (1280, 720),
                TEST_F_M,
                TEST_FNUM,
                0.5 * TEST_F_M,
                TEST_GAP_M,
                TEST_PITCH_M,
                0.1,
                1.0,
            )
            .is_err()
        );
        // Non-positive f-number.
        assert!(
            CameraEntity::new(
                Isometry3::identity(),
                p.clone(),
                (1280, 720),
                TEST_F_M,
                0.0,
                TEST_FOCUS_M,
                TEST_GAP_M,
                TEST_PITCH_M,
                0.1,
                1.0,
            )
            .is_err()
        );
        // Negative pixel pitch.
        assert!(
            CameraEntity::new(
                Isometry3::identity(),
                p,
                (1280, 720),
                TEST_F_M,
                TEST_FNUM,
                TEST_FOCUS_M,
                TEST_GAP_M,
                -1e-6,
                0.1,
                1.0,
            )
            .is_err()
        );
    }

    /// A plausible machine-vision line-laser beam waist: 0.25 mm radius.
    const TEST_WAIST_M: f64 = 0.25e-3;

    #[test]
    fn laser_entity_accepts_a_valid_fan() {
        let laser = LaserEntity::new(Isometry3::identity(), 0.3, 1.5, 660.0, TEST_WAIST_M)
            .expect("valid laser entity");
        assert_relative_eq!(laser.fan_half_angle, 0.3, epsilon = 1e-12);
        assert_relative_eq!(laser.fan_length, 1.5, epsilon = 1e-12);
        assert_relative_eq!(laser.beam_waist_m, TEST_WAIST_M, epsilon = 1e-15);
    }

    #[test]
    fn laser_entity_rejects_degenerate_fan_angle() {
        // Zero half-angle.
        assert!(LaserEntity::new(Isometry3::identity(), 0.0, 1.0, 660.0, TEST_WAIST_M).is_err());
        // Half-angle at/over π/2 would fold the fan back on itself.
        assert!(
            LaserEntity::new(
                Isometry3::identity(),
                std::f64::consts::FRAC_PI_2,
                1.0,
                660.0,
                TEST_WAIST_M,
            )
            .is_err()
        );
    }

    #[test]
    fn laser_entity_rejects_bad_wavelength() {
        assert!(LaserEntity::new(Isometry3::identity(), 0.3, 1.0, 0.0, TEST_WAIST_M).is_err());
        assert!(LaserEntity::new(Isometry3::identity(), 0.3, 1.0, -660.0, TEST_WAIST_M).is_err());
    }

    #[test]
    fn laser_entity_rejects_bad_beam_waist() {
        assert!(LaserEntity::new(Isometry3::identity(), 0.3, 1.0, 660.0, 0.0).is_err());
        assert!(LaserEntity::new(Isometry3::identity(), 0.3, 1.0, 660.0, -1e-4).is_err());
        assert!(LaserEntity::new(Isometry3::identity(), 0.3, 1.0, 660.0, f64::NAN).is_err());
    }

    #[test]
    fn target_entity_accepts_a_valid_rectangle() {
        let target =
            TargetEntity::new(Isometry3::identity(), 0.4, 0.3).expect("valid target entity");
        assert_relative_eq!(target.width, 0.4, epsilon = 1e-12);
        assert_relative_eq!(target.height, 0.3, epsilon = 1e-12);
    }

    #[test]
    fn target_entity_rejects_non_positive_extent() {
        assert!(TargetEntity::new(Isometry3::identity(), 0.0, 0.3).is_err());
        assert!(TargetEntity::new(Isometry3::identity(), 0.4, -0.3).is_err());
    }

    #[test]
    fn pose_maps_local_origin_to_its_translation() {
        // A pose's translation is, by construction, where the local origin
        // lands in the world — a sanity check on the "world ← local" reading.
        let translation = Translation3::new(1.0, -2.0, 0.5);
        let rotation = UnitQuaternion::from_axis_angle(&Vector3::z_axis(), 0.7);
        let pose = Isometry3::from_parts(translation, rotation);
        let world_origin = pose * Point3::origin();
        assert_relative_eq!(world_origin.x, 1.0, epsilon = 1e-12);
        assert_relative_eq!(world_origin.y, -2.0, epsilon = 1e-12);
        assert_relative_eq!(world_origin.z, 0.5, epsilon = 1e-12);
    }
}
