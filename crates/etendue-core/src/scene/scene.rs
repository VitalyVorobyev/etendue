//! The [`Scene`] aggregate: the posed cameras, lasers, and targets a design
//! consists of, plus the default MVP scene the application opens with.
//!
//! # Coordinate convention
//!
//! The scene lives in a right-handed, **`+Z`-up** world frame — the same frame
//! the `etendue-ui` viewport orbits and the ground grid lies in (`z = 0`).
//! Every entity carries an `Isometry3<f64>` pose mapping its local frame into
//! this world frame; see [`entity`](crate::scene::entity) for each entity's
//! local-frame conventions (notably: a camera looks down its local `+z`).

use nalgebra::{Isometry3, Point3, Rotation3, Translation3, UnitQuaternion, Vector3};
use serde::{Deserialize, Serialize};
use vision_calibration_core::{
    CameraParams, DistortionParams, FxFyCxCySkew, IntrinsicsParams, ProjectionParams,
    ScheimpflugParams, SensorParams,
};

use super::entity::{CameraEntity, LaserEntity, TargetEntity};

/// A complete optical-design scene: every camera, laser, and target.
///
/// M2 constructs scenes in code (see [`Scene::default_mvp`]); M3 adds the
/// editing UI and JSON load. The `Vec` fields are public so the UI can pose
/// and append entities directly.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Scene {
    /// Simulated triangulation cameras.
    pub cameras: Vec<CameraEntity>,
    /// Line-projecting lasers.
    pub lasers: Vec<LaserEntity>,
    /// Inspection targets.
    pub targets: Vec<TargetEntity>,
}

impl Scene {
    /// An empty scene — no cameras, lasers, or targets.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// The default MVP scene: one camera, one laser, one target, arranged as a
    /// plausible laser-triangulation setup.
    ///
    /// # Geometry
    ///
    /// All distances are in metres in the `+Z`-up world frame.
    ///
    /// - **Target** — a 0.15 m × 0.12 m plane standing upright, centred at
    ///   `(0, 0, 0.30)`, its face normal pointing back toward the sensor
    ///   (along world `+Y`). It is the surface the line is projected onto.
    /// - **Laser** — sits at `(0, -0.60, 0.30)`, level with the target centre
    ///   and 0.60 m in front of it, aimed straight at the target along world
    ///   `+Y`. It projects a vertical fan onto the target.
    /// - **Camera** — sits at `(0.28, -0.55, 0.30)`, offset ~0.28 m to the
    ///   side of the laser so it views the laser line from a triangulation
    ///   angle of roughly 14°. It is aimed at the same target-centre point.
    ///
    /// This is the textbook single-camera / single-laser triangulation
    /// arrangement: the baseline between laser and camera, together with the
    /// target standoff, sets the triangulation angle.
    ///
    /// # Field-of-view and defocus coherence
    ///
    /// The camera sits ~0.617 m from the target centre. A **16 mm f/2.8**
    /// lens on the 1280×1024 / 3.45 µm sensor gives a ~15.7° horizontal /
    /// ~12.6° vertical field of view, covering ~0.17 m × 0.14 m at that
    /// distance — so the 0.15 m × 0.12 m target is fully framed with a small
    /// margin. (A 25 mm lens, the earliest default, gave only a ~10° field
    /// covering ~0.11 m and left most of the target outside the frustum.)
    ///
    /// The lens is focused at the **0.617 m** camera-to-target distance, so
    /// with zero Scheimpflug tilt the plane of best focus passes through the
    /// on-axis target point. Because the upright target is oblique to the
    /// off-set camera, its depth varies across the frame, and the f/2.8
    /// aperture is open enough that the circle of confusion runs from ~0 at
    /// the focused band to ~2.6 px at the target edges: the M4 heatmap then
    /// shows a genuine sharp **band** (a depth iso-line) with a visible blur
    /// falloff, rather than a uniformly in-focus quad. Tilting the Scheimpflug
    /// τ rotates that band. An 8 mm f/4 lens — wide enough to frame a larger
    /// target — would instead give a depth of field so deep the whole target
    /// reads as sharp, defeating the heatmap as a visual; 16 mm f/2.8 is the
    /// coherent compromise that both frames the target and shows defocus.
    ///
    /// # Panics
    ///
    /// Does not panic: every entity here is constructed from hard-coded,
    /// validated parameters, so the inner `Result`s are always `Ok`.
    #[must_use]
    pub fn default_mvp() -> Self {
        // The point every emitter/sensor is aimed at: the target centre.
        let target_center = Point3::new(0.0, 0.0, 0.30);

        // --- Target -----------------------------------------------------
        // `TargetEntity` is a rectangle in its local z = 0 plane with the
        // normal along local +z. To stand it upright facing world -Y (back
        // toward the sensor), rotate local +z onto world +Y.
        let target_pose = Isometry3::from_parts(
            Translation3::from(target_center.coords),
            rotation_local_z_to(Vector3::y()),
        );
        // 0.15 m × 0.12 m: inside the camera's ~0.17 m × 0.14 m framed area
        // at the 0.617 m standoff, with a margin (see the FOV / defocus
        // coherence note on this function).
        let target = TargetEntity::new(target_pose, 0.15, 0.12)
            .expect("hard-coded default target parameters are valid");

        // --- Laser ------------------------------------------------------
        // Directly in front of the target, aimed at its centre along +Y.
        let laser_origin = Point3::new(0.0, -0.60, 0.30);
        let laser_pose = look_at_pose(laser_origin, target_center, Vector3::z());
        // ~17 deg half-angle fan, reaching a touch past the target standoff.
        // 660 nm red diode with a 0.25 mm beam-waist radius — a typical
        // machine-vision line laser. With a 660 nm wavelength that waist gives
        // a Rayleigh range of ~0.30 m, so the M5 Gaussian-beam width model
        // keeps the stripe a few-tenths-of-a-millimetre thick across the
        // 0.60 m standoff and visibly broadens it past that.
        let laser = LaserEntity::new(laser_pose, 0.30, 0.75, 660.0, 0.25e-3)
            .expect("hard-coded default laser parameters are valid");

        // --- Camera -----------------------------------------------------
        // Offset sideways from the laser so it triangulates the line.
        let camera_origin = Point3::new(0.28, -0.55, 0.30);
        let camera_pose = look_at_pose(camera_origin, target_center, Vector3::z());
        // The camera-to-target-centre distance: the camera sits at
        // (0.28, -0.55, 0.30), the target centre at (0, 0, 0.30), so the
        // standoff is sqrt(0.28^2 + 0.55^2) ≈ 0.6172 m. The lens is focused
        // there so the target's on-axis point lands on the plane of best
        // focus.
        let camera_target_distance_m = (0.28_f64.powi(2) + 0.55_f64.powi(2)).sqrt();
        // A plausible machine-vision sensor: 1280x1024 on a 3.45 um pixel
        // pitch, with a 16 mm f/2.8 lens. At the 0.617 m standoff a 16 mm
        // lens gives a ~15.7° × 12.6° field of view that frames the
        // 0.15 m × 0.12 m target with a margin, and the f/2.8 aperture makes
        // the circle of confusion run ~0..2.6 px across the obliquely-viewed
        // target — enough for the M4 heatmap to show a sharp band with a
        // visible falloff (see the coherence note on this function).
        //
        // The PHYSICAL optics parameters (focal length, f-number, focus
        // distance, principal gap, pixel pitch) are the source of truth; the
        // pixel-unit `fx`/`fy` in `intrinsics` are derived from them by
        // `CameraEntity::new` — so `fx = fy = 0.016 / 3.45e-6 ≈ 4637.7 px`.
        // The `fx`/`fy` written here are placeholders that `new` overwrites;
        // `cx`/`cy` are kept as the sensor centre.
        //
        // The sensor is Scheimpflug at zero tilt — geometrically identical to
        // Identity at tau_x = tau_y = 0, but the M3 UI exposes τ sliders that
        // need backing fields, and M4 reads the (here zero) tilt to place the
        // plane of best focus.
        let camera_focal_length_m = 0.016; // 16 mm lens
        let camera_f_number = 2.8; // f/2.8
        let camera_focus_distance_m = camera_target_distance_m; // focus on target
        let camera_principal_gap_m = 0.0; // treat as thin for the default
        let camera_pixel_pitch_m = 3.45e-6; // 3.45 um pixels
        let camera_params = CameraParams {
            projection: ProjectionParams::Pinhole,
            distortion: DistortionParams::None,
            sensor: SensorParams::Scheimpflug {
                params: ScheimpflugParams {
                    tilt_x: 0.0,
                    tilt_y: 0.0,
                },
            },
            intrinsics: IntrinsicsParams::FxFyCxCySkew {
                params: FxFyCxCySkew {
                    // Overwritten by `CameraEntity::new` from focal/pitch.
                    fx: 0.0,
                    fy: 0.0,
                    cx: 640.0,
                    cy: 512.0,
                    skew: 0.0,
                },
            },
        };
        // The drawn frustum brackets the target standoff (~0.617 m) so the
        // wireframe visibly encloses the target.
        let camera = CameraEntity::new(
            camera_pose,
            camera_params,
            (1280, 1024),
            camera_focal_length_m,
            camera_f_number,
            camera_focus_distance_m,
            camera_principal_gap_m,
            camera_pixel_pitch_m,
            0.30,
            0.95,
        )
        .expect("hard-coded default camera parameters are valid");

        Self {
            cameras: vec![camera],
            lasers: vec![laser],
            targets: vec![target],
        }
    }
}

/// The rotation that takes a frame's local `+z` axis onto the unit vector
/// `target_dir` (expressed in the parent frame).
///
/// Used to orient entities whose local model is built around `+z` — a camera's
/// optical axis, a laser's central fan ray, a target plane's normal. The
/// remaining roll about `target_dir` is left to `nalgebra`'s shortest-arc
/// choice, which is fine for the default scene (no entity needs a specific
/// roll). For a fully constrained orientation, use [`look_at_pose`].
///
/// `target_dir` is normalized internally; a zero vector falls back to the
/// identity rotation rather than producing `NaN`s.
fn rotation_local_z_to(target_dir: Vector3<f64>) -> UnitQuaternion<f64> {
    let from = Vector3::z();
    if target_dir.norm() < 1e-12 {
        return UnitQuaternion::identity();
    }
    UnitQuaternion::rotation_between(&from, &target_dir).unwrap_or_else(|| {
        // `rotation_between` returns `None` only for anti-parallel vectors
        // (here, `target_dir` ≈ -z): a 180° turn about any perpendicular axis.
        UnitQuaternion::from_axis_angle(&Vector3::x_axis(), std::f64::consts::PI)
    })
}

/// A pose that places a `+z`-looking entity at `eye`, with its local `+z` axis
/// pointing at `target` and its local `+y` axis kept as close as possible to
/// `-world_up` (image-`y`-down convention for cameras).
///
/// This is the fully-constrained counterpart to [`rotation_local_z_to`]: it
/// also fixes the roll. The result maps entity-local coordinates into the
/// parent (world) frame — `pose = world ← local`.
///
/// `world_up` must not be parallel to the eye→target direction; the default
/// scene picks `eye`/`target`/`world_up` so this holds.
fn look_at_pose(eye: Point3<f64>, target: Point3<f64>, world_up: Vector3<f64>) -> Isometry3<f64> {
    // `Rotation3::face_towards` builds a frame whose +z faces `target - eye`
    // and whose +y aligns with `world_up`. Cameras use image-y-down, so feed
    // `-world_up` to get +y pointing down in the image.
    let rotation = Rotation3::face_towards(&(target - eye), &(-world_up));
    Isometry3::from_parts(
        Translation3::from(eye.coords),
        UnitQuaternion::from_rotation_matrix(&rotation),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn empty_scene_has_no_entities() {
        let scene = Scene::empty();
        assert!(scene.cameras.is_empty());
        assert!(scene.lasers.is_empty());
        assert!(scene.targets.is_empty());
    }

    #[test]
    fn default_mvp_has_one_of_each_entity() {
        let scene = Scene::default_mvp();
        assert_eq!(scene.cameras.len(), 1);
        assert_eq!(scene.lasers.len(), 1);
        assert_eq!(scene.targets.len(), 1);
    }

    #[test]
    fn default_mvp_camera_looks_at_the_target() {
        let scene = Scene::default_mvp();
        let camera = &scene.cameras[0];
        let target = &scene.targets[0];
        // The target centre is the translation of the target pose.
        let target_center = target.pose * Point3::origin();
        // The camera's optical axis is its local +z mapped into the world.
        let optical_axis =
            (camera.pose * Point3::new(0.0, 0.0, 1.0)) - (camera.pose * Point3::origin());
        let to_target = (target_center - (camera.pose * Point3::origin())).normalize();
        // The optical axis must point at the target centre.
        assert_relative_eq!(
            optical_axis.normalize().dot(&to_target),
            1.0,
            epsilon = 1e-9
        );
    }

    #[test]
    fn default_mvp_laser_looks_at_the_target() {
        let scene = Scene::default_mvp();
        let laser = &scene.lasers[0];
        let target_center = scene.targets[0].pose * Point3::origin();
        let central_ray =
            (laser.pose * Point3::new(0.0, 0.0, 1.0)) - (laser.pose * Point3::origin());
        let to_target = (target_center - (laser.pose * Point3::origin())).normalize();
        assert_relative_eq!(central_ray.normalize().dot(&to_target), 1.0, epsilon = 1e-9);
    }

    #[test]
    fn default_mvp_has_a_triangulation_baseline() {
        // The whole point of the arrangement: the camera and laser are not
        // co-located, so they view/illuminate the target from different
        // directions. A zero baseline would mean no triangulation.
        let scene = Scene::default_mvp();
        let cam_origin = scene.cameras[0].pose * Point3::origin();
        let laser_origin = scene.lasers[0].pose * Point3::origin();
        let baseline = (cam_origin - laser_origin).norm();
        assert!(
            baseline > 0.1,
            "expected a meaningful triangulation baseline, got {baseline} m"
        );
    }

    #[test]
    fn default_mvp_triangulation_angle_is_reasonable() {
        // The angle subtended at the target centre between the laser and the
        // camera should land in a sane triangulation range (~10°–45°).
        let scene = Scene::default_mvp();
        let target_center = scene.targets[0].pose * Point3::origin();
        let cam_origin = scene.cameras[0].pose * Point3::origin();
        let laser_origin = scene.lasers[0].pose * Point3::origin();
        let to_cam = (cam_origin - target_center).normalize();
        let to_laser = (laser_origin - target_center).normalize();
        let angle = to_cam.dot(&to_laser).clamp(-1.0, 1.0).acos();
        let deg = angle.to_degrees();
        assert!(
            (10.0..=45.0).contains(&deg),
            "triangulation angle {deg}° outside the expected 10°–45° range"
        );
    }

    #[test]
    fn default_mvp_camera_field_of_view_covers_the_target() {
        // FOV coherence (M4): the camera's lens must be wide enough that every
        // point of the default target is imageable. `defocus_map` records a
        // sample as `None` exactly when it cannot be imaged (behind the camera
        // or outside the projection chain), so a map with no `None` cell over
        // a dense grid is a direct check that the target is fully framed.
        let scene = Scene::default_mvp();
        let map = crate::analysis::defocus_map(&scene.cameras[0], &scene.targets[0], 33, 27)
            .expect("defocus map for the default scene");
        assert!(
            map.coc_px().iter().all(Option::is_some),
            "every default-target sample must be imageable — the default lens \
             field of view must cover the whole target"
        );

        let values: Vec<f64> = map.coc_px().iter().filter_map(|&c| c).collect();
        let min = values.iter().copied().fold(f64::INFINITY, f64::min);
        let max = values.iter().copied().fold(0.0_f64, f64::max);

        // The on-axis target point sits on the plane of best focus (the lens
        // is focused at the camera-to-target distance), so the sharpest cell
        // is essentially zero blur.
        assert!(
            min < 1.0,
            "the focused band of the default target should be near-sharp, \
             got a minimum CoC of {min} px"
        );
        // Defocus coherence (the M4 visual checkpoint): the default scene
        // must also leave part of the target *out* of the depth of field, so
        // the heatmap shows a sharp band with a falloff rather than a
        // uniformly in-focus quad. The chosen 16 mm f/2.8 lens gives a peak
        // CoC of a few pixels at the target edges.
        assert!(
            max > 1.5,
            "the default scene should put the target edges out of focus so \
             the heatmap shows a band; peak CoC was only {max} px"
        );
    }

    #[test]
    fn rotation_local_z_to_maps_z_onto_the_target_direction() {
        let dir = Vector3::new(0.0, 1.0, 0.0);
        let q = rotation_local_z_to(dir);
        let mapped = q * Vector3::z();
        assert_relative_eq!(mapped, dir, epsilon = 1e-12);
    }

    #[test]
    fn rotation_local_z_to_handles_the_antiparallel_case() {
        // -z is the degenerate input for `rotation_between`; the fallback must
        // still produce a rotation that lands on -z.
        let dir = Vector3::new(0.0, 0.0, -1.0);
        let q = rotation_local_z_to(dir);
        let mapped = q * Vector3::z();
        assert_relative_eq!(mapped, dir, epsilon = 1e-12);
    }

    #[test]
    fn rotation_local_z_to_falls_back_to_identity_on_zero() {
        let q = rotation_local_z_to(Vector3::zeros());
        assert_relative_eq!(
            q.into_inner(),
            UnitQuaternion::identity().into_inner(),
            epsilon = 1e-12
        );
    }

    #[test]
    fn look_at_pose_aims_local_z_at_the_target() {
        let eye = Point3::new(1.0, -2.0, 0.5);
        let target = Point3::new(0.0, 0.0, 0.5);
        let pose = look_at_pose(eye, target, Vector3::z());
        // The local origin lands at `eye`.
        let world_eye = pose * Point3::origin();
        assert_relative_eq!(world_eye, eye, epsilon = 1e-12);
        // Local +z points from `eye` toward `target`.
        let axis = (pose * Point3::new(0.0, 0.0, 1.0)) - world_eye;
        let expected = (target - eye).normalize();
        assert_relative_eq!(axis.normalize(), expected, epsilon = 1e-9);
    }

    #[test]
    fn look_at_pose_keeps_local_y_pointing_down() {
        // Image-y-down: with world_up = +z, the entity's local +y should have
        // a negative world-z component (it points "down").
        let eye = Point3::new(0.0, -1.0, 0.5);
        let target = Point3::new(0.0, 0.0, 0.5);
        let pose = look_at_pose(eye, target, Vector3::z());
        let local_y = (pose * Point3::new(0.0, 1.0, 0.0)) - (pose * Point3::origin());
        assert!(
            local_y.z < 0.0,
            "local +y should point down (negative world z), got z = {}",
            local_y.z
        );
    }
}
