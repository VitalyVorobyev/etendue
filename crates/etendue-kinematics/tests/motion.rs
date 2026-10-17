//! Gate G1.4 (P7-2): compiled motion is feasible on every library robot.
//!
//! On random programs of PTP and LIN moves (random speed scales and sample
//! periods) for each robot of `assets/robots/` (and the local-only
//! `assets/robots-local/`, where built):
//!
//! - the analytic joint velocity, acceleration and jerk stay within
//!   `speed_scale ×` the limits (relative tolerance 1e-9), and the finite
//!   differences of the sampled positions agree;
//! - captures happen at rest: zero velocity, acceleration and jerk, and the
//!   robot does not move during the capture sample;
//! - PTP moves are straight in joint space (phase-synchronised), and their
//!   duration matches an n-DoF Ruckig solution with phase synchronisation to
//!   within one sample.
//!
//! Plus: a stored `ptp_pose` configuration is used as is and checked,
//! `resolve` pins every IK solution, and the motion report agrees.

mod common;

use std::path::{Path, PathBuf};

use common::{Rng, load};
use etendue_kinematics::{
    Error, ReportOptions, RobotModel, StepSpan, Trajectory, compile, report, resolve,
};
use etendue_scene::{IkReport, Quantity, ScenarioSpec, SceneSpec, Step};
use nalgebra::{Isometry3, Translation3, UnitQuaternion};
use rsruckig::prelude::{InputParameter, Ruckig, Synchronization, ThrowErrorHandler};
use serde_json::json;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every library robot: `(id, manifest path)`.
fn library() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for dir in ["assets/robots", "assets/robots-local"] {
        let Ok(entries) = std::fs::read_dir(repo_root().join(dir)) else {
            continue;
        };
        let mut found: Vec<PathBuf> = entries
            .map(|e| e.unwrap().path().join("robot.json"))
            .filter(|p| p.is_file())
            .collect();
        found.sort();
        out.extend(found.into_iter().map(|p| {
            let id = p
                .parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned();
            (id, p)
        }));
    }
    assert!(
        out.len() >= 3,
        "expected the robot library in assets/robots"
    );
    out
}

fn scene(id: &str, q0: &[f64]) -> SceneSpec {
    serde_json::from_value(json!({
        "version": 1,
        "robots": [{"id": id, "parent": "world",
                    "parent_se3_self": {"rotation": [0.0, 0.0, 0.0, 1.0], "translation": [0.0, 0.0, 0.0]},
                    "manifest": "robot.json", "initial_q": q0}]
    }))
    .unwrap()
}

/// A random start configuration and program: PTPs to random configurations
/// and to the poses of others, short LINs in the tool frame, captures in
/// between.
/// `None` when a LIN on the way cannot be compiled (through a singularity,
/// out of reach): a reported error, not a limit violation.
fn program(
    model: &RobotModel,
    id: &str,
    rng: &mut Rng,
    dt: f64,
) -> Option<(Vec<f64>, ScenarioSpec)> {
    let q0 = rng.q(model, 0.4);
    let mut steps = Vec::new();
    for k in 0..12 {
        let speed = rng.uniform(0.1, 1.0);
        match k % 3 {
            0 => steps.push(
                json!({"type": "ptp_joints", "robot": id, "q": rng.q(model, 0.4),
                                   "speed_scale": speed}),
            ),
            1 => {
                let pose = model.tcp_pose(&rng.q(model, 0.4));
                steps.push(json!({"type": "ptp_pose", "robot": id,
                                  "base_se3_tool": serde_json::to_value(pose).unwrap(),
                                  "speed_scale": speed}));
            }
            _ => {
                // A LIN from the configuration the previous step reached.
                let offset = Isometry3::from_parts(
                    Translation3::new(
                        rng.uniform(-0.03, 0.03),
                        rng.uniform(-0.03, 0.03),
                        rng.uniform(-0.03, 0.03),
                    ),
                    UnitQuaternion::from_euler_angles(
                        rng.uniform(-0.08, 0.08),
                        rng.uniform(-0.08, 0.08),
                        rng.uniform(-0.08, 0.08),
                    ),
                );
                // The offset for now; made absolute below.
                steps.push(json!({"type": "lin", "robot": id,
                                  "base_se3_tool": serde_json::to_value(offset).unwrap(),
                                  "speed_scale": speed}));
            }
        }
        steps.push(json!({"type": "capture"}));
    }
    let mut scenario: ScenarioSpec =
        serde_json::from_value(json!({"version": 1, "dt": dt, "steps": steps})).unwrap();
    // A LIN's target is relative to where the robot is: fill it in by
    // compiling the program up to it.
    let initial = scene(id, &q0);
    for i in 0..scenario.steps.len() {
        if let Step::Lin { base_se3_tool, .. } = &scenario.steps[i] {
            let offset = *base_se3_tool;
            let head = ScenarioSpec {
                steps: scenario.steps[..i].to_vec(),
                ..scenario.clone()
            };
            let traj = compile(&initial, &head, std::slice::from_ref(model)).ok()?;
            let q = &traj.samples.last().unwrap().robots[0].q;
            if let Step::Lin { base_se3_tool, .. } = &mut scenario.steps[i] {
                *base_se3_tool = model.tcp_pose(q) * offset;
            }
        }
    }
    Some((q0, scenario))
}

#[derive(Default)]
struct Stats {
    /// Largest analytic velocity, acceleration and jerk over limit.
    worst: [f64; 3],
    /// PTP moves whose duration was checked equal to Ruckig's.
    phase_checked: usize,
}

/// Check one robot's trajectory.
fn check(model: &RobotModel, scenario: &ScenarioSpec, traj: &Trajectory, stats: &mut Stats) {
    let dt = traj.dt;
    let joints = model.active_joints();
    let speed_of = |span: &StepSpan| match &scenario.steps[span.step] {
        Step::PtpJoints { speed_scale, .. }
        | Step::PtpPose { speed_scale, .. }
        | Step::Lin { speed_scale, .. } => *speed_scale,
        _ => 1.0,
    };
    let worst = &mut stats.worst;
    for span in &traj.steps {
        let speed = speed_of(span);
        for k in span.samples.clone() {
            let st = &traj.samples[k].robots[0];
            for (i, j) in joints.iter().enumerate() {
                let r = [
                    st.qd[i].abs() / (speed * j.max_velocity),
                    st.qdd[i].abs() / (speed * j.max_acceleration),
                    st.qddd[i].abs() / (speed * j.max_jerk),
                ];
                for (w, r) in worst.iter_mut().zip(r) {
                    *w = w.max(r);
                }
                assert!(
                    r.iter().all(|r| *r <= 1.0 + 1e-9),
                    "step {} sample {k} joint {}: v/a/j ratios {r:?}",
                    span.step,
                    j.name
                );
                assert!(
                    st.q[i] >= j.lower - 1e-12 && st.q[i] <= j.upper + 1e-12,
                    "joint {} outside its range",
                    j.name
                );
                let next = &traj.samples[k + 1].robots[0];
                let fd = (next.q[i] - st.q[i]) / dt;
                assert!(
                    fd.abs() <= speed * j.max_velocity * (1.0 + 1e-9),
                    "FD velocity {fd} at {k}"
                );
                if k > 0 {
                    let prev = &traj.samples[k - 1].robots[0];
                    let fdd = (next.q[i] - 2.0 * st.q[i] + prev.q[i]) / (dt * dt);
                    assert!(
                        fdd.abs() <= speed * j.max_acceleration * (1.0 + 1e-6),
                        "FD acceleration {fdd} at {k}"
                    );
                }
            }
        }
        // PTP: straight in joint space.
        if matches!(
            scenario.steps[span.step],
            Step::PtpJoints { .. } | Step::PtpPose { .. }
        ) && !span.samples.is_empty()
        {
            let q0 = &traj.samples[span.samples.start].robots[0].q;
            let q1 = &traj.samples[span.samples.end].robots[0].q;
            let d: Vec<f64> = q1.iter().zip(q0).map(|(a, b)| a - b).collect();
            let norm = d.iter().map(|x| x * x).sum::<f64>().sqrt();
            for k in span.samples.clone() {
                let q = &traj.samples[k].robots[0].q;
                let p: Vec<f64> = q.iter().zip(q0).map(|(a, b)| a - b).collect();
                let along = p.iter().zip(&d).map(|(a, b)| a * b).sum::<f64>() / norm;
                let off = p
                    .iter()
                    .zip(&d)
                    .map(|(a, b)| (a - along * b / norm).powi(2))
                    .sum::<f64>()
                    .sqrt();
                assert!(off <= 1e-12 * norm.max(1.0), "PTP leaves the line by {off}");
            }
            // An n-DoF Ruckig solution with phase synchronisation takes as long.
            let dofs = joints.len();
            let mut otg = Ruckig::<0, ThrowErrorHandler>::new(Some(dofs), dt);
            let mut input = InputParameter::<0>::new(Some(dofs));
            input.synchronization = Synchronization::Phase;
            for (i, j) in joints.iter().enumerate() {
                input.current_position[i] = q0[i];
                input.target_position[i] = q1[i];
                input.max_velocity[i] = speed * j.max_velocity;
                input.max_acceleration[i] = speed * j.max_acceleration;
                input.max_jerk[i] = speed * j.max_jerk;
            }
            let mut oracle = rsruckig::prelude::Trajectory::<0>::new(Some(dofs));
            otg.calculate(&input, &mut oracle).unwrap();
            // Ruckig keeps the slowest joint's profile shape only where it
            // fits every other joint; elsewhere it falls back to time
            // synchronisation, which is faster but leaves the line. So its
            // duration bounds ours from below, and equals it (up to the
            // rounding to whole samples) when one joint binds velocity,
            // acceleration and jerk alike.
            let ours = span.samples.len() as f64 * dt;
            let theirs = oracle.get_duration();
            assert!(
                ours >= theirs - 1e-9,
                "PTP takes {ours} s, Ruckig {theirs} s"
            );
            let binding = |f: fn(&etendue_kinematics::ActiveJoint) -> f64| {
                (0..dofs).filter(|&i| d[i] != 0.0).min_by(|&a, &b| {
                    (f(&joints[a]) / d[a].abs()).total_cmp(&(f(&joints[b]) / d[b].abs()))
                })
            };
            let v = binding(|j| j.max_velocity);
            if v == binding(|j| j.max_acceleration) && v == binding(|j| j.max_jerk) {
                assert!(
                    ours <= theirs + dt + 1e-9,
                    "PTP takes {ours} s, Ruckig {theirs} s"
                );
                stats.phase_checked += 1;
            }
        }
    }
    for (k, s) in traj.samples.iter().enumerate() {
        if s.capture.is_some() {
            let st = &s.robots[0];
            assert!(
                st.qd
                    .iter()
                    .chain(&st.qdd)
                    .chain(&st.qddd)
                    .all(|v| *v == 0.0),
                "capture {k} not at rest"
            );
            assert_eq!(
                traj.samples[k + 1].robots[0].q,
                st.q,
                "moved during capture {k}"
            );
        }
    }
}

#[test]
fn g1_4_motion_is_feasible_on_every_robot() {
    for (id, path) in library() {
        let (manifest, model) = load(&path);
        assert!(
            manifest.joints.iter().all(|j| j.max_jerk.is_some()),
            "{id}: every library joint has a jerk limit"
        );
        let mut rng = Rng::new(20261017);
        let (mut programs, mut moves, mut lin_singular) = (0, 0, 0);
        let mut stats = Stats::default();
        for _ in 0..10 {
            let dt = rng.uniform(0.002, 0.02);
            let Some((q0, scenario)) = program(&model, &id, &mut rng, dt) else {
                lin_singular += 1;
                continue;
            };
            match compile(&scene(&id, &q0), &scenario, std::slice::from_ref(&model)) {
                Ok(traj) => {
                    check(&model, &scenario, &traj, &mut stats);
                    programs += 1;
                    moves += traj.steps.iter().filter(|s| s.robot.is_some()).count();
                }
                // A random LIN may cross a singularity: a reported error,
                // not a limit violation.
                Err(Error::Scenario { message, .. }) if message.contains("lin") => {
                    lin_singular += 1
                }
                Err(e) => panic!("{id}: {e}"),
            }
        }
        let worst = stats.worst;
        println!(
            "G1.4 {id}: {programs} programs, {moves} moves, peak v/a/j ratios {:.9} / {:.9} / {:.9}; \
             {} PTP durations equal to Ruckig's; {lin_singular} programs with an infeasible LIN",
            worst[0], worst[1], worst[2], stats.phase_checked
        );
        assert!(programs >= 3, "{id}: too few programs compiled");
        assert!(
            stats.phase_checked > 0,
            "{id}: no PTP was compared with Ruckig"
        );
        // Time-optimal: some move saturates a limit.
        assert!(
            worst.iter().any(|w| *w > 0.99),
            "{id}: suspiciously slow {worst:?}"
        );
    }
}

fn ur5e() -> (String, RobotModel) {
    let (_, model) = load(&repo_root().join("assets/robots/ur5e/robot.json"));
    ("ur5e".into(), model)
}

const UR5E_HOME: [f64; 6] = [3.0, -1.6, 1.6, -1.5, -1.6, 0.2];

#[test]
fn stored_configurations_pin_the_ik_branch() {
    let (id, model) = ur5e();
    let s = scene(&id, &UR5E_HOME);
    let poses: Vec<_> = [
        [3.3, -1.4, 1.5, -1.6, -1.5, 0.4],
        [2.8, -1.8, 1.9, -1.7, -1.7, 0.0],
    ]
    .iter()
    .map(|q| serde_json::to_value(model.tcp_pose(q)).unwrap())
    .collect();
    let scenario: ScenarioSpec = serde_json::from_value(json!({
        "version": 1, "dt": 0.01,
        "steps": [
            {"type": "ptp_pose", "robot": id, "base_se3_tool": poses[0]},
            {"type": "capture"},
            {"type": "ptp_pose", "robot": id, "base_se3_tool": poses[1], "speed_scale": 0.5},
            {"type": "capture"}
        ]
    }))
    .unwrap();
    let resolved = resolve(&s, &scenario, std::slice::from_ref(&model)).unwrap();
    for step in &resolved.steps {
        if let Step::PtpPose { q, .. } = step {
            assert!(q.is_some(), "every ptp_pose is resolved");
        }
    }
    let a = compile(&s, &scenario, std::slice::from_ref(&model)).unwrap();
    let b = compile(&s, &resolved, std::slice::from_ref(&model)).unwrap();
    assert_eq!(a.samples.len(), b.samples.len());
    for (x, y) in a.samples.iter().zip(&b.samples) {
        assert_eq!(x.robots[0].q, y.robots[0].q);
    }
    assert!(matches!(b.steps[0].ik, Some((IkReport::Stored, _))));
    assert!(matches!(a.steps[0].ik, Some((IkReport::Solved { .. }, _))));

    // A stored configuration that no longer reaches the pose is refused.
    let mut stale = resolved.clone();
    if let Step::PtpPose { q: Some(q), .. } = &mut stale.steps[2] {
        q[0] += 1e-3;
    }
    let err = compile(&s, &stale, std::slice::from_ref(&model)).unwrap_err();
    assert!(
        matches!(&err, Error::Scenario { step: 2, message } if message.contains("stored q")),
        "{err}"
    );
}

#[test]
fn the_report_names_the_binding_limit() {
    let (id, model) = ur5e();
    let s = scene(&id, &UR5E_HOME);
    let scenario: ScenarioSpec = serde_json::from_value(json!({
        "version": 1, "dt": 0.005,
        "steps": [
            {"type": "ptp_joints", "robot": id, "q": [3.5, -1.6, 1.6, -1.5, -1.6, 0.2]},
            {"type": "capture"},
            {"type": "wait", "duration_s": 0.1},
            {"type": "ptp_joints", "robot": id, "q": [3.5, -1.6, 1.6, -1.5, -1.6, 0.2]}
        ]
    }))
    .unwrap();
    let r = report(
        &s,
        &scenario,
        std::slice::from_ref(&model),
        &ReportOptions::default(),
    )
    .unwrap();
    assert_eq!(r.steps.len(), 4);
    assert_eq!(r.captures, 1);
    let first = &r.steps[0];
    assert_eq!(first.kind, "ptp_joints");
    assert_eq!(first.robot.as_deref(), Some("ur5e"));
    let binding = first.binding.as_ref().unwrap();
    // Only shoulder_pan moves, by 0.5 rad: a short move that never reaches
    // its speed limit, so acceleration or jerk binds, at the limit.
    assert_eq!(binding.joint, "shoulder_pan_joint");
    assert_ne!(binding.quantity, Quantity::Velocity);
    assert!((binding.fraction - 1.0).abs() < 1e-6, "{binding:?}");
    assert!(first.min_sigma.unwrap() > 0.0);
    // The same configuration again: no motion.
    assert_eq!(r.steps[3].duration_s, 0.0);
    assert!((r.steps[2].duration_s - 0.1).abs() < 1e-12);
}
