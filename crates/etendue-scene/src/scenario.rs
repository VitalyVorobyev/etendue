//! Robot motion scenarios ([ADR 0003](https://github.com/VitalyVorobyev/etendue/blob/main/docs/adrs/0003-baking.md)).
//!
//! A scenario is a sequential program of steps. Steps run one after another:
//! a motion step moves one robot while every other robot holds still, and
//! every motion starts and ends at rest. Capture is **stop-and-shoot only**
//! (v1): a [`Step::Capture`] marks one stationary sample.

use nalgebra::Isometry3;
use serde::{Deserialize, Serialize};

use crate::SceneSpec;
use crate::validate::{Issues, ValidationError};

/// A motion scenario, version 1.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ScenarioSpec {
    /// Format version; must be `1`.
    pub version: u32,
    /// Baking sample period in seconds, `> 0`.
    pub dt: f64,
    /// The program, executed in order.
    pub steps: Vec<Step>,
    /// Free-text description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The generator the steps were made by, kept so it can be edited and
    /// run again; the steps are the source of truth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator: Option<GeneratorSpec>,
}

/// How a scenario's poses were generated, tagged on `type`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum GeneratorSpec {
    /// Poses on spheres around the target, the sensor looking at the centre.
    Sphere(SphereGenerator),
}

/// Eye-in-hand poses on spheres around a target (`etendue-synth::poses`).
///
/// The sensor frame (`look`, a frame the robot's tool carries, offset by
/// `look_se3_sensor`) is placed at radius `r`, polar angle `θ` from the
/// sphere's pole and azimuth `φ`, its +Z towards the centre and its +Y along
/// the target's print-down direction (so images read upright), then rolled
/// about +Z. Every `(r, θ, φ, roll)` is a candidate; one is kept when the
/// robot reaches it with joint margin and away from singularities, and every
/// required camera sees enough of the target.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct SphereGenerator {
    /// The robot that carries the sensor.
    pub robot: String,
    /// The sensor frame: a rig, a camera, or any frame rigidly carried by
    /// the robot's tool.
    pub look: String,
    /// The sensor axis relative to `look` (identity by default).
    #[cfg_attr(
        feature = "schemars",
        schemars(with = "Option<vision_calibration_core::Iso3Schema>")
    )]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look_se3_sensor: Option<Isometry3<f64>>,
    /// The target looked at.
    pub target: String,
    /// Sphere centre in the target frame, metres; default the target's
    /// centre (a PuzzlePole's axis midpoint).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub centre_m: Option<[f64; 3]>,
    /// The sphere's pole (θ = 0) in the target frame; default +Z, the
    /// board's normal (for a PuzzlePole, +X, the seam's outward normal).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pole: Option<[f64; 3]>,
    /// Sphere radii, metres.
    pub radii_m: Vec<f64>,
    /// Polar angles from the pole.
    pub theta: AngleSteps,
    /// Azimuths about the pole.
    pub phi: AngleSteps,
    /// Rolls of the sensor about its axis, radians (default `[0]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roll_rad: Vec<f64>,
    /// Which cameras must see the target, and how much of it.
    #[serde(default)]
    pub require: Visibility,
    /// Smallest distance of any joint from its limits (rad or m).
    #[serde(default = "default_joint_margin")]
    pub joint_margin_rad: f64,
    /// Smallest singular value of the tool Jacobian.
    #[serde(default = "default_min_sigma")]
    pub min_sigma: f64,
    /// Fraction of the joint limits the moves use, in `(0, 1]`.
    #[serde(default = "default_generator_speed")]
    pub speed_scale: f64,
    /// Go back to the start configuration at the end.
    #[serde(default)]
    pub return_home: bool,
}

/// `steps` angles from `min_rad` to `max_rad`, both included (a full turn
/// counts its start once).
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct AngleSteps {
    /// First angle, radians.
    pub min_rad: f64,
    /// Last angle, radians.
    pub max_rad: f64,
    /// Number of angles, ≥ 1.
    pub steps: u32,
}

impl AngleSteps {
    /// The angles.
    #[must_use]
    pub fn values(&self) -> Vec<f64> {
        let n = self.steps.max(1);
        if n == 1 {
            return vec![self.min_rad];
        }
        let span = self.max_rad - self.min_rad;
        // A full turn: the last angle would repeat the first.
        let full = (span.abs() - std::f64::consts::TAU).abs() < 1e-9;
        let parts = if full { f64::from(n) } else { f64::from(n - 1) };
        (0..n)
            .map(|k| self.min_rad + span * f64::from(k) / parts)
            .collect()
    }
}

/// Which cameras must see the target.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Visibility {
    /// The cameras; all of the scene's when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cameras: Option<Vec<String>>,
    /// Fraction of the target's points each must see, in `(0, 1]`.
    #[serde(default = "default_visible_fraction")]
    pub min_visible_fraction: f64,
    /// Image border margin, pixels.
    #[serde(default = "default_margin_px")]
    pub margin_px: f64,
}

impl Default for Visibility {
    fn default() -> Self {
        Self {
            cameras: None,
            min_visible_fraction: default_visible_fraction(),
            margin_px: default_margin_px(),
        }
    }
}

fn default_joint_margin() -> f64 {
    0.05
}

fn default_min_sigma() -> f64 {
    0.02
}

fn default_generator_speed() -> f64 {
    0.5
}

fn default_visible_fraction() -> f64 {
    1.0
}

fn default_margin_px() -> f64 {
    2.0
}

fn default_speed_scale() -> f64 {
    1.0
}

/// One scenario step, tagged on `type`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Step {
    /// Point-to-point move to a joint configuration: straight in joint space,
    /// every joint on one synchronised jerk-limited profile (Ruckig;
    /// trapezoidal when no moving joint has a jerk limit).
    PtpJoints {
        /// Robot id.
        robot: String,
        /// Target joint positions, manifest joint order.
        q: Vec<f64>,
        /// Fraction of the joint velocity, acceleration and jerk limits, in
        /// `(0, 1]`.
        #[serde(default = "default_speed_scale")]
        speed_scale: f64,
    },
    /// Point-to-point move to a tool pose (inverse kinematics from the
    /// current configuration, then as [`Step::PtpJoints`]).
    PtpPose {
        /// Robot id.
        robot: String,
        /// Target pose of the TCP link in the robot base frame.
        #[cfg_attr(
            feature = "schemars",
            schemars(with = "vision_calibration_core::Iso3Schema")
        )]
        base_se3_tool: Isometry3<f64>,
        /// Fraction of the joint velocity, acceleration and jerk limits, in
        /// `(0, 1]`.
        #[serde(default = "default_speed_scale")]
        speed_scale: f64,
        /// The joint configuration that reaches `base_se3_tool`, when it has
        /// been chosen (a pose generator, `resolve`): the move then goes
        /// there without inverse kinematics, so baking can never switch to
        /// another IK branch. It must reach the pose (checked when the
        /// scenario is compiled).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        q: Option<Vec<f64>>,
    },
    /// Straight-line Cartesian move of the TCP (translation linear, rotation
    /// slerp), time-scaled so no joint limit is exceeded.
    Lin {
        /// Robot id.
        robot: String,
        /// Target pose of the TCP link in the robot base frame.
        #[cfg_attr(
            feature = "schemars",
            schemars(with = "vision_calibration_core::Iso3Schema")
        )]
        base_se3_tool: Isometry3<f64>,
        /// Fraction of the joint velocity, acceleration and jerk limits, in
        /// `(0, 1]`.
        #[serde(default = "default_speed_scale")]
        speed_scale: f64,
    },
    /// Take one image with every camera: one stationary sample, flagged.
    Capture {
        /// Capture id; defaults to `cap_NNN` (running index over captures).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
    /// Hold still.
    Wait {
        /// Duration in seconds, `≥ 0`.
        duration_s: f64,
    },
}

impl ScenarioSpec {
    /// Structural checks against `scene`: version, `dt`, robot ids, speed
    /// scales, poses, wait durations, and unique capture ids.
    ///
    /// Joint-vector lengths, limits, and reachability need the robot models
    /// and are checked when the scenario is compiled (`etendue-kinematics`).
    ///
    /// # Errors
    ///
    /// Every problem found.
    pub fn validate(&self, scene: &SceneSpec) -> Result<(), ValidationError> {
        let mut issues = Issues::default();
        if self.version != 1 {
            issues.push(
                "version",
                format!("unsupported scenario version {}", self.version),
            );
        }
        issues.check_positive("dt", self.dt);
        let robot_exists = |id: &str| scene.robots.iter().any(|r| r.id == id);
        let mut capture_ids: Vec<String> = Vec::new();
        let mut n_captures = 0usize;
        for (i, step) in self.steps.iter().enumerate() {
            let path = format!("steps[{i}]");
            match step {
                Step::PtpJoints {
                    robot,
                    q,
                    speed_scale,
                } => {
                    check_robot(&mut issues, &path, robot, robot_exists(robot));
                    check_speed_scale(&mut issues, &path, *speed_scale);
                    if !q.iter().all(|v| v.is_finite()) {
                        issues.push(format!("{path}.q"), "joint positions must be finite");
                    }
                }
                Step::PtpPose {
                    robot,
                    base_se3_tool,
                    speed_scale,
                    q,
                } => {
                    check_robot(&mut issues, &path, robot, robot_exists(robot));
                    check_speed_scale(&mut issues, &path, *speed_scale);
                    issues.check_pose(&format!("{path}.base_se3_tool"), base_se3_tool);
                    if q.as_ref().is_some_and(|q| !q.iter().all(|v| v.is_finite())) {
                        issues.push(format!("{path}.q"), "joint positions must be finite");
                    }
                }
                Step::Lin {
                    robot,
                    base_se3_tool,
                    speed_scale,
                } => {
                    check_robot(&mut issues, &path, robot, robot_exists(robot));
                    check_speed_scale(&mut issues, &path, *speed_scale);
                    issues.check_pose(&format!("{path}.base_se3_tool"), base_se3_tool);
                }
                Step::Capture { id } => {
                    let id = id.clone().unwrap_or_else(|| default_capture_id(n_captures));
                    if capture_ids.contains(&id) {
                        issues.push(format!("{path}.id"), format!("duplicate capture id `{id}`"));
                    }
                    capture_ids.push(id);
                    n_captures += 1;
                }
                Step::Wait { duration_s } => {
                    if !(duration_s.is_finite() && *duration_s >= 0.0) {
                        issues.push(
                            format!("{path}.duration_s"),
                            format!("must be finite and ≥ 0, got {duration_s}"),
                        );
                    }
                }
            }
        }
        issues.finish()
    }
}

impl ScenarioSpec {
    /// A stop-and-shoot scenario from tool poses: for each pose a
    /// [`Step::PtpPose`] of `robot`, then a [`Step::Capture`] with the pose's
    /// id (or the default one). Sampled at `dt` seconds.
    ///
    /// Reachability is not checked here; it needs the robot model
    /// (`etendue-kinematics` `compile`).
    #[must_use]
    pub fn from_poses(
        robot: &str,
        poses: &[(Option<String>, Isometry3<f64>)],
        speed_scale: f64,
        dt: f64,
    ) -> Self {
        let steps = poses
            .iter()
            .flat_map(|(id, pose)| {
                [
                    Step::PtpPose {
                        robot: robot.to_owned(),
                        base_se3_tool: *pose,
                        speed_scale,
                        q: None,
                    },
                    Step::Capture { id: id.clone() },
                ]
            })
            .collect();
        Self {
            version: 1,
            dt,
            steps,
            description: Some(format!("{} tool poses of `{robot}`", poses.len())),
            generator: None,
        }
    }
}

/// The id given to the `index`-th capture (0-based) when a
/// [`Step::Capture`] has none: `cap_000`, `cap_001`, …
#[must_use]
pub fn default_capture_id(index: usize) -> String {
    format!("cap_{index:03}")
}

fn check_robot(issues: &mut Issues, path: &str, robot: &str, exists: bool) {
    if !exists {
        issues.push(format!("{path}.robot"), format!("unknown robot `{robot}`"));
    }
}

fn check_speed_scale(issues: &mut Issues, path: &str, s: f64) {
    if !(s.is_finite() && s > 0.0 && s <= 1.0) {
        issues.push(
            format!("{path}.speed_scale"),
            format!("must be in (0, 1], got {s}"),
        );
    }
}
