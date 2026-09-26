//! Scenario compiler and baker (P1-5): joint limits are never exceeded and
//! captures happen at rest.

mod common;

/// Agreement between two floating-point evaluation paths (different
/// operation order; the platform libm's `sin`/`cos` differ by ULPs, e.g.
/// MSVC vs glibc/macOS). Far below every gate (G1.1: 1e-9).
const TOL: f64 = 1e-12;

use common::{Rng, pose_distance, test_arm};
use etendue_kinematics::{Error, RobotModel, Trajectory, bake, compile};
use etendue_scene::{ScenarioSpec, SceneSpec, Step};
use nalgebra::{Isometry3, Translation3, UnitQuaternion, Vector3};
use proptest::prelude::*;
use serde_json::json;

fn iso(t: [f64; 3]) -> serde_json::Value {
    json!({"rotation": [0.0, 0.0, 0.0, 1.0], "translation": t})
}

const HOME: [f64; 6] = [0.0, -1.2, 1.4, -0.4, 1.2, 0.3];

fn scene() -> SceneSpec {
    serde_json::from_value(json!({
        "version": 1,
        "robots": [{"id": "arm", "parent": "world", "parent_se3_self": iso([0.0, 0.0, 0.8]),
                    "manifest": "test_arm.json", "initial_q": HOME}],
        "rigs": [{"id": "rig", "parent": "arm/tool0", "parent_se3_self": iso([0.0, 0.0, 0.05])}],
        "cameras": [{"id": "cam", "parent": "rig", "parent_se3_self": iso([0.03, 0.0, 0.0]),
            "params": {
                "projection": {"type": "pinhole"}, "distortion": {"type": "none"},
                "sensor": {"type": "identity"},
                "intrinsics": {"type": "fx_fy_cx_cy_skew", "fx": 1000.0, "fy": 1000.0, "cx": 640.0, "cy": 512.0, "skew": 0.0}},
            "resolution": [1280, 1024]}],
        "targets": [{"id": "board", "parent": "world", "parent_se3_self": iso([0.5, 0.0, 0.0]),
            "geometry": {"type": "board", "board": {"kind": "chessboard", "rows": 6, "cols": 9, "square_size_m": 0.02}}}]
    }))
    .unwrap()
}

/// Check every limit on a compiled trajectory; returns (max |q̇|/v, max |q̈|/a).
fn check_limits(model: &RobotModel, traj: &Trajectory) -> (f64, f64) {
    let (mut vr, mut ar) = (0.0_f64, 0.0_f64);
    for (k, s) in traj.samples.iter().enumerate() {
        let st = &s.robots[0];
        for (i, j) in model.active_joints().iter().enumerate() {
            assert!(
                st.q[i] >= j.lower - 1e-12 && st.q[i] <= j.upper + 1e-12,
                "sample {k}: joint {} = {} outside limits",
                j.name,
                st.q[i]
            );
            vr = vr.max(st.qd[i].abs() / j.max_velocity);
            ar = ar.max(st.qdd[i].abs() / j.max_acceleration);
            // Finite differences of positions obey the same bounds (mean
            // value theorem; the trajectory is C¹ with bounded q̈).
            if k + 1 < traj.samples.len() {
                let fd = (traj.samples[k + 1].robots[0].q[i] - st.q[i]) / traj.dt;
                assert!(
                    fd.abs() <= j.max_velocity * (1.0 + 1e-9),
                    "FD velocity {fd} at {k}"
                );
            }
            if k >= 1 && k + 1 < traj.samples.len() {
                let fdd = (traj.samples[k + 1].robots[0].q[i] - 2.0 * st.q[i]
                    + traj.samples[k - 1].robots[0].q[i])
                    / (traj.dt * traj.dt);
                assert!(
                    fdd.abs() <= j.max_acceleration * (1.0 + 1e-6),
                    "FD accel {fdd} at {k}"
                );
            }
        }
        if s.capture.is_some() {
            assert!(
                st.qd.iter().chain(&st.qdd).all(|v| *v == 0.0),
                "capture {k} not at rest"
            );
            // Stationary for the whole capture interval.
            if k + 1 < traj.samples.len() {
                assert_eq!(
                    traj.samples[k + 1].robots[0].q,
                    st.q,
                    "moved during capture {k}"
                );
            }
        }
    }
    assert!(
        vr <= 1.0 + 1e-9 && ar <= 1.0 + 1e-9,
        "limit ratios v {vr}, a {ar}"
    );
    (vr, ar)
}

fn program() -> ScenarioSpec {
    let model = test_arm();
    let home_pose = model.tcp_pose(&HOME);
    let shifted = home_pose
        * Isometry3::from_parts(
            Translation3::new(0.05, -0.03, 0.02),
            UnitQuaternion::from_axis_angle(&Vector3::y_axis(), 0.15),
        );
    let pose2 = home_pose * Isometry3::translation(-0.04, 0.06, 0.0);
    serde_json::from_value(json!({
        "version": 1, "dt": 0.01,
        "steps": [
            {"type": "capture"},
            {"type": "lin", "robot": "arm", "base_se3_tool": serde_json::to_value(shifted).unwrap()},
            {"type": "capture"},
            {"type": "ptp_pose", "robot": "arm", "base_se3_tool": serde_json::to_value(pose2).unwrap(), "speed_scale": 0.5},
            {"type": "capture", "id": "third"},
            {"type": "wait", "duration_s": 0.25},
            {"type": "ptp_joints", "robot": "arm", "q": [0.3, -1.0, 1.2, -0.2, 1.0, 0.0]},
            {"type": "capture"}
        ]
    }))
    .unwrap()
}

#[test]
fn compiled_program_respects_limits_and_captures_at_rest() {
    let model = test_arm();
    let traj = compile(&scene(), &program(), std::slice::from_ref(&model)).unwrap();
    let (vr, ar) = check_limits(&model, &traj);
    // The fastest motion of each step saturates some limit.
    assert!(vr > 0.9 || ar > 0.9, "suspiciously slow: v {vr}, a {ar}");
    let ids: Vec<_> = traj
        .samples
        .iter()
        .filter_map(|s| s.capture.clone())
        .collect();
    assert_eq!(ids, ["cap_000", "cap_001", "third", "cap_003"]);
}

#[test]
fn lin_keeps_the_tcp_on_the_straight_line() {
    let model = test_arm();
    let scenario = program();
    let Step::Lin {
        base_se3_tool: target,
        ..
    } = scenario.steps[1]
    else {
        unreachable!()
    };
    let traj = compile(&scene(), &scenario, std::slice::from_ref(&model)).unwrap();
    let start = model.tcp_pose(&HOME).translation.vector;
    let end = target.translation.vector;
    let dir = (end - start).normalize();
    // Samples between the first and second capture are the lin move.
    let caps: Vec<usize> = (0..traj.samples.len())
        .filter(|&k| traj.samples[k].capture.is_some())
        .collect();
    let mut worst = 0.0_f64;
    for s in &traj.samples[caps[0]..=caps[1]] {
        let p = model.tcp_pose(&s.robots[0].q).translation.vector - start;
        worst = worst.max((p - dir * p.dot(&dir)).norm());
    }
    assert!(worst < 1e-6, "lin deviates {worst} m from the line");
    let (dt, dr) = pose_distance(&model.tcp_pose(&traj.samples[caps[1]].robots[0].q), &target);
    assert!(dt < 1e-9 && dr < 1e-9);
}

#[test]
fn bake_composes_the_frame_tree() {
    let model = test_arm();
    let s = scene();
    let baked = bake(&s, &program(), std::slice::from_ref(&model)).unwrap();
    baked.validate().unwrap();
    assert_eq!(baked.frames[0], "world");
    assert_eq!(
        baked.robots[0].joint_names,
        ["j1", "j2", "j3", "j4", "j5", "j6"]
    );
    let cam = baked.frames.iter().position(|f| f == "cam").unwrap();
    let tool0 = baked.frames.iter().position(|f| f == "arm/tool0").unwrap();
    for k in baked.capture_indices() {
        let sample = &baked.samples[k];
        let q = &sample.joint_positions[0];
        let expected = Isometry3::translation(0.0, 0.0, 0.8)
            * model.tcp_pose(q)
            * Isometry3::translation(0.0, 0.0, 0.05)
            * Isometry3::translation(0.03, 0.0, 0.0);
        let (dt, dr) = pose_distance(&sample.world_se3_frame[cam], &expected);
        assert!(dt < TOL && dr < TOL, "{dt} {dr}");
        let (dt, _) = pose_distance(
            &sample.world_se3_frame[tool0],
            &(Isometry3::translation(0.0, 0.0, 0.8) * model.tcp_pose(q)),
        );
        assert!(dt < TOL);
        assert_eq!(sample.t, k as f64 * baked.dt);
    }
}

#[test]
fn infeasible_steps_are_reported_with_their_index() {
    let model = test_arm();
    let bad: ScenarioSpec = serde_json::from_value(json!({
        "version": 1, "dt": 0.01,
        "steps": [{"type": "capture"}, {"type": "ptp_joints", "robot": "arm", "q": [9.0, 0, 0, 0, 0, 0]}]
    }))
    .unwrap();
    assert!(matches!(
        compile(&scene(), &bad, std::slice::from_ref(&model)),
        Err(Error::Scenario { step: 1, .. })
    ));
    let unreachable: ScenarioSpec = serde_json::from_value(json!({
        "version": 1, "dt": 0.01,
        "steps": [{"type": "ptp_pose", "robot": "arm", "base_se3_tool": iso([5.0, 0.0, 0.0])}]
    }))
    .unwrap();
    assert!(matches!(
        compile(&scene(), &unreachable, std::slice::from_ref(&model)),
        Err(Error::Scenario { step: 0, .. })
    ));
}

fn random_step(seed: u64) -> serde_json::Value {
    let model = test_arm();
    let mut rng = Rng::new(seed);
    match seed % 4 {
        0 => json!({"type": "ptp_joints", "robot": "arm", "q": rng.q(&model, 0.05),
                    "speed_scale": rng.uniform(0.05, 1.0)}),
        1 => json!({"type": "capture"}),
        2 => json!({"type": "wait", "duration_s": rng.uniform(0.0, 0.2)}),
        _ => {
            // A small Cartesian offset from HOME, expressed in the tool frame.
            let offset = Isometry3::from_parts(
                Translation3::new(
                    rng.uniform(-0.05, 0.05),
                    rng.uniform(-0.05, 0.05),
                    rng.uniform(-0.05, 0.05),
                ),
                UnitQuaternion::from_euler_angles(
                    rng.uniform(-0.1, 0.1),
                    rng.uniform(-0.1, 0.1),
                    rng.uniform(-0.1, 0.1),
                ),
            );
            json!([
                {"type": "ptp_joints", "robot": "arm", "q": HOME},
                {"type": "lin", "robot": "arm", "base_se3_tool": serde_json::to_value(model.tcp_pose(&HOME) * offset).unwrap(),
                 "speed_scale": rng.uniform(0.05, 1.0)}
            ])
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn random_programs_never_exceed_limits(seeds in proptest::collection::vec(any::<u64>(), 1..7), dt in 0.002..0.05f64) {
        let mut steps = Vec::new();
        for seed in seeds {
            match random_step(seed) {
                serde_json::Value::Array(v) => steps.extend(v),
                v => steps.push(v),
            }
        }
        let scenario: ScenarioSpec = serde_json::from_value(json!({"version": 1, "dt": dt, "steps": steps})).unwrap();
        let model = test_arm();
        match compile(&scene(), &scenario, std::slice::from_ref(&model)) {
            Ok(traj) => { check_limits(&model, &traj); }
            // A random lin may cross a singularity; that is a reported
            // error, not a limit violation.
            Err(Error::Scenario { message, .. }) if message.contains("lin") => prop_assume!(false),
            Err(e) => return Err(TestCaseError::fail(e.to_string())),
        }
    }
}
