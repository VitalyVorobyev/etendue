//! Robot kinematics for **etendue** — the only place FK and IK live
//! ([ADR 0003](https://github.com/VitalyVorobyev/etendue/blob/main/docs/adrs/0003-baking.md)).
//!
//! - [`RobotModel`] — a URDF kinematic tree (via `urdf-rs`) bound to its
//!   `robot.json` manifest ([`etendue_scene::RobotManifest`]).
//! - Forward kinematics: [`RobotModel::link_poses`], [`RobotModel::tcp_pose`],
//!   [`RobotModel::tcp_jacobian`] — custom serial-chain code on nalgebra 0.34
//!   (the `k` crate is excluded by the nalgebra hard pin).
//! - Inverse kinematics: [`RobotModel::ik`], damped least squares with
//!   deterministic restarts; with the `opw` feature also analytic OPW IK
//!   (`RobotModel::ik_opw`, via `rs-opw-kinematics`).
//! - Scenarios: [`compile`] (trapezoidal PTP and Cartesian LIN motions with
//!   stop-and-shoot captures) and [`bake`] (→ [`etendue_scene::BakedScenario`]).
//!
//! All poses are relative to the robot's base link and use the
//! `a_se3_b` naming of calibration-rs ADR 0009. The crate does no I/O.

pub mod error;
pub mod fk;
pub mod ik;
#[cfg(feature = "opw")]
pub mod opw;
pub mod scenario;
mod trajectory;
pub mod urdf;

pub use error::{Error, Result};
pub use ik::{IkOptions, IkSolution};
pub use scenario::{JointState, Trajectory, TrajectorySample, bake, compile};
pub use urdf::{ActiveJoint, Joint, JointKind, RobotModel};
