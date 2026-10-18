//! Scenario compiler (P1-5, P7-2) and baker (ADR 0003).
//!
//! [`compile`] turns a [`ScenarioSpec`] into a [`Trajectory`]: joint
//! positions, velocities, accelerations and jerks of every robot at
//! `t = k · dt`, plus what each step did. [`bake`] adds forward kinematics
//! and the scene frame tree to produce the [`BakedScenario`] every other
//! consumer reads.
//!
//! # Timing model
//!
//! Steps run sequentially; one robot moves at a time and every motion is
//! rest-to-rest (zero velocity and acceleration at both ends).
//!
//! - `ptp_joints` / `ptp_pose`: straight line in joint space,
//!   `q(t) = q₀ + s(t) Δq`, where the path parameter `s` follows a
//!   time-optimal jerk-limited profile (Ruckig) with
//!   `|ṡ| ≤ min_i v_i / |Δq_i|` and likewise for acceleration and jerk. Every
//!   joint starts and stops together (phase synchronisation), and none
//!   exceeds `speed_scale ×` its velocity, acceleration or jerk limit. A
//!   `ptp_pose` with a stored `q` goes there (checked to reach the pose);
//!   otherwise IK from the current configuration chooses it.
//! - `lin`: the TCP follows the straight line (translation) / slerp
//!   (rotation) between the poses. Waypoints every ≤ 1 mm and ≤ 0.5° are
//!   solved by IK (each seeded from the previous one), joined by a natural
//!   cubic spline per joint, and traversed with a jerk-limited profile whose
//!   limits come from the spline's exact derivative bounds `d1, d2, d3`:
//!   `ṡ ≤ min(v/d1, √(a/2d2), ∛(J/3d3))`, `s̈ ≤ min(a/2d1, J/(9 d2 ṡmax))`,
//!   `s⃛ ≤ J/3d1`, so that every joint's velocity, acceleration
//!   (`q'' ṡ² + q' s̈`) and jerk (`q''' ṡ³ + 3 q'' ṡ s̈ + q' s⃛`) stay within its
//!   limits. The spline's exact position range is checked against the joint
//!   limits.
//! - Each motion's duration is rounded **up** to a whole number of samples
//!   (a profile of that duration, within the same limits), so every step
//!   boundary is a sample.
//! - `capture` occupies one stationary sample (the robot is at rest there)
//!   and advances the clock by `dt`; `wait` holds for `⌈duration / dt⌉`
//!   samples.
//! - A final sample holds the end state.

use std::ops::Range;

use etendue_scene::{
    BakedRobot, BakedSample, BakedScenario, CaptureEvent, FrameGraph, IkReport, ScenarioSpec,
    SceneSpec, Step, default_capture_id,
};
use nalgebra::Isometry3;

use crate::error::{Error, Result};
use crate::ik::IkOptions;
use crate::trajectory::{PathProfile, Spline};
use crate::urdf::RobotModel;

/// Largest translation between `lin` waypoints, metres.
pub const LIN_MAX_STEP_M: f64 = 1e-3;
/// Largest rotation between `lin` waypoints, radians (0.5°).
pub const LIN_MAX_STEP_RAD: f64 = 0.5 * std::f64::consts::PI / 180.0;
/// Largest joint change between consecutive `lin` waypoints (∞-norm) before
/// the path is rejected as crossing a singularity or an IK branch switch.
pub const LIN_MAX_JOINT_JUMP: f64 = 0.2;
/// How closely a stored `ptp_pose` configuration must reach its pose:
/// metres and radians.
pub const STORED_Q_TOLERANCE: f64 = 1e-6;

/// One robot's state at a sample.
#[derive(Clone, Debug, PartialEq)]
pub struct JointState {
    /// Positions.
    pub q: Vec<f64>,
    /// Velocities.
    pub qd: Vec<f64>,
    /// Accelerations.
    pub qdd: Vec<f64>,
    /// Jerks.
    pub qddd: Vec<f64>,
}

impl JointState {
    fn at_rest(q: Vec<f64>) -> Self {
        let n = q.len();
        Self {
            q,
            qd: vec![0.0; n],
            qdd: vec![0.0; n],
            qddd: vec![0.0; n],
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

/// What one scenario step did.
#[derive(Clone, Debug)]
pub struct StepSpan {
    /// Index into the scenario's steps.
    pub step: usize,
    /// The robot it moved (scene index), for a motion.
    pub robot: Option<usize>,
    /// Its samples: a motion's samples end before the sample at which the
    /// robot arrives (that one belongs to the next step).
    pub samples: Range<usize>,
    /// For a `ptp_pose`: how its configuration was found, and the
    /// configuration.
    pub ik: Option<(IkReport, Vec<f64>)>,
    /// For a `lin`: the largest joint change between waypoints.
    pub max_joint_jump: Option<f64>,
}

/// A compiled scenario: joint states of every robot at `t = k · dt`.
#[derive(Clone, Debug)]
pub struct Trajectory {
    /// Sample period, seconds.
    pub dt: f64,
    /// Samples; sample `k` is at `t = k · dt`.
    pub samples: Vec<TrajectorySample>,
    /// One entry per scenario step, in order.
    pub steps: Vec<StepSpan>,
}

enum Motion {
    Ptp { q0: Vec<f64>, dq: Vec<f64> },
    Lin { splines: Vec<Spline> },
}

impl Motion {
    fn eval(&self, [s, sd, sdd, sddd]: [f64; 4]) -> JointState {
        match self {
            Motion::Ptp { q0, dq } => JointState {
                q: q0.iter().zip(dq).map(|(a, d)| a + d * s).collect(),
                qd: dq.iter().map(|d| d * sd).collect(),
                qdd: dq.iter().map(|d| d * sdd).collect(),
                qddd: dq.iter().map(|d| d * sddd).collect(),
            },
            Motion::Lin { splines } => {
                let n = splines.len();
                let mut st = JointState {
                    q: Vec::with_capacity(n),
                    qd: Vec::with_capacity(n),
                    qdd: Vec::with_capacity(n),
                    qddd: Vec::with_capacity(n),
                };
                for sp in splines {
                    let (y, yd, ydd) = sp.eval(s);
                    let yddd = sp.third(s);
                    st.q.push(y);
                    st.qd.push(yd * sd);
                    st.qdd.push(ydd * sd * sd + yd * sdd);
                    st.qddd
                        .push(yddd * sd * sd * sd + 3.0 * ydd * sd * sdd + yd * sddd);
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

    fn run(
        &mut self,
        robot: usize,
        motion: &Motion,
        profile: &PathProfile,
        end: Vec<f64>,
        step: usize,
    ) -> Result<()> {
        let n = sample_count(profile.duration(), self.dt);
        let profile = profile
            .stretched(n as f64 * self.dt)
            .map_err(|message| Error::Scenario { step, message })?;
        for i in 0..n {
            let mut robots = self.rest();
            robots[robot] = motion.eval(profile.eval(i as f64 * self.dt));
            self.samples.push(TrajectorySample {
                robots,
                capture: None,
            });
        }
        self.state[robot] = end;
        Ok(())
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
fn ptp(
    model: &RobotModel,
    q0: &[f64],
    q1: &[f64],
    speed: f64,
    step: usize,
) -> Result<Option<(Motion, PathProfile)>> {
    let dq: Vec<f64> = q1.iter().zip(q0).map(|(a, b)| a - b).collect();
    let mut limits = [f64::INFINITY; 3];
    for (d, j) in dq.iter().zip(model.active_joints()) {
        if *d != 0.0 {
            let joint = [j.max_velocity, j.max_acceleration, j.max_jerk];
            for (limit, l) in limits.iter_mut().zip(joint) {
                *limit = limit.min(speed * l / d.abs());
            }
        }
    }
    if limits[0].is_infinite() {
        return Ok(None);
    }
    let [v, a, j] = limits;
    let profile =
        PathProfile::fastest(v, a, j).map_err(|message| Error::Scenario { step, message })?;
    Ok(Some((
        Motion::Ptp {
            q0: q0.to_vec(),
            dq,
        },
        profile,
    )))
}

/// A `lin` move: the motion, its profile, the end configuration and the
/// largest joint change between waypoints.
type LinMove = (Motion, PathProfile, Vec<f64>, f64);

/// Cartesian straight-line move.
fn lin(
    model: &RobotModel,
    q0: &[f64],
    target: &Isometry3<f64>,
    speed: f64,
    step: usize,
) -> Result<Option<LinMove>> {
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
    let mut max_jump = 0.0_f64;
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
        max_jump = max_jump.max(jump);
        nodes.push(sol.q);
    }
    let dof = model.dof();
    let splines: Vec<Spline> = (0..dof)
        .map(|j| Spline::natural(nodes.iter().map(|q| q[j]).collect()))
        .collect();
    // Velocity and acceleration bounds of `s` first; the acceleration bound
    // from jerk needs the final speed bound.
    let mut bounds = Vec::with_capacity(dof);
    let mut sd_max = f64::INFINITY;
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
        let d3 = sp.third_bound();
        let (v, a, j) = (
            speed * joint.max_velocity,
            speed * joint.max_acceleration,
            speed * joint.max_jerk,
        );
        if d1 > 0.0 {
            sd_max = sd_max.min(v / d1);
        }
        if d2 > 0.0 {
            sd_max = sd_max.min((0.5 * a / d2).sqrt());
        }
        if d3 > 0.0 {
            sd_max = sd_max.min((j / (3.0 * d3)).cbrt());
        }
        bounds.push((d1, d2, a, j));
    }
    let end = nodes.pop().expect("n ≥ 4 nodes");
    if sd_max.is_infinite() {
        return Ok(None);
    }
    let (mut sdd_max, mut sddd_max) = (f64::INFINITY, f64::INFINITY);
    for (d1, d2, a, j) in bounds {
        if d1 > 0.0 {
            sdd_max = sdd_max.min(0.5 * a / d1);
            sddd_max = sddd_max.min(j / (3.0 * d1));
        }
        if d2 > 0.0 {
            sdd_max = sdd_max.min(j / (9.0 * d2 * sd_max));
        }
    }
    let profile = PathProfile::fastest(sd_max, sdd_max, sddd_max)
        .map_err(|message| Error::Scenario { step, message })?;
    Ok(Some((Motion::Lin { splines }, profile, end, max_jump)))
}

/// The configuration a `ptp_pose` goes to: its stored `q` (checked), or IK
/// from `current`.
fn ptp_pose_target(
    model: &RobotModel,
    current: &[f64],
    base_se3_tool: &Isometry3<f64>,
    stored: Option<&Vec<f64>>,
    step: usize,
) -> Result<(IkReport, Vec<f64>)> {
    let scenario = |message: String| Error::Scenario { step, message };
    if let Some(q) = stored {
        model.check_q(q, 0.0).map_err(|e| scenario(e.to_string()))?;
        let reached = model.tcp_pose(q);
        let dt = (reached.translation.vector - base_se3_tool.translation.vector).norm();
        let dr = reached.rotation.angle_to(&base_se3_tool.rotation);
        if dt > STORED_Q_TOLERANCE || dr > STORED_Q_TOLERANCE {
            return Err(scenario(format!(
                "the stored q reaches {dt:.3e} m / {dr:.3e} rad from base_se3_tool; resolve the \
                 scenario again (or remove q)"
            )));
        }
        return Ok((IkReport::Stored, q.clone()));
    }
    let sol = model
        .ik(base_se3_tool, current, &IkOptions::default())
        .map_err(|e| scenario(e.to_string()))?;
    Ok((
        IkReport::Solved {
            attempt: sol.attempt,
            iterations: sol.iterations,
        },
        sol.q,
    ))
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
///   target outside the joint limits, unreachable pose, a stored `q` that
///   does not reach its pose, `lin` through a singularity).
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
    let mut spans = Vec::with_capacity(scenario.steps.len());

    for (step, spec) in scenario.steps.iter().enumerate() {
        let k0 = b.samples.len();
        let mut span = StepSpan {
            step,
            robot: None,
            samples: k0..k0,
            ik: None,
            max_joint_jump: None,
        };
        match spec {
            Step::PtpJoints {
                robot,
                q,
                speed_scale,
            } => {
                let r = robot_index(scene, robot, step)?;
                span.robot = Some(r);
                let model = &b.robots[r];
                model.check_q(q, 0.0).map_err(|e| Error::Scenario {
                    step,
                    message: e.to_string(),
                })?;
                if let Some((motion, profile)) = ptp(model, &b.state[r], q, *speed_scale, step)? {
                    b.run(r, &motion, &profile, q.clone(), step)?;
                }
            }
            Step::PtpPose {
                robot,
                base_se3_tool,
                speed_scale,
                q,
            } => {
                let r = robot_index(scene, robot, step)?;
                span.robot = Some(r);
                let model = &b.robots[r];
                let (ik, target) =
                    ptp_pose_target(model, &b.state[r], base_se3_tool, q.as_ref(), step)?;
                if let Some((motion, profile)) =
                    ptp(model, &b.state[r], &target, *speed_scale, step)?
                {
                    b.run(r, &motion, &profile, target.clone(), step)?;
                }
                span.ik = Some((ik, target));
            }
            Step::Lin {
                robot,
                base_se3_tool,
                speed_scale,
            } => {
                let r = robot_index(scene, robot, step)?;
                span.robot = Some(r);
                let model = &b.robots[r];
                if let Some((motion, profile, end, jump)) =
                    lin(model, &b.state[r], base_se3_tool, *speed_scale, step)?
                {
                    b.run(r, &motion, &profile, end, step)?;
                    span.max_joint_jump = Some(jump);
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
        span.samples = k0..b.samples.len();
        spans.push(span);
    }
    b.hold(1, None);
    Ok(Trajectory {
        dt: scenario.dt,
        samples: b.samples,
        steps: spans,
    })
}

/// `scenario` with every `ptp_pose` given the configuration it compiles to
/// (`q`), so that baking it again can never choose another IK branch.
///
/// # Errors
///
/// As [`compile`].
pub fn resolve(
    scene: &SceneSpec,
    scenario: &ScenarioSpec,
    robots: &[RobotModel],
) -> Result<ScenarioSpec> {
    let trajectory = compile(scene, scenario, robots)?;
    let mut resolved = scenario.clone();
    for (spec, span) in resolved.steps.iter_mut().zip(&trajectory.steps) {
        if let (Step::PtpPose { q, .. }, Some((_, target))) = (spec, &span.ik) {
            *q = Some(target.clone());
        }
    }
    Ok(resolved)
}

/// What a bake records besides the poses.
#[derive(Clone, Copy, Debug, Default)]
pub struct BakeOptions {
    /// Record joint velocities and accelerations in every sample.
    pub derivatives: bool,
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
    bake_with(scene, scenario, robots, BakeOptions::default())
}

/// [`bake`], with options.
///
/// # Errors
///
/// As [`bake`].
pub fn bake_with(
    scene: &SceneSpec,
    scenario: &ScenarioSpec,
    robots: &[RobotModel],
    options: BakeOptions,
) -> Result<BakedScenario> {
    let trajectory = compile(scene, scenario, robots)?;
    bake_trajectory(scene, &trajectory, robots, options)
}

/// Bake a compiled trajectory (see [`bake_with`]).
///
/// # Errors
///
/// [`Error::Validation`] if a parent frame names a link the robot model does
/// not have.
pub fn bake_trajectory(
    scene: &SceneSpec,
    trajectory: &Trajectory,
    robots: &[RobotModel],
    options: BakeOptions,
) -> Result<BakedScenario> {
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
            let per_robot = |f: fn(&JointState) -> &Vec<f64>| {
                options
                    .derivatives
                    .then(|| sample.robots.iter().map(|st| f(st).clone()).collect())
            };
            BakedSample {
                t: k as f64 * trajectory.dt,
                world_se3_frame: graph.resolve(|r, l| poses[r][l]),
                joint_positions: sample.robots.iter().map(|st| st.q.clone()).collect(),
                joint_velocities: per_robot(|st| &st.qd),
                joint_accelerations: per_robot(|st| &st.qdd),
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
