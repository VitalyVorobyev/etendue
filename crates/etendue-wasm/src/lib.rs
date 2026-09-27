//! WebAssembly facade for the etendue kernel (npm `@etendue/wasm`).
//!
//! A [`Session`] is one loaded scene: the [`SceneSpec`], one
//! [`RobotModel`] per scene robot, and one calibration-rs camera model per
//! camera. It is created once from JSON documents and then answers
//! [`Session::bake`] and [`Session::project_points`]. The JS class
//! [`EtendueScene`] wraps it; the npm package adds a typed layer on top that
//! passes documents as objects (`scripts/build-npm.mjs`).
//!
//! The crate only marshals. Kinematics is `etendue-kinematics`'s, projection
//! is `vision-calibration-core`'s, and validation is `etendue-scene`'s — the
//! same code `etendue-cli` runs. Documents cross the boundary as JSON text,
//! parsed with `serde_json`'s `float_roundtrip`, so every `f64` arrives
//! bit-exact (gate G0.1).
//!
//! Scene files name robot manifests and URDFs by relative path. The host
//! resolves and reads them and passes the contents in as [`RobotSource`]s;
//! this crate does no I/O.

use std::fmt;

use etendue_kinematics::{RobotModel, bake};
use etendue_scene::{
    BakedScenario, FrameGraph, Issue, RobotManifest, ScenarioSpec, SceneSpec, ValidationError,
};
use nalgebra::{Isometry3, Point2, Point3, Quaternion, Translation3, UnitQuaternion};
use serde::Deserialize;
use vision_calibration_core::CameraModel;
use wasm_bindgen::prelude::*;

/// The contents of one robot's asset files, for the scene robot `id`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RobotSource {
    /// The scene robot this model is for (`SceneSpec::robots[].id`).
    pub id: String,
    /// The parsed `robot.json` manifest.
    pub manifest: RobotManifest,
    /// The URDF text the manifest points to.
    pub urdf: String,
}

/// Why a call failed.
#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    /// A document is not valid JSON or does not match its schema.
    Parse {
        /// Which document (`"scene"`, `"robots"`, `"scenario"`).
        what: &'static str,
        /// The parser's message.
        message: String,
    },
    /// A document parsed but failed validation.
    Invalid {
        /// Which document.
        what: String,
        /// Every problem found.
        issues: Vec<Issue>,
    },
    /// A robot model, IK, or scenario step failed.
    Kinematics(String),
    /// Bad call arguments (unknown camera, ragged point array, …).
    Input(String),
}

impl Error {
    /// The discriminant, as exposed to JS (`error.kind`).
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Parse { .. } => "parse",
            Self::Invalid { .. } => "invalid",
            Self::Kinematics(_) => "kinematics",
            Self::Input(_) => "input",
        }
    }

    fn invalid(what: impl Into<String>, e: ValidationError) -> Self {
        Self::Invalid {
            what: what.into(),
            issues: e.issues,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse { what, message } => write!(f, "{what}: {message}"),
            Self::Invalid { what, issues } => {
                write!(f, "{what}: {} problem(s)", issues.len())?;
                for issue in issues {
                    write!(f, "\n  {}: {}", issue.path, issue.message)?;
                }
                Ok(())
            }
            Self::Kinematics(message) | Self::Input(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for Error {}

impl From<etendue_kinematics::Error> for Error {
    fn from(e: etendue_kinematics::Error) -> Self {
        match e {
            etendue_kinematics::Error::Validation(v) => Self::invalid("scenario", v),
            other => Self::Kinematics(other.to_string()),
        }
    }
}

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;

fn parse<T: serde::de::DeserializeOwned>(what: &'static str, json: &str) -> Result<T> {
    serde_json::from_str(json).map_err(|e| Error::Parse {
        what,
        message: e.to_string(),
    })
}

/// A loaded, validated scene.
pub struct Session {
    scene: SceneSpec,
    robots: Vec<RobotModel>,
    cameras: Vec<CameraModel>,
}

impl Session {
    /// Parse and validate a scene and its robot models, as
    /// `etendue validate <scene.json>` does.
    ///
    /// `robots_json` is a JSON array of [`RobotSource`], one per scene robot
    /// in any order.
    ///
    /// # Errors
    ///
    /// [`Error::Parse`] for malformed documents; [`Error::Invalid`] if the
    /// scene, a manifest, or the frame tree (unknown robot links) fails
    /// validation; [`Error::Kinematics`] if a URDF does not load or an
    /// `initial_q` is out of limits; [`Error::Input`] if the robot sources do
    /// not match the scene robots one to one.
    pub fn load(scene_json: &str, robots_json: &str) -> Result<Self> {
        let scene: SceneSpec = parse("scene", scene_json)?;
        scene.validate().map_err(|e| Error::invalid("scene", e))?;
        let mut sources: Vec<RobotSource> = parse("robots", robots_json)?;

        let mut robots = Vec::with_capacity(scene.robots.len());
        for spec in &scene.robots {
            let at = sources
                .iter()
                .position(|s| s.id == spec.id)
                .ok_or_else(|| Error::Input(format!("no robot source for robot `{}`", spec.id)))?;
            let source = sources.swap_remove(at);
            source
                .manifest
                .validate()
                .map_err(|e| Error::invalid(format!("robot `{}` manifest", spec.id), e))?;
            let model = RobotModel::from_urdf_str(&source.urdf, &source.manifest)
                .map_err(|e| Error::Kinematics(format!("robot `{}`: {e}", spec.id)))?;
            if let Some(q) = &spec.initial_q {
                model.check_q(q, 0.0).map_err(|e| {
                    Error::Kinematics(format!("robot `{}` initial_q: {e}", spec.id))
                })?;
            }
            robots.push(model);
        }
        if let Some(extra) = sources.first() {
            return Err(Error::Input(format!(
                "robot source `{}` matches no scene robot",
                extra.id
            )));
        }
        let links: Vec<Vec<String>> = robots.iter().map(|m| m.links().to_vec()).collect();
        FrameGraph::build(&scene, &links).map_err(|e| Error::invalid("scene", e))?;

        // `scene.validate()` has already built every camera model once.
        let cameras = scene
            .cameras
            .iter()
            .map(|c| c.params.build())
            .collect::<std::result::Result<_, _>>()
            .map_err(|e| Error::Input(format!("camera model does not build: {e}")))?;
        Ok(Self {
            scene,
            robots,
            cameras,
        })
    }

    /// The loaded scene.
    #[must_use]
    pub fn scene(&self) -> &SceneSpec {
        &self.scene
    }

    /// Compile and bake a scenario (`etendue bake`). A scenario with no
    /// steps bakes to a single sample at the robots' `initial_q`.
    ///
    /// # Errors
    ///
    /// [`Error::Parse`] for a malformed scenario; [`Error::Invalid`] if it
    /// fails validation against the scene; [`Error::Kinematics`] if a step
    /// cannot be executed.
    pub fn bake(&self, scenario_json: &str) -> Result<BakedScenario> {
        let scenario: ScenarioSpec = parse("scenario", scenario_json)?;
        Ok(bake(&self.scene, &scenario, &self.robots)?)
    }

    /// Project world-frame points through camera `camera_id` placed at
    /// `world_se3_camera` — typically that camera's frame in a baked sample.
    ///
    /// `xyz_world` is a flat `[x0, y0, z0, x1, …]` array in metres. Each point
    /// is mapped into the camera frame with `world_se3_camera⁻¹` and projected
    /// by the `vision-calibration-core` model built from the camera's
    /// `CameraParams`. Returns a flat `[u0, v0, u1, …]` pixel array; a point
    /// the camera cannot image (on or behind the camera plane, or rejected by
    /// the projection chain) yields `NaN, NaN`. Points outside the image
    /// bounds are returned as projected, not clipped.
    ///
    /// # Errors
    ///
    /// [`Error::Input`] if `camera_id` names no camera or `xyz_world.len()`
    /// is not a multiple of 3.
    pub fn project_points(
        &self,
        camera_id: &str,
        world_se3_camera: &Isometry3<f64>,
        xyz_world: &[f64],
    ) -> Result<Vec<f64>> {
        let index = self.camera_index(camera_id)?;
        if !xyz_world.len().is_multiple_of(3) {
            return Err(Error::Input(format!(
                "point array length must be a multiple of 3, got {}",
                xyz_world.len()
            )));
        }
        let model = &self.cameras[index];
        let camera_se3_world = world_se3_camera.inverse();
        let mut uv = Vec::with_capacity(xyz_world.len() / 3 * 2);
        for p in xyz_world.chunks_exact(3) {
            let p_cam = camera_se3_world * Point3::new(p[0], p[1], p[2]);
            match model.project_point_c(&p_cam.coords) {
                Some(px) => uv.extend_from_slice(&[px.x, px.y]),
                None => uv.extend_from_slice(&[f64::NAN, f64::NAN]),
            }
        }
        Ok(uv)
    }

    /// Back-project pixels of camera `camera_id` to viewing rays, returned
    /// as points on the camera-frame `z = 1` plane (flat `[x0, y0, 1, …]`).
    ///
    /// This is the inverse of [`Session::project_points`] up to depth —
    /// `vision-calibration-core`'s `backproject_pixel` (inverse intrinsics,
    /// sensor, iterative undistortion). Viewers use it to draw a camera's
    /// true field of view (the image border back-projected, distortion
    /// included) without any camera math of their own.
    ///
    /// # Errors
    ///
    /// [`Error::Input`] if `camera_id` names no camera or `uv.len()` is odd.
    pub fn backproject_pixels(&self, camera_id: &str, uv: &[f64]) -> Result<Vec<f64>> {
        let model = &self.cameras[self.camera_index(camera_id)?];
        if !uv.len().is_multiple_of(2) {
            return Err(Error::Input(format!(
                "pixel array length must be even, got {}",
                uv.len()
            )));
        }
        let mut xyz = Vec::with_capacity(uv.len() / 2 * 3);
        for px in uv.chunks_exact(2) {
            let ray = model.backproject_pixel(&Point2::new(px[0], px[1]));
            xyz.extend_from_slice(&[ray.point.x, ray.point.y, ray.point.z]);
        }
        Ok(xyz)
    }

    /// Extent `[x, y]` in metres of target `target_id` (etendue-scene
    /// `TargetGeometry::extent_m`), or `None` when the geometry does not
    /// determine it (a puzzleboard).
    ///
    /// # Errors
    ///
    /// [`Error::Input`] if `target_id` names no target.
    pub fn target_extent(&self, target_id: &str) -> Result<Option<[f64; 2]>> {
        self.scene
            .targets
            .iter()
            .find(|t| t.id == target_id)
            .map(|t| t.geometry.extent_m())
            .ok_or_else(|| Error::Input(format!("no target `{target_id}`")))
    }

    fn camera_index(&self, camera_id: &str) -> Result<usize> {
        self.scene
            .cameras
            .iter()
            .position(|c| c.id == camera_id)
            .ok_or_else(|| Error::Input(format!("no camera `{camera_id}`")))
    }
}

/// An SE(3) from its flat wire order `[qx, qy, qz, qw, tx, ty, tz]` — the
/// `{rotation, translation}` JSON form flattened. The quaternion is taken as
/// given (as nalgebra's serde does); scene validation guarantees unit
/// quaternions for poses that come out of a bake.
///
/// # Errors
///
/// [`Error::Input`] if `wire.len() != 7`.
pub fn iso3_from_wire(wire: &[f64]) -> Result<Isometry3<f64>> {
    let &[qx, qy, qz, qw, tx, ty, tz] = wire else {
        return Err(Error::Input(format!(
            "an SE(3) is 7 numbers [qx, qy, qz, qw, tx, ty, tz], got {}",
            wire.len()
        )));
    };
    Ok(Isometry3::from_parts(
        Translation3::new(tx, ty, tz),
        UnitQuaternion::new_unchecked(Quaternion::new(qw, qx, qy, qz)),
    ))
}

// ---------------------------------------------------------------------------
// JS bindings
// ---------------------------------------------------------------------------

/// Turn an [`Error`] into a JS `Error` carrying `kind` and, for
/// [`Error::Invalid`], `issues: {path, message}[]`.
fn js_error(e: &Error) -> JsValue {
    let err = js_sys::Error::new(&e.to_string());
    let obj: &JsValue = err.as_ref();
    // Setting a property on a fresh `Error` object cannot fail.
    let _ = js_sys::Reflect::set(obj, &"kind".into(), &e.kind().into());
    if let Error::Invalid { what, issues } = e {
        let list = js_sys::Array::new();
        for issue in issues {
            let item = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&item, &"path".into(), &issue.path.as_str().into());
            let _ = js_sys::Reflect::set(&item, &"message".into(), &issue.message.as_str().into());
            list.push(&item);
        }
        let _ = js_sys::Reflect::set(obj, &"document".into(), &what.as_str().into());
        let _ = js_sys::Reflect::set(obj, &"issues".into(), &list);
    }
    err.into()
}

/// A loaded scene (JS side of [`Session`]). Documents go in and come out as
/// JSON text; the npm package's typed wrapper converts them to objects.
#[wasm_bindgen]
pub struct EtendueScene {
    inner: Session,
}

#[wasm_bindgen]
impl EtendueScene {
    /// Load and validate a scene. See [`Session::load`].
    ///
    /// # Errors
    ///
    /// Throws an `Error` with `kind` (and `issues` for validation errors).
    #[wasm_bindgen(constructor)]
    pub fn new(scene_json: &str, robots_json: &str) -> std::result::Result<Self, JsValue> {
        Session::load(scene_json, robots_json)
            .map(|inner| Self { inner })
            .map_err(|e| js_error(&e))
    }

    /// Bake a scenario; returns the `BakedScenario` as JSON text. See
    /// [`Session::bake`].
    ///
    /// # Errors
    ///
    /// Throws an `Error` with `kind` (and `issues` for validation errors).
    pub fn bake(&self, scenario_json: &str) -> std::result::Result<String, JsValue> {
        let baked = self.inner.bake(scenario_json).map_err(|e| js_error(&e))?;
        serde_json::to_string(&baked).map_err(|e| js_error(&Error::Input(e.to_string())))
    }

    /// Project world points through a camera. `world_se3_camera` is
    /// `[qx, qy, qz, qw, tx, ty, tz]`. See [`Session::project_points`].
    ///
    /// # Errors
    ///
    /// Throws an `Error` with `kind: "input"` for bad arguments.
    #[wasm_bindgen(js_name = projectPoints)]
    pub fn project_points(
        &self,
        camera_id: &str,
        world_se3_camera: &[f64],
        xyz_world: &[f64],
    ) -> std::result::Result<Vec<f64>, JsValue> {
        let pose = iso3_from_wire(world_se3_camera).map_err(|e| js_error(&e))?;
        self.inner
            .project_points(camera_id, &pose, xyz_world)
            .map_err(|e| js_error(&e))
    }

    /// Back-project pixels to `z = 1` camera-frame points. See
    /// [`Session::backproject_pixels`].
    ///
    /// # Errors
    ///
    /// Throws an `Error` with `kind: "input"` for bad arguments.
    #[wasm_bindgen(js_name = backprojectPixels)]
    pub fn backproject_pixels(
        &self,
        camera_id: &str,
        uv: &[f64],
    ) -> std::result::Result<Vec<f64>, JsValue> {
        self.inner
            .backproject_pixels(camera_id, uv)
            .map_err(|e| js_error(&e))
    }

    /// Target extent `[x, y]` in metres, or `undefined`. See
    /// [`Session::target_extent`].
    ///
    /// # Errors
    ///
    /// Throws an `Error` with `kind: "input"` for an unknown target.
    #[wasm_bindgen(js_name = targetExtent)]
    pub fn target_extent(&self, target_id: &str) -> std::result::Result<Option<Vec<f64>>, JsValue> {
        self.inner
            .target_extent(target_id)
            .map(|e| e.map(Vec::from))
            .map_err(|e| js_error(&e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCENE: &str = include_str!("../../../examples/eye_in_hand_ur5e/scene.json");
    const SCENARIO: &str = include_str!("../../../examples/eye_in_hand_ur5e/scenario.json");
    const MANIFEST: &str = include_str!("../../../assets/robots/ur5e/robot.json");
    const URDF: &str = include_str!("../../../assets/robots/ur5e/robot.urdf");

    fn robots_json(id: &str) -> String {
        format!(
            r#"[{{"id": {}, "manifest": {MANIFEST}, "urdf": {}}}]"#,
            serde_json::to_string(id).unwrap(),
            serde_json::to_string(URDF).unwrap()
        )
    }

    fn session() -> Session {
        Session::load(SCENE, &robots_json("ur5e")).expect("example scene loads")
    }

    #[test]
    fn bake_matches_the_kinematics_crate() {
        let s = session();
        let baked = s.bake(SCENARIO).unwrap();
        let scenario: ScenarioSpec = serde_json::from_str(SCENARIO).unwrap();
        let direct = bake(s.scene(), &scenario, &s.robots).unwrap();
        assert_eq!(
            serde_json::to_string(&baked).unwrap(),
            serde_json::to_string(&direct).unwrap()
        );
        assert!(baked.capture_indices().count() > 0);
    }

    #[test]
    fn empty_scenario_bakes_the_rest_pose() {
        let baked = session()
            .bake(r#"{"version": 1, "dt": 0.01, "steps": []}"#)
            .unwrap();
        assert_eq!(baked.samples.len(), 1);
        assert_eq!(baked.frames[0], "world");
    }

    #[test]
    fn projects_through_the_baked_camera_pose() {
        let s = session();
        let baked = s.bake(SCENARIO).unwrap();
        let k = baked.capture_indices().next().unwrap();
        let cam = baked.frames.iter().position(|f| f == "cam_left").unwrap();
        let world_se3_camera = baked.samples[k].world_se3_frame[cam];
        // A point 0.4 m straight ahead lands on the principal point.
        let ahead = world_se3_camera * Point3::new(0.0, 0.0, 0.4);
        let uv = s
            .project_points("cam_left", &world_se3_camera, &[ahead.x, ahead.y, ahead.z])
            .unwrap();
        let cx = 640.0;
        let cy = 512.0;
        assert!(
            (uv[0] - cx).abs() < 1e-6 && (uv[1] - cy).abs() < 1e-6,
            "{uv:?}"
        );
        // The camera's own centre cannot be imaged.
        let o = world_se3_camera.translation.vector;
        let uv = s
            .project_points("cam_left", &world_se3_camera, &[o.x, o.y, o.z])
            .unwrap();
        assert!(uv[0].is_nan() && uv[1].is_nan());
    }

    #[test]
    fn backprojection_inverts_projection() {
        let s = session();
        let uv = [0.0, 0.0, 1280.0, 1024.0, 640.0, 512.0, 100.5, 900.25];
        let rays = s.backproject_pixels("cam_left", &uv).unwrap();
        assert_eq!(rays.len(), 12);
        let back = s
            .project_points("cam_left", &Isometry3::identity(), &rays)
            .unwrap();
        for (a, b) in uv.iter().zip(&back) {
            assert!((a - b).abs() < 1e-6, "{uv:?} vs {back:?}");
        }
        assert!(matches!(
            s.backproject_pixels("cam_left", &[1.0]),
            Err(Error::Input(_))
        ));
    }

    #[test]
    fn target_extent_comes_from_the_scene() {
        let s = session();
        let id = &s.scene().targets[0].id;
        // The example's 9×6 chessboard with 25 mm squares: 10 × 7 squares.
        assert_eq!(
            s.target_extent(id).unwrap(),
            Some([10.0 * 0.025, 7.0 * 0.025])
        );
        assert!(matches!(s.target_extent("nope"), Err(Error::Input(_))));
    }

    #[test]
    fn wire_pose_round_trips_through_serde_order() {
        let iso: Isometry3<f64> = serde_json::from_str(
            r#"{"rotation": [0.1, -0.2, 0.3, 0.9273618495495703], "translation": [1.0, 2.0, 3.0]}"#,
        )
        .unwrap();
        let wire = iso3_from_wire(&[0.1, -0.2, 0.3, 0.9273618495495703, 1.0, 2.0, 3.0]).unwrap();
        assert_eq!(iso, wire);
        assert!(matches!(iso3_from_wire(&[0.0; 6]), Err(Error::Input(_))));
    }

    #[test]
    fn reports_structured_errors() {
        assert!(matches!(
            Session::load("{", "[]"),
            Err(Error::Parse { what: "scene", .. })
        ));
        assert!(matches!(
            Session::load(SCENE, "[]"),
            Err(Error::Input(m)) if m.contains("ur5e")
        ));
        assert!(matches!(
            Session::load(SCENE, &robots_json("other")),
            Err(Error::Input(_))
        ));
        let bad_version = SCENE.replacen("\"version\": 1", "\"version\": 2", 1);
        let Err(Error::Invalid { what, issues }) =
            Session::load(&bad_version, &robots_json("ur5e"))
        else {
            panic!("expected a validation error");
        };
        assert_eq!(what, "scene");
        assert_eq!(issues[0].path, "version");
        let s = session();
        assert!(matches!(
            s.project_points("nope", &Isometry3::identity(), &[0.0, 0.0, 1.0]),
            Err(Error::Input(_))
        ));
        assert!(matches!(
            s.project_points("cam_left", &Isometry3::identity(), &[0.0, 0.0]),
            Err(Error::Input(_))
        ));
    }
}
