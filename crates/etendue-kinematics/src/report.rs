//! Motion reports: how close each step of a compiled scenario comes to the
//! robots' limits, joint ranges and singularities.

use etendue_scene::{
    Binding, IkReport, JointPeak, MotionReport, MotionWarning, Quantity, ScenarioSpec, SceneSpec,
    Step, StepReport,
};

use crate::error::Result;
use crate::scenario::{Trajectory, compile};
use crate::urdf::RobotModel;

/// When a step earns a warning.
#[derive(Clone, Copy, Debug)]
pub struct ReportOptions {
    /// A joint within this distance of a position limit (rad or m).
    pub near_limit: f64,
    /// The TCP Jacobian's smallest singular value below this.
    pub near_singular: f64,
    /// A `lin` joint change between waypoints above this (rad or m).
    pub large_joint_jump: f64,
}

impl Default for ReportOptions {
    fn default() -> Self {
        Self {
            near_limit: 2.0_f64.to_radians(),
            near_singular: 0.02,
            large_joint_jump: 0.05,
        }
    }
}

fn kind(step: &Step) -> &'static str {
    match step {
        Step::PtpJoints { .. } => "ptp_joints",
        Step::PtpPose { .. } => "ptp_pose",
        Step::Lin { .. } => "lin",
        Step::Capture { .. } => "capture",
        Step::Wait { .. } => "wait",
    }
}

/// Compile `scenario` and report on every step.
///
/// # Errors
///
/// As [`compile`].
pub fn report(
    scene: &SceneSpec,
    scenario: &ScenarioSpec,
    robots: &[RobotModel],
    options: &ReportOptions,
) -> Result<MotionReport> {
    let trajectory = compile(scene, scenario, robots)?;
    Ok(report_trajectory(
        scene,
        scenario,
        &trajectory,
        robots,
        options,
    ))
}

/// Report on a compiled trajectory (see [`report`]).
#[must_use]
pub fn report_trajectory(
    scene: &SceneSpec,
    scenario: &ScenarioSpec,
    trajectory: &Trajectory,
    robots: &[RobotModel],
    options: &ReportOptions,
) -> MotionReport {
    let dt = trajectory.dt;
    let steps = trajectory
        .steps
        .iter()
        .zip(&scenario.steps)
        .map(|(span, spec)| {
            let mut out = StepReport {
                step: span.step,
                kind: kind(spec).to_owned(),
                robot: span.robot.map(|r| scene.robots[r].id.clone()),
                t0: span.samples.start as f64 * dt,
                duration_s: span.samples.len() as f64 * dt,
                peaks: Vec::new(),
                binding: None,
                min_limit_margin: None,
                min_sigma: None,
                ik: span.ik.as_ref().map(|(ik, _)| ik.clone()),
                max_joint_jump: span.max_joint_jump,
                warnings: Vec::new(),
            };
            if let Some(IkReport::Solved { attempt, .. }) = out.ik
                && attempt > 0
            {
                out.warnings.push(MotionWarning::IkRestartUsed { attempt });
            }
            if let Some(jump) = span.max_joint_jump
                && jump > options.large_joint_jump
            {
                out.warnings.push(MotionWarning::LargeJointJump { jump });
            }
            let Some(r) = span.robot else { return out };
            if span.samples.is_empty() {
                return out;
            }
            let model = &robots[r];
            let joints = model.active_joints();
            // The motion's samples, and the one at which it arrives.
            let last = span.samples.end.min(trajectory.samples.len() - 1);
            let states = (span.samples.start..=last).map(|k| &trajectory.samples[k].robots[r]);

            let mut peak = vec![[0.0_f64; 3]; joints.len()];
            let mut margin: Option<(f64, usize)> = None;
            let mut sigma = f64::INFINITY;
            for st in states {
                for (i, j) in joints.iter().enumerate() {
                    peak[i][0] = peak[i][0].max(st.qd[i].abs() / j.max_velocity);
                    peak[i][1] = peak[i][1].max(st.qdd[i].abs() / j.max_acceleration);
                    peak[i][2] = peak[i][2].max(st.qddd[i].abs() / j.max_jerk);
                    let m = (st.q[i] - j.lower).min(j.upper - st.q[i]);
                    if margin.is_none_or(|(best, _)| m < best) {
                        margin = Some((m, i));
                    }
                }
                let (_, jacobian) = model.tcp_jacobian(&st.q);
                let s = jacobian.singular_values().min();
                sigma = sigma.min(s);
            }
            let mut binding: Option<Binding> = None;
            for (i, j) in joints.iter().enumerate() {
                let finite_jerk = j.max_jerk.is_finite();
                out.peaks.push(JointPeak {
                    joint: j.name.clone(),
                    velocity: peak[i][0],
                    acceleration: peak[i][1],
                    jerk: finite_jerk.then_some(peak[i][2]),
                });
                for (quantity, fraction) in [
                    (Quantity::Velocity, peak[i][0]),
                    (Quantity::Acceleration, peak[i][1]),
                    (Quantity::Jerk, if finite_jerk { peak[i][2] } else { 0.0 }),
                ] {
                    if binding.as_ref().is_none_or(|b| fraction > b.fraction) {
                        binding = Some(Binding {
                            joint: j.name.clone(),
                            quantity,
                            fraction,
                        });
                    }
                }
            }
            out.binding = binding;
            if let Some((m, i)) = margin {
                out.min_limit_margin = Some(m);
                if m < options.near_limit {
                    out.warnings.push(MotionWarning::NearLimit {
                        joint: joints[i].name.clone(),
                        margin: m,
                    });
                }
            }
            out.min_sigma = Some(sigma);
            if sigma < options.near_singular {
                out.warnings.push(MotionWarning::NearSingular { sigma });
            }
            out
        })
        .collect();
    MotionReport {
        version: 1,
        duration_s: (trajectory.samples.len().saturating_sub(1)) as f64 * dt,
        captures: trajectory
            .samples
            .iter()
            .filter(|s| s.capture.is_some())
            .count(),
        steps,
    }
}
