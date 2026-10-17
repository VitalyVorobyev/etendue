//! Motion reports: how a compiled scenario uses the robots' limits.
//!
//! `etendue-kinematics` computes a [`MotionReport`] from a scenario; this
//! module only defines the document, so its JSON Schema sits beside the
//! scene's and the scenario's.

use serde::{Deserialize, Serialize};

/// Feasibility of every step of a scenario, version 1.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct MotionReport {
    /// Format version; `1`.
    pub version: u32,
    /// Total duration, seconds.
    pub duration_s: f64,
    /// Number of captures.
    pub captures: usize,
    /// One entry per scenario step, in order.
    pub steps: Vec<StepReport>,
}

/// One step of the program.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct StepReport {
    /// Index into the scenario's `steps`.
    pub step: usize,
    /// The step's `type` (`ptp_joints`, `ptp_pose`, `lin`, `capture`, `wait`).
    pub kind: String,
    /// The robot it moves, for a motion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub robot: Option<String>,
    /// Start time, seconds.
    pub t0: f64,
    /// Duration, seconds (`0` for a motion that does not move).
    pub duration_s: f64,
    /// Per joint of the moving robot: the largest |velocity|, |acceleration|
    /// and |jerk| over the step as fractions of the joint's limits.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub peaks: Vec<JointPeak>,
    /// The joint and quantity closest to its limit: what sets the step's
    /// duration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<Binding>,
    /// Smallest distance of any joint from its position limits over the
    /// step (rad or m).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_limit_margin: Option<f64>,
    /// Smallest singular value of the 6×n TCP Jacobian over the step
    /// (translation rows in m/rad, rotation rows in rad/rad): near zero at
    /// a singularity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_sigma: Option<f64>,
    /// How a `ptp_pose` found its joint configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ik: Option<IkReport>,
    /// For a `lin`: the largest joint change between consecutive IK
    /// waypoints (rad or m).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_joint_jump: Option<f64>,
    /// What deserves a look.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<MotionWarning>,
}

/// Peak use of one joint's limits over a step, as fractions of the limits
/// (`1` = at the limit; `speed_scale` keeps them at or below it).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct JointPeak {
    /// Joint name.
    pub joint: String,
    /// max |q̇| / velocity limit.
    pub velocity: f64,
    /// max |q̈| / acceleration limit.
    pub acceleration: f64,
    /// max |q⃛| / jerk limit, if the joint has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jerk: Option<f64>,
}

/// Which limit binds a step.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Binding {
    /// Joint name.
    pub joint: String,
    /// The limited quantity.
    pub quantity: Quantity,
    /// Its peak as a fraction of the limit.
    pub fraction: f64,
}

/// A joint quantity with a limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Quantity {
    /// Joint speed.
    Velocity,
    /// Joint acceleration.
    Acceleration,
    /// Joint jerk.
    Jerk,
}

/// How a `ptp_pose` step's configuration was found.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
pub enum IkReport {
    /// The step's own `q` (checked to reach the pose).
    Stored,
    /// Inverse kinematics from the robot's configuration before the step.
    Solved {
        /// `0`: converged from the current configuration; `k`: on restart
        /// `k`, from a seed elsewhere in the joint space — likely another IK
        /// branch, so the arm may swing far.
        attempt: usize,
        /// Iterations, all attempts.
        iterations: usize,
    },
}

/// Something about a step that deserves a look, tagged on `type`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum MotionWarning {
    /// IK converged only from a restart seed: the configuration may be on
    /// another branch than the robot's current one. Store `q` (resolve the
    /// scenario) to pin the branch.
    IkRestartUsed {
        /// The restart that converged.
        attempt: usize,
    },
    /// A joint comes within `margin` of a position limit.
    NearLimit {
        /// Joint name.
        joint: String,
        /// Its smallest distance from a limit (rad or m).
        margin: f64,
    },
    /// The TCP Jacobian's smallest singular value drops below the
    /// threshold: the arm passes near a singularity.
    NearSingular {
        /// The smallest singular value.
        sigma: f64,
    },
    /// A `lin` move's joints change fast between waypoints.
    LargeJointJump {
        /// The largest change (rad or m).
        jump: f64,
    },
}
