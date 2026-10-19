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

use std::collections::BTreeMap;
use std::fmt;

use etendue_core::from_scene::{physical_camera, resolve};
use etendue_core::optics::CardinalPoints;

use etendue_kinematics::{
    BakeOptions, ReportOptions, RobotModel, bake_trajectory, compile, report_trajectory,
};
use etendue_scene::{
    BakedScenario, CameraSpec, FrameGraph, GeneratorSpec, Issue, LaserSpec, MotionReport,
    RigLayout, RobotManifest, ScenarioSpec, SceneSpec, ValidationError, export,
};
use etendue_synth::gt::{BoardPoint, VisibilitySpec, project_points};
use etendue_synth::laser::{StripeSurface, stripe};
use etendue_synth::poses::{Generated, generate};
use etendue_synth::target_mesh::{TargetMesh, geometry_mesh};
use etendue_synth::{CanonicalCamera, CanonicalSpec, PixelCentre, RemapLut, remap_lut};
use nalgebra::{Isometry3, Point2, Point3, Quaternion, Translation3, UnitQuaternion};
use serde::Deserialize;
use vision_calibration_core::{CameraModel, CameraParams};
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
///
/// The session keeps the scene in projection frames (`etendue-core::from_scene::resolve`): a
/// camera with optics sits at its entrance pupil with its equivalent pinhole, so every bake,
/// projection and image here is the physical camera's. [`Session::mech_se3_projection`] gives
/// where each such camera's projection frame is in its body's frame.
pub struct Session {
    scene: SceneSpec,
    mech_se3_projection: BTreeMap<String, Isometry3<f64>>,
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
        let resolved = resolve(&scene).map_err(|e| Error::invalid("scene", e))?;
        let (scene, mech_se3_projection) = (resolved.scene, resolved.mech_se3_projection);
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
            mech_se3_projection,
            robots,
            cameras,
        })
    }

    /// Replace the scene, keeping the robot models: the edit path of an
    /// interactive editor, which must not re-parse URDFs on every change.
    ///
    /// The new scene must name the same robots (by id) as the loaded one;
    /// their manifests and URDFs are not re-read, so a changed `manifest`
    /// path is not followed (load a new session for that). On error the
    /// session is unchanged.
    ///
    /// # Errors
    ///
    /// As [`Session::load`] for the scene; [`Error::Input`] if the robot ids
    /// differ from the loaded scene's.
    pub fn update_scene(&mut self, scene_json: &str) -> Result<()> {
        let scene: SceneSpec = parse("scene", scene_json)?;
        scene.validate().map_err(|e| Error::invalid("scene", e))?;
        let resolved = resolve(&scene).map_err(|e| Error::invalid("scene", e))?;
        let (scene, mech_se3_projection) = (resolved.scene, resolved.mech_se3_projection);
        let mut robots = Vec::with_capacity(scene.robots.len());
        for spec in &scene.robots {
            let at = self
                .scene
                .robots
                .iter()
                .position(|r| r.id == spec.id)
                .ok_or_else(|| {
                    Error::Input(format!(
                        "robot `{}` is not loaded; open the scene again to add a robot",
                        spec.id
                    ))
                })?;
            let model = &self.robots[at];
            if let Some(q) = &spec.initial_q {
                model.check_q(q, 0.0).map_err(|e| {
                    Error::Kinematics(format!("robot `{}` initial_q: {e}", spec.id))
                })?;
            }
            robots.push(at);
        }
        let links: Vec<Vec<String>> = robots
            .iter()
            .map(|&at| self.robots[at].links().to_vec())
            .collect();
        FrameGraph::build(&scene, &links).map_err(|e| Error::invalid("scene", e))?;
        let cameras = scene
            .cameras
            .iter()
            .map(|c| c.params.build())
            .collect::<std::result::Result<_, _>>()
            .map_err(|e| Error::Input(format!("camera model does not build: {e}")))?;
        let mut old: Vec<Option<RobotModel>> = std::mem::take(&mut self.robots)
            .into_iter()
            .map(Some)
            .collect();
        self.robots = robots
            .iter()
            .map(|&at| old[at].take().expect("robot ids are unique"))
            .collect();
        self.scene = scene;
        self.mech_se3_projection = mech_se3_projection;
        self.cameras = cameras;
        Ok(())
    }

    /// Where camera `camera_id`'s projection frame (its entrance pupil) is in its body's frame,
    /// for a camera with optics; `None` for a plain pinhole, whose frame is its projection frame.
    ///
    /// # Errors
    ///
    /// [`Error::Input`] for an unknown camera.
    pub fn mech_se3_projection(&self, camera_id: &str) -> Result<Option<Isometry3<f64>>> {
        self.camera_index(camera_id)?;
        Ok(self.mech_se3_projection.get(camera_id).copied())
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
        self.bake_with(scenario_json, BakeOptions::default())
    }

    /// [`Session::bake`] with options (joint velocities and accelerations).
    ///
    /// # Errors
    ///
    /// As [`Session::bake`].
    pub fn bake_with(&self, scenario_json: &str, options: BakeOptions) -> Result<BakedScenario> {
        let scenario: ScenarioSpec = parse("scenario", scenario_json)?;
        let trajectory = compile(&self.scene, &scenario, &self.robots)?;
        Ok(bake_trajectory(
            &self.scene,
            &trajectory,
            &self.robots,
            options,
        )?)
    }

    /// The motion report of a scenario: per step, limit use, joint margins,
    /// singularity margin and warnings (`etendue export --report`).
    ///
    /// # Errors
    ///
    /// As [`Session::bake`].
    pub fn motion_report(&self, scenario_json: &str) -> Result<MotionReport> {
        let scenario: ScenarioSpec = parse("scenario", scenario_json)?;
        let trajectory = compile(&self.scene, &scenario, &self.robots)?;
        Ok(report_trajectory(
            &self.scene,
            &scenario,
            &trajectory,
            &self.robots,
            &ReportOptions::default(),
        ))
    }

    /// The scenario with every `ptp_pose`'s joint configuration stored, so
    /// that it can never bake onto another IK branch.
    ///
    /// # Errors
    ///
    /// As [`Session::bake`].
    pub fn resolve(&self, scenario_json: &str) -> Result<ScenarioSpec> {
        let scenario: ScenarioSpec = parse("scenario", scenario_json)?;
        Ok(etendue_kinematics::resolve(
            &self.scene,
            &scenario,
            &self.robots,
        )?)
    }

    /// Robot `robot_id`'s tool pose and joints at every capture, as CSV or
    /// JSON (`etendue export --captures`).
    ///
    /// # Errors
    ///
    /// As [`Session::bake`]; [`Error::Input`] for an unknown robot or
    /// format.
    pub fn export_captures(
        &self,
        scenario_json: &str,
        robot_id: &str,
        format: &str,
    ) -> Result<String> {
        let at = self.robot_index(robot_id)?;
        let baked = self.bake(scenario_json)?;
        let model = &self.robots[at];
        let links = model.links();
        let rows = export::captures(
            &baked,
            robot_id,
            &links[model.base_link()],
            &links[model.tcp_link()],
        )
        .map_err(|e| Error::Input(e.to_string()))?;
        match format {
            "csv" => Ok(rows.to_csv()),
            "json" => serde_json::to_string_pretty(&rows)
                .map(|t| t + "\n")
                .map_err(|e| Error::Input(e.to_string())),
            other => Err(Error::Input(format!(
                "captures export format must be \"csv\" or \"json\", got {other:?}"
            ))),
        }
    }

    /// Robot `robot_id`'s timed joint trajectory as CSV (`etendue export
    /// --trajectory`).
    ///
    /// # Errors
    ///
    /// As [`Session::bake`]; [`Error::Input`] for an unknown robot.
    pub fn export_trajectory(&self, scenario_json: &str, robot_id: &str) -> Result<String> {
        self.robot_index(robot_id)?;
        let baked = self.bake_with(scenario_json, BakeOptions { derivatives: true })?;
        export::trajectory_csv(&baked, robot_id).map_err(|e| Error::Input(e.to_string()))
    }

    /// Run a pose generator (a scenario's `generator`) on this scene: every
    /// candidate pose and the scenario of the kept ones (etendue-synth
    /// `poses::generate`).
    ///
    /// # Errors
    ///
    /// [`Error::Parse`] for a malformed spec; [`Error::Input`] if it does not
    /// fit the scene (unknown robot, frame or target; a sensor not on the
    /// tool; bad parameters).
    pub fn generate_poses(&self, spec_json: &str, dt: f64) -> Result<Generated> {
        let spec: GeneratorSpec = parse("generator", spec_json)?;
        let GeneratorSpec::Sphere(sphere) = spec;
        generate(&self.scene, &self.robots, &sphere, dt).map_err(|e| Error::Input(e.to_string()))
    }

    fn robot_index(&self, robot_id: &str) -> Result<usize> {
        self.scene
            .robots
            .iter()
            .position(|r| r.id == robot_id)
            .ok_or_else(|| Error::Input(format!("no robot `{robot_id}`")))
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

    /// The canonical render camera of camera `camera_id` and its remap LUT
    /// (etendue-synth, ADR 0004): render the canonical pinhole, then sample it
    /// at `lut[target pixel]` to get the target camera's image.
    ///
    /// # Errors
    ///
    /// [`Error::Input`] for an unknown camera or an invalid `spec`.
    pub fn remap(
        &self,
        camera_id: &str,
        spec: &CanonicalSpec,
        pixel_centre: PixelCentre,
    ) -> Result<(CanonicalCamera, RemapLut)> {
        let camera = &self.scene.cameras[self.camera_index(camera_id)?];
        let synth = |e: etendue_synth::Error| Error::Input(format!("camera `{camera_id}`: {e}"));
        let canonical =
            CanonicalCamera::cover(&camera.params, camera.resolution, spec, pixel_centre)
                .map_err(synth)?;
        let lut = remap_lut(&camera.params, camera.resolution, &canonical).map_err(synth)?;
        Ok((canonical, lut))
    }

    fn target(&self, target_id: &str) -> Result<&etendue_scene::TargetSpec> {
        self.scene
            .targets
            .iter()
            .find(|t| t.id == target_id)
            .ok_or_else(|| Error::Input(format!("no target `{target_id}`")))
    }

    /// The surface of target `target_id` as triangles in its frame: the
    /// pattern calib-targets prints, on a plane or the PuzzlePole's cylinder
    /// (etendue-synth `geometry_mesh`).
    ///
    /// # Errors
    ///
    /// [`Error::Input`] for an unknown target or one whose surface is
    /// undetermined.
    pub fn target_mesh(&self, target_id: &str) -> Result<TargetMesh> {
        geometry_mesh(&self.target(target_id)?.geometry)
            .map_err(|e| Error::Input(format!("target `{target_id}`: {e}")))
    }

    /// The feature points of target `target_id` (its inner corners, with
    /// calib-targets' labels), in its frame; none for a target calib-targets
    /// does not print.
    ///
    /// # Errors
    ///
    /// [`Error::Input`] for an unknown or invalid target.
    pub fn target_points(&self, target_id: &str) -> Result<Vec<BoardPoint>> {
        Ok(
            etendue_synth::board::layout(&self.target(target_id)?.geometry)
                .map_err(|e| Error::Input(format!("target `{target_id}`: {e}")))?
                .map(|l| l.points)
                .unwrap_or_default(),
        )
    }

    /// Target `target_id`'s feature points as camera `camera_id` sees them,
    /// with both placed in the world: flat `[u0, v0, …]`, `NaN, NaN` for a
    /// point the camera does not see (behind it, outside the image or on a
    /// side of the target facing away) — the ground truth's visibility tests
    /// (etendue-synth `project_points`, no margin).
    ///
    /// # Errors
    ///
    /// [`Error::Input`] for an unknown camera or target.
    pub fn project_target(
        &self,
        camera_id: &str,
        world_se3_camera: &Isometry3<f64>,
        target_id: &str,
        world_se3_target: &Isometry3<f64>,
    ) -> Result<Vec<f64>> {
        let index = self.camera_index(camera_id)?;
        let points = self.target_points(target_id)?;
        let projected = project_points(
            &self.cameras[index],
            self.scene.cameras[index].resolution,
            world_se3_camera,
            world_se3_target,
            &points,
            &VisibilitySpec { margin_px: 0.0 },
            -0.5,
        );
        Ok(projected
            .iter()
            .flat_map(|p| match (p.visible(), p.pixel) {
                (true, Some([u, v])) => [u, v],
                _ => [f64::NAN, f64::NAN],
            })
            .collect())
    }

    /// Laser `laser_id`'s stripe on target `target_id` as camera `camera_id`
    /// sees it, all three placed in the world: flat `[u0, v0, …]` along the
    /// stripe (1 mm apart on the target), `NaN, NaN` where the camera does not
    /// see it or the stripe breaks (a PuzzlePole's two arcs). Empty if the
    /// laser misses the target or the target has no printed surface.
    ///
    /// # Errors
    ///
    /// [`Error::Input`] for an unknown camera, laser or target, or a laser
    /// out of range.
    pub fn laser_stripe(
        &self,
        camera_id: &str,
        world_se3_camera: &Isometry3<f64>,
        laser_id: &str,
        world_se3_laser: &Isometry3<f64>,
        target_id: &str,
        world_se3_target: &Isometry3<f64>,
    ) -> Result<Vec<f64>> {
        const SPACING_M: f64 = 1e-3;
        let index = self.camera_index(camera_id)?;
        let laser = self
            .scene
            .lasers
            .iter()
            .find(|l| l.id == laser_id)
            .ok_or_else(|| Error::Input(format!("no laser `{laser_id}`")))?;
        let Some(surface) = StripeSurface::of(&self.target(target_id)?.geometry) else {
            return Ok(Vec::new());
        };
        let points = stripe(laser, world_se3_laser, world_se3_target, surface, SPACING_M)
            .map_err(|e| Error::Input(e.to_string()))?;
        let projected = project_points(
            &self.cameras[index],
            self.scene.cameras[index].resolution,
            world_se3_camera,
            world_se3_target,
            &points,
            &VisibilitySpec { margin_px: 0.0 },
            -0.5,
        );
        let mut out = Vec::with_capacity(2 * projected.len());
        for (i, p) in projected.iter().enumerate() {
            if i > 0 && (points[i].xyz() - points[i - 1].xyz()).norm() > 2.5 * SPACING_M {
                out.extend([f64::NAN, f64::NAN]);
            }
            match (p.visible(), p.pixel) {
                (true, Some([u, v])) => out.extend([u, v]),
                _ => out.extend([f64::NAN, f64::NAN]),
            }
        }
        Ok(out)
    }

    fn camera_index(&self, camera_id: &str) -> Result<usize> {
        self.scene
            .cameras
            .iter()
            .position(|c| c.id == camera_id)
            .ok_or_else(|| Error::Input(format!("no camera `{camera_id}`")))
    }
}

/// What a camera's optics give (ADR 0008), for an editor to show and store.
#[derive(Clone, Debug, serde::Serialize)]
pub struct DerivedOptics {
    /// The equivalent pinhole about the entrance pupil: what the camera's `params` must hold.
    pub params: CameraParams,
    /// The projection frame (entrance pupil) in the camera's body frame.
    pub mech_se3_projection: Isometry3<f64>,
    /// The lens's points on the body's axis (m from the image-plane datum).
    pub cardinal_points: CardinalPoints,
    /// H′ to the sensor at this focus (m).
    pub image_distance_m: f64,
    /// H to the plane in focus (m).
    pub object_distance_m: f64,
    /// Lateral magnification at this focus.
    pub magnification: f64,
    /// Exit over entrance pupil diameter.
    pub pupil_magnification: f64,
    /// Entrance pupil diameter (m).
    pub entrance_pupil_diameter_m: f64,
    /// Exit pupil diameter (m).
    pub exit_pupil_diameter_m: f64,
    /// `N · (1 + m / m_p)`.
    pub working_f_number: f64,
}

/// What `optics` give a camera built as `params` (its principal point and distortion are kept;
/// its focal lengths, skew and sensor tilt are derived).
///
/// # Errors
///
/// [`Error::Parse`] for malformed documents; [`Error::Input`] for optics that are not physical
/// (a focus the lens cannot reach, a pupil beyond the focal point).
pub fn derive_optics(optics_json: &str, params_json: &str) -> Result<DerivedOptics> {
    let optics: etendue_scene::OpticsSpec = parse("optics", optics_json)?;
    let params: CameraParams = parse("camera params", params_json)?;
    let camera = physical_camera(&optics, &params).map_err(|e| Error::Input(e.to_string()))?;
    Ok(DerivedOptics {
        params: camera.pinhole().clone(),
        mech_se3_projection: *camera.mech_se3_projection(),
        cardinal_points: *camera.cardinal_points(),
        image_distance_m: camera.image_distance_m(),
        object_distance_m: camera.object_distance_m(),
        magnification: camera.magnification(),
        pupil_magnification: camera.pupil_magnification(),
        entrance_pupil_diameter_m: camera.entrance_pupil_diameter_m(),
        exit_pupil_diameter_m: camera.exit_pupil_diameter_m(),
        working_f_number: camera.working_f_number(),
    })
}

/// A rig layout's cameras and lasers.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ExpandedRig {
    /// `<rig>_cam<i>`.
    pub cameras: Vec<CameraSpec>,
    /// `<rig>_laser<i>`, each linked to its camera.
    pub lasers: Vec<LaserSpec>,
}

/// The cameras and lasers of a rig layout for rig `rig`
/// (`etendue-scene` `RigLayout::expand`).
///
/// # Errors
///
/// [`Error::Parse`] for a malformed layout; [`Error::Invalid`] if it fails
/// validation.
pub fn expand_rig(layout_json: &str, rig: &str) -> Result<ExpandedRig> {
    let layout: RigLayout = parse("layout", layout_json)?;
    layout
        .validate()
        .map_err(|e| Error::invalid("rig layout", e))?;
    let (cameras, lasers) = layout.expand(rig);
    Ok(ExpandedRig { cameras, lasers })
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

/// The cameras and lasers of a rig layout, as JSON text `{cameras,
/// lasers}`. See [`expand_rig`].
///
/// # Errors
///
/// Throws an `Error` with `kind` (and `issues` for validation errors).
#[wasm_bindgen(js_name = expandRig)]
pub fn expand_rig_js(layout_json: &str, rig: &str) -> std::result::Result<String, JsValue> {
    let expanded = expand_rig(layout_json, rig).map_err(|e| js_error(&e))?;
    serde_json::to_string(&expanded).map_err(|e| js_error(&Error::Input(e.to_string())))
}

/// A library item (ADR 0008 §5) parsed and checked: its version first, then its fields, its single
/// part, and, for a camera design with optics, its params against what the optics give.
///
/// # Errors
///
/// [`Error::Invalid`] with every problem found.
pub fn validate_library_item(text: &str) -> Result<etendue_scene::LibraryItem> {
    let version: etendue_scene::Versioned = parse("library item", text)?;
    etendue_scene::LibraryItem::check_version(version.version)
        .map_err(|e| Error::invalid("library item", e))?;
    let item: etendue_scene::LibraryItem = parse("library item", text)?;
    let mut issues = item.validate().err().map(|e| e.issues).unwrap_or_default();
    if let Some(c) = &item.camera
        && let Some(o) = &c.optics
    {
        issues.extend(etendue_core::from_scene::check_derived(
            o, &c.params, "camera",
        ));
    }
    if issues.is_empty() {
        Ok(item)
    } else {
        Err(Error::invalid("library item", ValidationError { issues }))
    }
}

/// A library item, checked, as JSON text. See [`validate_library_item`].
///
/// # Errors
///
/// Throws an `Error` with `kind` (and `issues` for validation errors).
#[wasm_bindgen(js_name = validateLibraryItem)]
pub fn validate_library_item_js(text: &str) -> std::result::Result<String, JsValue> {
    let item = validate_library_item(text).map_err(|e| js_error(&e))?;
    serde_json::to_string(&item).map_err(|e| js_error(&Error::Input(e.to_string())))
}

/// What a camera's optics give, as JSON text. See [`derive_optics`].
///
/// # Errors
///
/// Throws an `Error` with `kind`.
#[wasm_bindgen(js_name = deriveOptics)]
pub fn derive_optics_js(
    optics_json: &str,
    params_json: &str,
) -> std::result::Result<String, JsValue> {
    let derived = derive_optics(optics_json, params_json).map_err(|e| js_error(&e))?;
    serde_json::to_string(&derived).map_err(|e| js_error(&Error::Input(e.to_string())))
}

/// The PuzzlePole circumferences calib-targets supports, with their seam
/// rows: `[{squares, start_row}]` as JSON text.
#[wasm_bindgen(js_name = poleCircumferences)]
#[must_use]
pub fn pole_circumferences() -> String {
    let periods: Vec<serde_json::Value> = etendue_synth::board::pole_periods()
        .into_iter()
        .map(|(squares, start_row)| serde_json::json!({"squares": squares, "start_row": start_row}))
        .collect();
    serde_json::Value::Array(periods).to_string()
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

    /// Replace the scene, keeping the robot models. See
    /// [`Session::update_scene`].
    ///
    /// # Errors
    ///
    /// Throws an `Error` with `kind` (and `issues` for validation errors);
    /// the session is unchanged.
    #[wasm_bindgen(js_name = updateScene)]
    pub fn update_scene(&mut self, scene_json: &str) -> std::result::Result<(), JsValue> {
        self.inner
            .update_scene(scene_json)
            .map_err(|e| js_error(&e))
    }

    /// Where camera `camera_id`'s projection frame (its entrance pupil) is in its body's frame,
    /// as `[qx, qy, qz, qw, tx, ty, tz]`; `undefined` for a camera without optics. The baked
    /// frame of a camera is its projection frame. See [`Session::mech_se3_projection`].
    ///
    /// # Errors
    ///
    /// Throws an `Error` with `kind` for an unknown camera.
    #[wasm_bindgen(js_name = mechSe3Projection)]
    pub fn mech_se3_projection(
        &self,
        camera_id: &str,
    ) -> std::result::Result<Option<Vec<f64>>, JsValue> {
        let pose = self
            .inner
            .mech_se3_projection(camera_id)
            .map_err(|e| js_error(&e))?;
        Ok(pose.map(|p| {
            let (q, t) = (p.rotation.coords, p.translation.vector);
            vec![q.x, q.y, q.z, q.w, t.x, t.y, t.z]
        }))
    }

    /// Bake a scenario; returns the `BakedScenario` as JSON text. See
    /// [`Session::bake`].
    ///
    /// # Errors
    ///
    /// Throws an `Error` with `kind` (and `issues` for validation errors).
    pub fn bake(
        &self,
        scenario_json: &str,
        derivatives: bool,
    ) -> std::result::Result<String, JsValue> {
        let baked = self
            .inner
            .bake_with(scenario_json, BakeOptions { derivatives })
            .map_err(|e| js_error(&e))?;
        serde_json::to_string(&baked).map_err(|e| js_error(&Error::Input(e.to_string())))
    }

    /// The motion report as JSON text. See [`Session::motion_report`].
    ///
    /// # Errors
    ///
    /// Throws an `Error` with `kind` (and `issues` for validation errors).
    #[wasm_bindgen(js_name = motionReport)]
    pub fn motion_report(&self, scenario_json: &str) -> std::result::Result<String, JsValue> {
        let report = self
            .inner
            .motion_report(scenario_json)
            .map_err(|e| js_error(&e))?;
        serde_json::to_string(&report).map_err(|e| js_error(&Error::Input(e.to_string())))
    }

    /// The scenario with every IK solution stored, as JSON text. See
    /// [`Session::resolve`].
    ///
    /// # Errors
    ///
    /// Throws an `Error` with `kind` (and `issues` for validation errors).
    #[wasm_bindgen(js_name = resolveScenario)]
    pub fn resolve_scenario(&self, scenario_json: &str) -> std::result::Result<String, JsValue> {
        let resolved = self
            .inner
            .resolve(scenario_json)
            .map_err(|e| js_error(&e))?;
        serde_json::to_string(&resolved).map_err(|e| js_error(&Error::Input(e.to_string())))
    }

    /// A robot's tool pose and joints at every capture (`"csv"` or
    /// `"json"`). See [`Session::export_captures`].
    ///
    /// # Errors
    ///
    /// Throws an `Error` with `kind`.
    #[wasm_bindgen(js_name = exportCaptures)]
    pub fn export_captures(
        &self,
        scenario_json: &str,
        robot_id: &str,
        format: &str,
    ) -> std::result::Result<String, JsValue> {
        self.inner
            .export_captures(scenario_json, robot_id, format)
            .map_err(|e| js_error(&e))
    }

    /// Generated poses as JSON text `{candidates, scenario}`. See
    /// [`Session::generate_poses`].
    ///
    /// # Errors
    ///
    /// Throws an `Error` with `kind`.
    #[wasm_bindgen(js_name = generatePoses)]
    pub fn generate_poses(&self, spec_json: &str, dt: f64) -> std::result::Result<String, JsValue> {
        let generated = self
            .inner
            .generate_poses(spec_json, dt)
            .map_err(|e| js_error(&e))?;
        serde_json::to_string(&generated).map_err(|e| js_error(&Error::Input(e.to_string())))
    }

    /// A robot's timed joint trajectory as CSV. See
    /// [`Session::export_trajectory`].
    ///
    /// # Errors
    ///
    /// Throws an `Error` with `kind`.
    #[wasm_bindgen(js_name = exportTrajectory)]
    pub fn export_trajectory(
        &self,
        scenario_json: &str,
        robot_id: &str,
    ) -> std::result::Result<String, JsValue> {
        self.inner
            .export_trajectory(scenario_json, robot_id)
            .map_err(|e| js_error(&e))
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

    /// Canonical camera and remap LUT of a camera. `spec_json` is an
    /// etendue-synth `CanonicalSpec` (`{supersample, margin, scan_step_px}`)
    /// and `pixel_centre` is `"integer"` or `"half"`. Returns
    /// `{canonical: string (CanonicalCamera JSON), lut: Float32Array}`. See
    /// [`Session::remap`].
    ///
    /// # Errors
    ///
    /// Throws an `Error` with `kind` `parse` or `input`.
    pub fn remap(
        &self,
        camera_id: &str,
        spec_json: &str,
        pixel_centre: &str,
    ) -> std::result::Result<JsValue, JsValue> {
        let spec: CanonicalSpec = parse("remap spec", spec_json).map_err(|e| js_error(&e))?;
        let centre: PixelCentre = serde_json::from_value(serde_json::Value::String(
            pixel_centre.into(),
        ))
        .map_err(|_| {
            js_error(&Error::Input(format!(
                "pixel centre must be \"integer\" or \"half\", got {pixel_centre:?}"
            )))
        })?;
        let (canonical, lut) = self
            .inner
            .remap(camera_id, &spec, centre)
            .map_err(|e| js_error(&e))?;
        let out = js_sys::Object::new();
        let canonical = serde_json::to_string(&canonical)
            .map_err(|e| js_error(&Error::Input(e.to_string())))?;
        let _ = js_sys::Reflect::set(&out, &"canonical".into(), &canonical.into());
        let _ = js_sys::Reflect::set(
            &out,
            &"lut".into(),
            &js_sys::Float32Array::from(lut.data.as_slice()).into(),
        );
        Ok(out.into())
    }

    /// A target's surface as triangles: `{positions: Float32Array (xyz),
    /// normals: Float32Array, indices: Uint32Array, dark: Uint8Array (per
    /// triangle)}`. See [`Session::target_mesh`].
    ///
    /// # Errors
    ///
    /// Throws an `Error` with `kind: "input"`.
    #[wasm_bindgen(js_name = targetMesh)]
    pub fn target_mesh(&self, target_id: &str) -> std::result::Result<JsValue, JsValue> {
        let mesh = self
            .inner
            .target_mesh(target_id)
            .map_err(|e| js_error(&e))?;
        let flat =
            |v: &[[f64; 3]]| -> Vec<f32> { v.iter().flat_map(|p| p.map(|c| c as f32)).collect() };
        let out = js_sys::Object::new();
        let set = |key: &str, value: JsValue| {
            let _ = js_sys::Reflect::set(&out, &key.into(), &value);
        };
        set(
            "positions",
            js_sys::Float32Array::from(flat(&mesh.positions).as_slice()).into(),
        );
        set(
            "normals",
            js_sys::Float32Array::from(flat(&mesh.normals).as_slice()).into(),
        );
        let indices: Vec<u32> = mesh.triangles.iter().flatten().copied().collect();
        set(
            "indices",
            js_sys::Uint32Array::from(indices.as_slice()).into(),
        );
        let dark: Vec<u8> = mesh.dark.iter().map(|d| u8::from(*d)).collect();
        set("dark", js_sys::Uint8Array::from(dark.as_slice()).into());
        Ok(out.into())
    }

    /// A target's feature points as JSON text (an array of etendue-synth
    /// `BoardPoint`). See [`Session::target_points`].
    ///
    /// # Errors
    ///
    /// Throws an `Error` with `kind: "input"`.
    #[wasm_bindgen(js_name = targetPoints)]
    pub fn target_points(&self, target_id: &str) -> std::result::Result<String, JsValue> {
        let points = self
            .inner
            .target_points(target_id)
            .map_err(|e| js_error(&e))?;
        serde_json::to_string(&points).map_err(|e| js_error(&Error::Input(e.to_string())))
    }

    /// A target's feature points seen by a camera (`NaN` where not seen).
    /// Poses are `[qx, qy, qz, qw, tx, ty, tz]`. See
    /// [`Session::project_target`].
    ///
    /// # Errors
    ///
    /// Throws an `Error` with `kind: "input"`.
    #[wasm_bindgen(js_name = projectTarget)]
    pub fn project_target(
        &self,
        camera_id: &str,
        world_se3_camera: &[f64],
        target_id: &str,
        world_se3_target: &[f64],
    ) -> std::result::Result<Vec<f64>, JsValue> {
        let camera = iso3_from_wire(world_se3_camera).map_err(|e| js_error(&e))?;
        let target = iso3_from_wire(world_se3_target).map_err(|e| js_error(&e))?;
        self.inner
            .project_target(camera_id, &camera, target_id, &target)
            .map_err(|e| js_error(&e))
    }

    /// A laser's stripe on a target seen by a camera (`NaN` where not seen
    /// or broken). Poses are `[qx, qy, qz, qw, tx, ty, tz]`. See
    /// [`Session::laser_stripe`].
    ///
    /// # Errors
    ///
    /// Throws an `Error` with `kind: "input"`.
    #[wasm_bindgen(js_name = laserStripe)]
    pub fn laser_stripe(
        &self,
        camera_id: &str,
        world_se3_camera: &[f64],
        laser_id: &str,
        world_se3_laser: &[f64],
        target_id: &str,
        world_se3_target: &[f64],
    ) -> std::result::Result<Vec<f64>, JsValue> {
        let camera = iso3_from_wire(world_se3_camera).map_err(|e| js_error(&e))?;
        let laser = iso3_from_wire(world_se3_laser).map_err(|e| js_error(&e))?;
        let target = iso3_from_wire(world_se3_target).map_err(|e| js_error(&e))?;
        self.inner
            .laser_stripe(camera_id, &camera, laser_id, &laser, target_id, &target)
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
        let direct = etendue_kinematics::bake(s.scene(), &scenario, &s.robots).unwrap();
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
    fn remap_covers_the_camera() {
        let s = session();
        let (canonical, lut) = s
            .remap("cam_left", &CanonicalSpec::default(), PixelCentre::Integer)
            .unwrap();
        assert_eq!((lut.width, lut.height), (1280, 1024));
        assert!(canonical.resolution[0] >= 1280 && canonical.resolution[1] >= 1024);
        assert!(lut.data.iter().all(|v| v.is_finite()));
        assert!(matches!(
            s.remap("nope", &CanonicalSpec::default(), PixelCentre::Integer),
            Err(Error::Input(_))
        ));
    }

    #[test]
    fn update_scene_keeps_robots_and_rejects_bad_edits() {
        let mut s = session();
        let mut scene: serde_json::Value = serde_json::from_str(SCENE).unwrap();
        scene["cameras"][0]["resolution"] = serde_json::json!([640, 512]);
        s.update_scene(&scene.to_string()).unwrap();
        assert_eq!(s.scene().cameras[0].resolution, [640, 512]);
        assert!(s.bake(SCENARIO).is_ok());

        // An invalid edit leaves the session as it was.
        let mut bad = scene.clone();
        bad["cameras"][0]["parent"] = serde_json::json!("nowhere");
        assert!(matches!(
            s.update_scene(&bad.to_string()),
            Err(Error::Invalid { .. })
        ));
        assert_eq!(s.scene().cameras[0].resolution, [640, 512]);

        // Robots cannot be added by an edit.
        let mut more = scene.clone();
        let mut robot = more["robots"][0].clone();
        robot["id"] = serde_json::json!("other");
        more["robots"].as_array_mut().unwrap().push(robot);
        assert!(matches!(
            s.update_scene(&more.to_string()),
            Err(Error::Input(m)) if m.contains("other")
        ));
        assert_eq!(s.robots.len(), 1);
    }

    #[test]
    fn exports_and_report_come_from_the_same_bake() {
        let s = session();
        let report = s.motion_report(SCENARIO).unwrap();
        assert_eq!(report.captures, 10);
        let csv = s.export_captures(SCENARIO, "ur5e", "csv").unwrap();
        assert_eq!(csv.lines().count(), 11);
        assert!(csv.starts_with("capture,t,x,y,z,qx,qy,qz,qw,rx,ry,rz,shoulder_pan_joint"));
        let trajectory = s.export_trajectory(SCENARIO, "ur5e").unwrap();
        let baked = s.bake(SCENARIO).unwrap();
        assert_eq!(trajectory.lines().count(), baked.samples.len() + 1);
        assert!(matches!(
            s.export_captures(SCENARIO, "ur5e", "xml"),
            Err(Error::Input(_))
        ));
        assert!(matches!(
            s.export_trajectory(SCENARIO, "nope"),
            Err(Error::Input(_))
        ));
        let resolved = s.resolve(SCENARIO).unwrap();
        assert!(
            resolved
                .steps
                .iter()
                .all(|st| !matches!(st, etendue_scene::Step::PtpPose { q: None, .. }))
        );
    }

    #[test]
    fn rig_layouts_expand_or_report_their_problems() {
        let layout = serde_json::json!({
            "count": 3, "radius_m": 0.08, "working_distance_m": 0.4,
            "camera": {"params": serde_json::from_str::<serde_json::Value>(SCENE).unwrap()["cameras"][0]["params"],
                       "resolution": [1280, 1024]},
            "lasers": {"radius_m": 0.02, "fan_half_angle": 0.3, "fan_length": 1.0,
                       "wavelength_nm": 660.0, "beam_waist_m": 0.0005}
        });
        let rig = expand_rig(&layout.to_string(), "head").unwrap();
        assert_eq!(rig.cameras.len(), 3);
        assert_eq!(rig.lasers[2].camera.as_deref(), Some("head_cam2"));
        let mut bad = layout;
        bad["radius_m"] = serde_json::json!(0.0);
        assert!(matches!(
            expand_rig(&bad.to_string(), "head"),
            Err(Error::Invalid { .. })
        ));
    }

    #[test]
    fn each_camera_sees_its_lasers_stripe() {
        let s = Session::load(
            include_str!("../../../examples/rig3_laser_ur5e/scene.json"),
            &robots_json("ur5e"),
        )
        .unwrap();
        let baked = s
            .bake(include_str!(
                "../../../examples/rig3_laser_ur5e/scenario.json"
            ))
            .unwrap();
        let k = baked.capture_indices().next().unwrap();
        let at = |f: &str| {
            baked.samples[k].world_se3_frame[baked.frames.iter().position(|x| x == f).unwrap()]
        };
        for i in 0..3 {
            let (camera, laser) = (format!("rig_cam{i}"), format!("rig_laser{i}"));
            let uv = s
                .laser_stripe(
                    &camera,
                    &at(&camera),
                    &laser,
                    &at(&laser),
                    "board",
                    &at("board"),
                )
                .unwrap();
            let seen = uv.chunks(2).filter(|p| p[0].is_finite()).count();
            // The board is 200 mm wide; the stripe crosses it at 1 mm spacing.
            assert!(seen > 100, "{camera}: {seen} stripe points");
        }
        assert!(matches!(
            s.laser_stripe(
                "rig_cam0",
                &at("rig_cam0"),
                "nope",
                &at("rig_cam0"),
                "board",
                &at("board")
            ),
            Err(Error::Input(_))
        ));
    }

    #[test]
    fn targets_have_meshes_and_projected_points() {
        let s = session();
        let id = s.scene().targets[0].id.clone();
        let mesh = s.target_mesh(&id).unwrap();
        assert_eq!(mesh.triangles.len(), mesh.dark.len());
        assert_eq!(s.target_points(&id).unwrap().len(), 54);
        let baked = s.bake(SCENARIO).unwrap();
        let k = baked.capture_indices().next().unwrap();
        let at = |f: &str| {
            baked.samples[k].world_se3_frame[baked.frames.iter().position(|x| x == f).unwrap()]
        };
        let uv = s
            .project_target("cam_left", &at("cam_left"), &id, &at(&id))
            .unwrap();
        assert_eq!(uv.len(), 108);
        let seen: Vec<&[f64]> = uv.chunks(2).filter(|p| p[0].is_finite()).collect();
        assert!(!seen.is_empty(), "the board is in view at a capture");
        assert!(
            seen.iter()
                .all(|p| (-0.5..=1279.5).contains(&p[0]) && (-0.5..=1023.5).contains(&p[1])),
            "what is seen lands in the image"
        );
        let periods: serde_json::Value = serde_json::from_str(&pole_circumferences()).unwrap();
        assert!(
            periods
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["squares"] == 12)
        );
    }

    #[test]
    fn poses_are_generated_around_the_board() {
        let s = session();
        let spec = r#"{"type": "sphere", "robot": "ur5e", "look": "rig", "target": "board",
            "radii_m": [0.5], "theta": {"min_rad": 0.2, "max_rad": 0.5, "steps": 2},
            "phi": {"min_rad": 0.0, "max_rad": 6.283185307179586, "steps": 6}}"#;
        let generated = s.generate_poses(spec, 0.01).unwrap();
        assert_eq!(generated.candidates.len(), 12);
        let text = serde_json::to_string(&generated.scenario).unwrap();
        assert!(s.bake(&text).is_ok(), "the generated scenario bakes");
        assert!(matches!(
            s.generate_poses(r#"{"type": "sphere"}"#, 0.01),
            Err(Error::Parse { .. })
        ));
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
