//! Scenario compiler (P1-5) and baker (ADR 0003).
//!
//! [`compile`] turns a [`ScenarioSpec`] into a [`Trajectory`]: joint
//! positions, velocities, and accelerations of every robot at `t = k · dt`.
//! [`bake`] adds forward kinematics and the scene frame tree to produce the
//! [`BakedScenario`] every other consumer reads.
//!
//! # Timing model
//!
//! Steps run sequentially; one robot moves at a time and every motion is
//! rest-to-rest.
//!
//! - `ptp_joints` / `ptp_pose`: straight line in joint space with
//!   synchronised trapezoidal profiles — every joint starts and stops
//!   together, and no joint exceeds `speed_scale ×` its velocity or
//!   acceleration limit.
//! - `lin`: the TCP follows the straight line (translation) / slerp
//!   (rotation) between the poses. Waypoints every ≤ 1 mm and ≤ 0.5° are
//!   solved by IK (each seeded from the previous one), joined by a natural
//!   cubic spline per joint, and traversed with a trapezoidal profile whose
//!   limits come from the spline's exact derivative bounds, so that
//!   `|q'| ṡ ≤ v` and `|q'| s̈ + |q''| ṡ² ≤ a` hold for every joint. The
//!   spline's exact position range is checked against the joint limits.
//! - Each motion's duration is rounded **up** to a whole number of samples
//!   by slowing it down uniformly, so every step boundary is a sample.
//! - `capture` occupies one stationary sample (the robot is at rest there)
//!   and advances the clock by `dt`; `wait` holds for `⌈duration / dt⌉`
//!   samples.
//! - A final sample holds the end state.

use etendue_scene::{
    BakedRobot, BakedSample, BakedScenario, CaptureEvent, FrameGraph, ScenarioSpec, SceneSpec,
    Step, default_capture_id,
};
use nalgebra::Isometry3;

use crate::error::{Error, Result};
use crate::ik::IkOptions;
use crate::trajectory::{Spline, Trapezoid};
use crate::urdf::RobotModel;

/// Largest translation between `lin` waypoints, metres.
pub const LIN_MAX_STEP_M: f64 = 1e-3;
/// Largest rotation between `lin` waypoints, radians (0.5°).
pub const LIN_MAX_STEP_RAD: f64 = 0.5 * std::f64::consts::PI / 180.0;
/// Largest joint change between consecutive `lin` waypoints (∞-norm) before
/// the path is rejected as crossing a singularity or an IK branch switch.
pub const LIN_MAX_JOINT_JUMP: f64 = 0.2;

/// One robot's state at a sample.
#[derive(Clone, Debug, PartialEq)]
pub struct JointState {
    /// Positions.
    pub q: Vec<f64>,
    /// Velocities.
    pub qd: Vec<f64>,
    /// Accelerations.
    pub qdd: Vec<f64>,
}

impl JointState {
    fn at_rest(q: Vec<f64>) -> Self {
        let n = q.len();
        Self {
            q,
            qd: vec![0.0; n],
            qdd: vec![0.0; n],
        }
    }
}

/// One sample of a compiled trajectory.
#[derive(Clone, Debug)]
pub struct TrajectorySample {
    /// Per robot (scene order).
    pub robots: Vec<JointState>,
    /// Capture id, on stationary capture samples.
    pub capture: Option<String>,
}

/// A compiled scenario: joint states of every robot at `t = k · dt`.
#[derive(Clone, Debug)]
pub struct Trajectory {
    /// Sample period, seconds.
    pub dt: f64,
    /// Samples; sample `k` is at `t = k · dt`.
    pub samples: Vec<TrajectorySample>,
}

enum Motion {
    Ptp { q0: Vec<f64>, dq: Vec<f64> },
    Lin { splines: Vec<Spline> },
}

impl Motion {
    fn eval(&self, s: f64, sd: f64, sdd: f64) -> JointState {
        match self {
            Motion::Ptp { q0, dq } => JointState {
                q: q0.iter().zip(dq).map(|(a, d)| a + d * s).collect(),
                qd: dq.iter().map(|d| d * sd).collect(),
                qdd: dq.iter().map(|d| d * sdd).collect(),
            },
            Motion::Lin { splines } => {
                let mut st = JointState::at_rest(Vec::with_capacity(splines.len()));
                st.qd.clear();
                st.qdd.clear();
                for sp in splines {
                    let (y, yd, ydd) = sp.eval(s);
                    st.q.push(y);
                    st.qd.push(yd * sd);
                    st.qdd.push(ydd * sd * sd + yd * sdd);
                }
                st
            }
        }
    }
}

/// Number of samples a motion of natural duration `t` occupies.
fn sample_count(t: f64, dt: f64) -> usize {
    // Tolerate round-off so an exact multiple of dt is not bumped up.
    ((t / dt) * (1.0 - 1e-12)).ceil().max(1.0) as usize
}

struct Builder<'a> {
    dt: f64,
    robots: &'a [RobotModel],
    state: Vec<Vec<f64>>,
    samples: Vec<TrajectorySample>,
}

impl Builder<'_> {
    fn rest(&self) -> Vec<JointState> {
        self.state
            .iter()
            .cloned()
            .map(JointState::at_rest)
            .collect()
    }

    fn hold(&mut self, n: usize, capture: Option<String>) {
        for i in 0..n {
            self.samples.push(TrajectorySample {
                robots: self.rest(),
                capture: if i == 0 { capture.clone() } else { None },
            });
        }
    }

    fn run(&mut self, robot: usize, motion: &Motion, profile: Trapezoid, end: Vec<f64>) {
        let n = sample_count(profile.natural_duration(), self.dt);
        let profile = profile.stretched(n as f64 * self.dt);
        for i in 0..n {
            let (s, sd, sdd) = profile.eval(i as f64 * self.dt);
            let mut robots = self.rest();
            robots[robot] = motion.eval(s, sd, sdd);
            self.samples.push(TrajectorySample {
                robots,
                capture: None,
            });
        }
        self.state[robot] = end;
    }
}

fn robot_index(scene: &SceneSpec, id: &str, step: usize) -> Result<usize> {
    scene
        .robots
        .iter()
        .position(|r| r.id == id)
        .ok_or_else(|| Error::Scenario {
            step,
            message: format!("unknown robot `{id}`"),
        })
}

/// Joint-space PTP from `q0` to `q1`, or `None` if they coincide.
fn ptp(model: &RobotModel, q0: &[f64], q1: &[f64], speed: f64) -> Option<(Motion, Trapezoid)> {
    let dq: Vec<f64> = q1.iter().zip(q0).map(|(a, b)| a - b).collect();
    let mut v_max = f64::INFINITY;
    let mut a_max = f64::INFINITY;
    for (d, j) in dq.iter().zip(model.active_joints()) {
        if *d != 0.0 {
            v_max = v_max.min(speed * j.max_velocity / d.abs());
            a_max = a_max.min(speed * j.max_acceleration / d.abs());
        }
    }
    if v_max.is_infinite() {
        return None;
    }
    Some((
        Motion::Ptp {
            q0: q0.to_vec(),
            dq,
        },
        Trapezoid::fastest(v_max, a_max),
    ))
}

/// Cartesian straight-line move; returns the motion, its profile, and the
/// end configuration.
fn lin(
    model: &RobotModel,
    q0: &[f64],
    target: &Isometry3<f64>,
    speed: f64,
    step: usize,
) -> Result<Option<(Motion, Trapezoid, Vec<f64>)>> {
    let start = model.tcp_pose(q0);
    let distance = (target.translation.vector - start.translation.vector).norm();
    let angle = start.rotation.angle_to(&target.rotation);
    if distance == 0.0 && angle == 0.0 {
        return Ok(None);
    }
    let n = ((distance / LIN_MAX_STEP_M)
        .max(angle / LIN_MAX_STEP_RAD)
        .ceil() as usize)
        .max(4);
    // Strictly local IK: each waypoint continues the previous one's branch.
    let opts = IkOptions {
        restarts: 0,
        ..IkOptions::default()
    };
    let mut nodes: Vec<Vec<f64>> = Vec::with_capacity(n + 1);
    nodes.push(q0.to_vec());
    for k in 1..=n {
        let s = k as f64 / n as f64;
        let pose = Isometry3::from_parts(
            (start.translation.vector.lerp(&target.translation.vector, s)).into(),
            start.rotation.slerp(&target.rotation, s),
        );
        let prev = nodes.last().expect("seeded with q0");
        let sol = model.ik(&pose, prev, &opts).map_err(|e| Error::Scenario {
            step,
            message: format!("lin waypoint {k}/{n} unreachable: {e}"),
        })?;
        let jump = sol
            .q
            .iter()
            .zip(prev)
            .fold(0.0_f64, |acc, (a, b)| acc.max((a - b).abs()));
        if jump > LIN_MAX_JOINT_JUMP {
            return Err(Error::Scenario {
                step,
                message: format!(
                    "lin path jumps {jump:.3} rad between waypoints {} and {k} (singularity or \
                     IK branch switch); split the move or use ptp_pose",
                    k - 1
                ),
            });
        }
        nodes.push(sol.q);
    }
    let dof = model.dof();
    let splines: Vec<Spline> = (0..dof)
        .map(|j| Spline::natural(nodes.iter().map(|q| q[j]).collect()))
        .collect();
    let mut sd_max = f64::INFINITY;
    let mut sdd_max = f64::INFINITY;
    for (sp, joint) in splines.iter().zip(model.active_joints()) {
        let (lo, hi) = sp.range();
        if lo < joint.lower || hi > joint.upper {
            return Err(Error::Scenario {
                step,
                message: format!(
                    "lin path takes joint `{}` to [{lo}, {hi}], outside [{}, {}]",
                    joint.name, joint.lower, joint.upper
                ),
            });
        }
        let (d1, d2) = sp.derivative_bounds();
        let (v, a) = (speed * joint.max_velocity, speed * joint.max_acceleration);
        if d1 > 0.0 {
            sd_max = sd_max.min(v / d1);
            sdd_max = sdd_max.min(0.5 * a / d1);
        }
        if d2 > 0.0 {
            sd_max = sd_max.min((0.5 * a / d2).sqrt());
        }
    }
    let end = nodes.pop().expect("n ≥ 4 nodes");
    if sd_max.is_infinite() {
        return Ok(None);
    }
    Ok(Some((
        Motion::Lin { splines },
        Trapezoid::fastest(sd_max, sdd_max),
        end,
    )))
}

/// Compile `scenario` for `scene`. `robots[i]` is the model of
/// `scene.robots[i]`.
///
/// # Errors
///
/// - [`Error::Validation`] if the scene or scenario fails validation;
/// - [`Error::InvalidModel`] if `robots` is not aligned with `scene.robots`;
/// - [`Error::InvalidJoints`] if an initial configuration is invalid;
/// - [`Error::Scenario`] if a step cannot be executed (wrong joint count,
///   target outside the joint limits, unreachable pose, `lin` through a
///   singularity).
pub fn compile(
    scene: &SceneSpec,
    scenario: &ScenarioSpec,
    robots: &[RobotModel],
) -> Result<Trajectory> {
    scene.validate()?;
    scenario.validate(scene)?;
    if robots.len() != scene.robots.len() {
        return Err(Error::InvalidModel(format!(
            "{} robot models for {} scene robots",
            robots.len(),
            scene.robots.len()
        )));
    }
    let mut state = Vec::with_capacity(robots.len());
    for (spec, model) in scene.robots.iter().zip(robots) {
        let q = spec
            .initial_q
            .clone()
            .unwrap_or_else(|| vec![0.0; model.dof()]);
        model
            .check_q(&q, 0.0)
            .map_err(|e| Error::InvalidJoints(format!("robot `{}` initial_q: {e}", spec.id)))?;
        state.push(q);
    }
    let mut b = Builder {
        dt: scenario.dt,
        robots,
        state,
        samples: Vec::new(),
    };
    let mut n_captures = 0usize;

    for (step, spec) in scenario.steps.iter().enumerate() {
        match spec {
            Step::PtpJoints {
                robot,
                q,
                speed_scale,
            } => {
                let r = robot_index(scene, robot, step)?;
                let model = &b.robots[r];
                model.check_q(q, 0.0).map_err(|e| Error::Scenario {
                    step,
                    message: e.to_string(),
                })?;
                if let Some((motion, profile)) = ptp(model, &b.state[r], q, *speed_scale) {
                    b.run(r, &motion, profile, q.clone());
                }
            }
            Step::PtpPose {
                robot,
                base_se3_tool,
                speed_scale,
            } => {
                let r = robot_index(scene, robot, step)?;
                let model = &b.robots[r];
                let sol = model
                    .ik(base_se3_tool, &b.state[r], &IkOptions::default())
                    .map_err(|e| Error::Scenario {
                        step,
                        message: e.to_string(),
                    })?;
                if let Some((motion, profile)) = ptp(model, &b.state[r], &sol.q, *speed_scale) {
                    b.run(r, &motion, profile, sol.q);
                }
            }
            Step::Lin {
                robot,
                base_se3_tool,
                speed_scale,
            } => {
                let r = robot_index(scene, robot, step)?;
                let model = &b.robots[r];
                if let Some((motion, profile, end)) =
                    lin(model, &b.state[r], base_se3_tool, *speed_scale, step)?
                {
                    b.run(r, &motion, profile, end);
                }
            }
            Step::Capture { id } => {
                let id = id.clone().unwrap_or_else(|| default_capture_id(n_captures));
                n_captures += 1;
                b.hold(1, Some(id));
            }
            Step::Wait { duration_s } => {
                if *duration_s > 0.0 {
                    b.hold(sample_count(*duration_s, b.dt), None);
                }
            }
        }
    }
    b.hold(1, None);
    Ok(Trajectory {
        dt: scenario.dt,
        samples: b.samples,
    })
}

/// Compile and bake: `world_se3_frame` for every scene frame at every
/// sample, plus joint positions and capture flags.
///
/// # Errors
///
/// As [`compile`], plus [`Error::Validation`] if a parent frame names a link
/// the robot model does not have.
pub fn bake(
    scene: &SceneSpec,
    scenario: &ScenarioSpec,
    robots: &[RobotModel],
) -> Result<BakedScenario> {
    let trajectory = compile(scene, scenario, robots)?;
    let links: Vec<Vec<String>> = robots.iter().map(|m| m.links().to_vec()).collect();
    let graph = FrameGraph::build(scene, &links)?;
    let samples = trajectory
        .samples
        .iter()
        .enumerate()
        .map(|(k, sample)| {
            let poses: Vec<Vec<Isometry3<f64>>> = robots
                .iter()
                .zip(&sample.robots)
                .map(|(m, st)| m.link_poses(&st.q))
                .collect();
            BakedSample {
                t: k as f64 * trajectory.dt,
                world_se3_frame: graph.resolve(|r, l| poses[r][l]),
                joint_positions: sample.robots.iter().map(|st| st.q.clone()).collect(),
                capture: sample.capture.clone().map(|id| CaptureEvent { id }),
            }
        })
        .collect();
    Ok(BakedScenario {
        version: 1,
        dt: trajectory.dt,
        frames: graph.names().map(str::to_owned).collect(),
        robots: scene
            .robots
            .iter()
            .zip(robots)
            .map(|(spec, m)| BakedRobot {
                id: spec.id.clone(),
                joint_names: m.active_joints().iter().map(|j| j.name.clone()).collect(),
            })
            .collect(),
        samples,
    })
}
