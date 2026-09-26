//! Versioned scene and scenario schema for **etendue**.
//!
//! This crate is the data contract shared by the etendue tools, the web
//! packages (via JSON Schema), and the Blender backend:
//!
//! - [`SceneSpec`] — a frame tree of robots, rigs, cameras, lasers, lights,
//!   targets, and parts ([ADR 0002]).
//! - [`ScenarioSpec`] — a robot motion program with stop-and-shoot captures.
//! - [`BakedScenario`] — the scenario sampled by `etendue-kinematics`:
//!   `world_se3_frame` for every frame per sample ([ADR 0003]).
//! - [`RobotManifest`] — the `robot.json` robot asset manifest.
//!
//! # Conventions
//!
//! - Transforms are named `a_se3_b` (maps `b` coordinates into `a`,
//!   calibration-rs ADR 0009). On the wire an SE(3) is
//!   `{"rotation": [qx, qy, qz, qw], "translation": [tx, ty, tz]}`
//!   (nalgebra `Isometry3` serde; unit quaternion to 1e-9, metres).
//! - World frame: right-handed, +Z up, metres. Cameras use the
//!   calibration-rs/OpenCV frame (+Z forward, +X right, +Y down).
//! - Units are metres and radians unless a field name says otherwise.
//! - Enums are tagged on `type` in `snake_case`; structs reject unknown
//!   fields.
//!
//! The crate does no I/O and no kinematics: [`FrameGraph`] resolves the
//! attachment tree given link poses computed elsewhere.
//!
//! **Parsing floats exactly.** Parse these documents with `serde_json`'s
//! `float_roundtrip` feature enabled. Its default float parser can be off by
//! one ULP, which breaks bit-exact round trips of poses and ground truth.
//!
//! [ADR 0002]: https://github.com/VitalyVorobyev/etendue/blob/main/docs/adrs/0002-frame-tree.md
//! [ADR 0003]: https://github.com/VitalyVorobyev/etendue/blob/main/docs/adrs/0003-baking.md

pub mod baked;
pub mod entity;
pub mod frame;
pub mod light;
pub mod robot;
pub mod scenario;
pub mod validate;

use std::collections::BTreeMap;

use nalgebra::Isometry3;
use serde::{Deserialize, Serialize};

pub use baked::{BakedRobot, BakedSample, BakedScenario, CaptureEvent};
pub use entity::{CameraSpec, LaserSpec, PartSpec, RigSpec, TargetGeometry, TargetSpec};
pub use frame::{FrameGraph, FrameRef, FrameSpec, ParsedFrameRef, WORLD, is_valid_id};
pub use light::{LightShape, LightSpec};
pub use robot::{
    ManifestJoint, ManifestLicense, ManifestSource, ManifestVisual, RobotManifest, RobotSpec,
};
pub use scenario::{ScenarioSpec, Step, default_capture_id};
pub use validate::{Issue, UNIT_QUATERNION_TOLERANCE, ValidationError};

use validate::Issues;

/// The scene format version this crate reads and writes.
pub const SCENE_VERSION: u32 = 1;

/// A scene: every frame, robot, and entity, version 1.
///
/// All ids — of frames, robots, rigs, cameras, lasers, lights, targets, and
/// parts — share one namespace.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct SceneSpec {
    /// Format version; must be [`SCENE_VERSION`].
    pub version: u32,
    /// Named auxiliary frames (fixtures, mounting plates, tool offsets).
    #[serde(default)]
    pub frames: Vec<FrameSpec>,
    /// Robots.
    #[serde(default)]
    pub robots: Vec<RobotSpec>,
    /// Camera rigs.
    #[serde(default)]
    pub rigs: Vec<RigSpec>,
    /// Cameras.
    #[serde(default)]
    pub cameras: Vec<CameraSpec>,
    /// Line lasers.
    #[serde(default)]
    pub lasers: Vec<LaserSpec>,
    /// Light sources.
    #[serde(default)]
    pub lights: Vec<LightSpec>,
    /// Targets (calibration boards, plain planes).
    #[serde(default)]
    pub targets: Vec<TargetSpec>,
    /// Passive mesh parts.
    #[serde(default)]
    pub parts: Vec<PartSpec>,
    /// Free-text description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A view of one placed item: its id, parent, and pose.
#[derive(Clone, Copy, Debug)]
pub struct Mount<'a> {
    /// Which `SceneSpec` array the item is in (`"cameras"`, …).
    pub kind: &'static str,
    /// Index in that array.
    pub index: usize,
    /// The item's id.
    pub id: &'a str,
    /// Parent frame.
    pub parent: &'a FrameRef,
    /// Pose in the parent.
    pub parent_se3_self: &'a Isometry3<f64>,
}

impl Mount<'_> {
    /// Document path of the item, e.g. `cameras[2]`.
    #[must_use]
    pub fn path(&self) -> String {
        format!("{}[{}]", self.kind, self.index)
    }
}

impl SceneSpec {
    /// An empty version-1 scene.
    #[must_use]
    pub fn new() -> Self {
        Self {
            version: SCENE_VERSION,
            frames: Vec::new(),
            robots: Vec::new(),
            rigs: Vec::new(),
            cameras: Vec::new(),
            lasers: Vec::new(),
            lights: Vec::new(),
            targets: Vec::new(),
            parts: Vec::new(),
            description: None,
        }
    }

    /// Every placed item, in declaration order: frames, robots, rigs,
    /// cameras, lasers, lights, targets, parts.
    pub fn mounted(&self) -> impl Iterator<Item = Mount<'_>> {
        macro_rules! mounts {
            ($field:ident) => {
                self.$field.iter().enumerate().map(|(index, e)| Mount {
                    kind: stringify!($field),
                    index,
                    id: &e.id,
                    parent: &e.parent,
                    parent_se3_self: &e.parent_se3_self,
                })
            };
        }
        mounts!(frames)
            .chain(mounts!(robots))
            .chain(mounts!(rigs))
            .chain(mounts!(cameras))
            .chain(mounts!(lasers))
            .chain(mounts!(lights))
            .chain(mounts!(targets))
            .chain(mounts!(parts))
    }

    /// Structural validation that needs no robot model.
    ///
    /// Checks the version; id syntax and uniqueness; unit-quaternion poses;
    /// parent references (grammar, existence of the entity or robot, no bare
    /// robot ids, no cycles); and per-entity parameters (camera resolution
    /// and buildable projection, laser fan, target extent, light power and
    /// colour).
    ///
    /// That a `"<robot>/<link>"` link exists is checked by
    /// [`FrameGraph::build`] once the robot models are loaded.
    ///
    /// # Errors
    ///
    /// Every problem found.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut issues = Issues::default();
        if self.version != SCENE_VERSION {
            issues.push(
                "version",
                format!(
                    "unsupported scene version {} (expected {SCENE_VERSION})",
                    self.version
                ),
            );
        }

        let mut seen: BTreeMap<&str, String> = BTreeMap::new();
        for m in self.mounted() {
            let path = m.path();
            if !is_valid_id(m.id) {
                issues.push(
                    format!("{path}.id"),
                    format!("`{}` is not a valid id ([A-Za-z0-9_-]+, not `world`)", m.id),
                );
            } else if let Some(first) = seen.get(m.id) {
                issues.push(
                    format!("{path}.id"),
                    format!("duplicate id `{}` (first used by {first})", m.id),
                );
            } else {
                seen.insert(m.id, path.clone());
            }
            issues.check_pose(&format!("{path}.parent_se3_self"), m.parent_se3_self);
        }

        self.validate_entities(&mut issues);

        // Parent resolution and cycles. Robot link names are unknown without
        // the models, so accept any link a robot is referenced with here.
        if issues.is_empty() {
            let mut referenced: Vec<Vec<String>> = vec![Vec::new(); self.robots.len()];
            for m in self.mounted() {
                if let Ok(ParsedFrameRef::RobotLink { robot, link }) = m.parent.parse()
                    && let Some(r) = self.robots.iter().position(|x| x.id == robot)
                    && !referenced[r].iter().any(|l| l == link)
                {
                    referenced[r].push(link.to_owned());
                }
            }
            if let Err(e) = FrameGraph::build(self, &referenced) {
                for issue in e.issues {
                    issues.push(issue.path, issue.message);
                }
            }
        }
        issues.finish()
    }

    fn validate_entities(&self, issues: &mut Issues) {
        for (i, r) in self.robots.iter().enumerate() {
            if r.manifest.is_empty() {
                issues.push(format!("robots[{i}].manifest"), "must not be empty");
            }
            if let Some(q) = &r.initial_q
                && !q.iter().all(|v| v.is_finite())
            {
                issues.push(
                    format!("robots[{i}].initial_q"),
                    "joint positions must be finite",
                );
            }
        }
        for (i, c) in self.cameras.iter().enumerate() {
            let path = format!("cameras[{i}]");
            if c.resolution[0] == 0 || c.resolution[1] == 0 {
                issues.push(
                    format!("{path}.resolution"),
                    format!("must be non-zero, got {:?}", c.resolution),
                );
            }
            if let Err(e) = c.params.build() {
                issues.push(
                    format!("{path}.params"),
                    format!("camera model does not build: {e}"),
                );
            }
        }
        for (i, l) in self.lasers.iter().enumerate() {
            let path = format!("lasers[{i}]");
            if !(l.fan_half_angle.is_finite()
                && l.fan_half_angle > 0.0
                && l.fan_half_angle < std::f64::consts::FRAC_PI_2)
            {
                issues.push(
                    format!("{path}.fan_half_angle"),
                    format!("must be in (0, π/2), got {}", l.fan_half_angle),
                );
            }
            issues.check_positive(&format!("{path}.fan_length"), l.fan_length);
            issues.check_positive(&format!("{path}.wavelength_nm"), l.wavelength_nm);
            issues.check_positive(&format!("{path}.beam_waist_m"), l.beam_waist_m);
        }
        for (i, t) in self.targets.iter().enumerate() {
            if let TargetGeometry::Rectangle { width, height } = t.geometry {
                issues.check_positive(&format!("targets[{i}].geometry.width"), width);
                issues.check_positive(&format!("targets[{i}].geometry.height"), height);
            }
        }
        for (i, l) in self.lights.iter().enumerate() {
            let path = format!("lights[{i}]");
            if !(l.power_w.is_finite() && l.power_w >= 0.0) {
                issues.push(
                    format!("{path}.power_w"),
                    format!("must be finite and ≥ 0, got {}", l.power_w),
                );
            }
            if !l
                .color
                .iter()
                .all(|c| c.is_finite() && (0.0..=1.0).contains(c))
            {
                issues.push(format!("{path}.color"), "channels must be in [0, 1]");
            }
            match l.shape {
                LightShape::Point { radius_m } => {
                    if !(radius_m.is_finite() && radius_m >= 0.0) {
                        issues.push(format!("{path}.shape.radius_m"), "must be finite and ≥ 0");
                    }
                }
                LightShape::Spot { cone_angle, blend } => {
                    if !(cone_angle.is_finite()
                        && cone_angle > 0.0
                        && cone_angle < std::f64::consts::PI)
                    {
                        issues.push(format!("{path}.shape.cone_angle"), "must be in (0, π)");
                    }
                    if !(0.0..=1.0).contains(&blend) {
                        issues.push(format!("{path}.shape.blend"), "must be in [0, 1]");
                    }
                }
                LightShape::Area { size_m } => {
                    issues.check_positive(&format!("{path}.shape.size_m[0]"), size_m[0]);
                    issues.check_positive(&format!("{path}.shape.size_m[1]"), size_m[1]);
                }
            }
        }
        for (i, p) in self.parts.iter().enumerate() {
            if p.mesh.is_empty() {
                issues.push(format!("parts[{i}].mesh"), "must not be empty");
            }
        }
    }
}

impl Default for SceneSpec {
    fn default() -> Self {
        Self::new()
    }
}
