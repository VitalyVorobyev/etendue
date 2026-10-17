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
}

fn default_speed_scale() -> f64 {
    1.0
}

/// One scenario step, tagged on `type`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Step {
    /// Point-to-point move to a joint configuration: synchronised
    /// trapezoidal profiles, straight in joint space.
    PtpJoints {
        /// Robot id.
        robot: String,
        /// Target joint positions, manifest joint order.
        q: Vec<f64>,
        /// Fraction of the joint velocity and acceleration limits, in
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
        /// Fraction of the joint velocity and acceleration limits, in
        /// `(0, 1]`.
        #[serde(default = "default_speed_scale")]
        speed_scale: f64,
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
        /// Fraction of the joint velocity and acceleration limits, in
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
                }
                | Step::Lin {
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
