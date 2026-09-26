//! Analytic inverse kinematics for OPW-type arms (feature `opw`).
//!
//! Wraps `rs-opw-kinematics` (glam-based). The glam ↔ nalgebra conversion
//! happens only in this module. Every analytic solution is **checked
//! against this crate's own URDF forward kinematics** before it is returned,
//! so a preset whose frames do not match the URDF fails loudly rather than
//! silently.

use nalgebra::{Isometry3, Quaternion, Translation3, UnitQuaternion};
use rs_opw_kinematics::glam::{DQuat, DVec3};
use rs_opw_kinematics::kinematic_traits::Kinematics;
use rs_opw_kinematics::kinematics_impl::OPWKinematics;
use rs_opw_kinematics::parameters::opw_kinematics::Parameters;
use rs_opw_kinematics::pose::Pose;

use crate::error::{Error, Result};
use crate::ik::IkSolution;
use crate::urdf::RobotModel;

/// Largest pose error an analytic solution may have against the URDF model
/// (metres / radians) before it is rejected as a frame mismatch.
pub const OPW_URDF_TOLERANCE: f64 = 1e-9;

/// An OPW parameter set whose frames are `base → tcp_link` of a robot
/// manifest (REP-199 `base`, ROS-Industrial `tool0`).
pub struct OpwSolver {
    kinematics: OPWKinematics,
}

impl OpwSolver {
    /// A validated `rs-opw-kinematics` preset, by name:
    ///
    /// - `"irb1200_5_90"` — ABB IRB 1200-5/0.90
    ///   (`assets/robots/abb_irb1200_5_90`), ROS-Industrial joint coordinates.
    ///
    /// Returns `None` for names that have not been validated against an
    /// etendue robot asset.
    #[must_use]
    pub fn preset(name: &str) -> Option<Self> {
        let parameters = match name {
            "irb1200_5_90" => Parameters::irb1200_5_90(),
            _ => return None,
        };
        Some(Self {
            kinematics: OPWKinematics::new(parameters),
        })
    }

    /// Every analytic solution for `base_se3_tcp` (up to 8), unfiltered.
    #[must_use]
    pub fn solutions(&self, base_se3_tcp: &Isometry3<f64>) -> Vec<[f64; 6]> {
        let t = base_se3_tcp.translation.vector;
        let q = base_se3_tcp.rotation.quaternion();
        let pose = Pose::from_parts(
            DVec3::new(t.x, t.y, t.z),
            DQuat::from_xyzw(q.i, q.j, q.k, q.w),
        );
        self.kinematics.inverse(&pose)
    }

    /// The preset's own forward kinematics.
    #[must_use]
    pub fn forward(&self, q: &[f64; 6]) -> Isometry3<f64> {
        let pose = self.kinematics.forward(q);
        let (t, r) = (pose.translation, pose.rotation);
        Isometry3::from_parts(
            Translation3::new(t.x, t.y, t.z),
            UnitQuaternion::from_quaternion(Quaternion::new(r.w, r.x, r.y, r.z)),
        )
    }
}

impl RobotModel {
    /// Analytic IK: the OPW solution nearest to `seed` (Euclidean in joint
    /// space) that lies inside the joint limits and reproduces
    /// `base_se3_tcp` under this model's URDF forward kinematics to
    /// [`OPW_URDF_TOLERANCE`]. Solutions are also tried shifted by ±2π per
    /// joint where the limits span more than one turn.
    ///
    /// # Errors
    ///
    /// - [`Error::InvalidJoints`] if the model does not have 6 joints or the
    ///   seed has the wrong length;
    /// - [`Error::IkFailed`] if no solution passes the checks (unreachable
    ///   pose, joint limits, or a preset/URDF frame mismatch).
    pub fn ik_opw(
        &self,
        solver: &OpwSolver,
        base_se3_tcp: &Isometry3<f64>,
        seed: &[f64],
    ) -> Result<IkSolution> {
        if self.dof() != 6 || seed.len() != 6 {
            return Err(Error::InvalidJoints(format!(
                "OPW needs a 6-joint robot and seed, got {} joints and {} values",
                self.dof(),
                seed.len()
            )));
        }
        let tau = std::f64::consts::TAU;
        let mut best: Option<(f64, IkSolution)> = None;
        for sol in solver.solutions(base_se3_tcp) {
            // Each joint: the analytic value and its ±2π shifts inside the limits.
            let choices: Vec<Vec<f64>> = sol
                .iter()
                .zip(self.active_joints())
                .map(|(&v, j)| {
                    [v - tau, v, v + tau]
                        .into_iter()
                        .filter(|c| c.is_finite() && *c >= j.lower && *c <= j.upper)
                        .collect()
                })
                .collect();
            if choices.iter().any(Vec::is_empty) {
                continue;
            }
            // Per joint the nearest admissible value to the seed.
            let q: Vec<f64> = choices
                .iter()
                .zip(seed)
                .map(|(c, s)| {
                    *c.iter()
                        .min_by(|a, b| (*a - s).abs().total_cmp(&(*b - s).abs()))
                        .expect("non-empty")
                })
                .collect();
            let reached = self.tcp_pose(&q);
            let dt = (reached.translation.vector - base_se3_tcp.translation.vector).norm();
            let dr = reached.rotation.angle_to(&base_se3_tcp.rotation);
            if dt > OPW_URDF_TOLERANCE || dr > OPW_URDF_TOLERANCE {
                continue;
            }
            let dist: f64 = q.iter().zip(seed).map(|(a, b)| (a - b) * (a - b)).sum();
            if best.as_ref().is_none_or(|(d, _)| dist < *d) {
                best = Some((
                    dist,
                    IkSolution {
                        q,
                        iterations: 0,
                        attempt: 0,
                        translation_error: dt,
                        rotation_error: dr,
                    },
                ));
            }
        }
        best.map(|(_, s)| s).ok_or_else(|| {
            Error::IkFailed(format!(
                "robot `{}`: no OPW solution inside the joint limits matches the URDF",
                self.id
            ))
        })
    }
}
