//! Validation and frame-graph resolution.

use approx::assert_relative_eq;
use etendue_scene::{FrameGraph, ScenarioSpec, SceneSpec, ValidationError};
use nalgebra::{Isometry3, Point3, Translation3, UnitQuaternion, Vector3};
use serde_json::json;

fn iso(t: [f64; 3]) -> serde_json::Value {
    json!({"rotation": [0.0, 0.0, 0.0, 1.0], "translation": t})
}

fn pinhole() -> serde_json::Value {
    json!({
        "projection": {"type": "pinhole"},
        "distortion": {"type": "none"},
        "sensor": {"type": "identity"},
        "intrinsics": {"type": "fx_fy_cx_cy_skew", "fx": 1000.0, "fy": 1000.0, "cx": 640.0, "cy": 512.0, "skew": 0.0}
    })
}

/// Eye-in-hand: a rig on the robot flange, two cameras on the rig, a board
/// on the table.
fn eye_in_hand() -> serde_json::Value {
    json!({
        "version": 1,
        "frames": [{"id": "table", "parent": "world", "parent_se3_self": iso([0.6, 0.0, 0.0])}],
        "robots": [{"id": "ur", "parent": "world", "parent_se3_self": iso([0.0, 0.0, 0.8]), "manifest": "ur5e/robot.json"}],
        "rigs": [{"id": "rig", "parent": "ur/tool0", "parent_se3_self": iso([0.0, 0.0, 0.05])}],
        "cameras": [
            {"id": "cam0", "parent": "rig", "parent_se3_self": iso([-0.05, 0.0, 0.0]), "params": pinhole(), "resolution": [1280, 1024]},
            {"id": "cam1", "parent": "rig", "parent_se3_self": iso([0.05, 0.0, 0.0]), "params": pinhole(), "resolution": [1280, 1024]}
        ],
        "targets": [{"id": "board", "parent": "table", "parent_se3_self": iso([0.0, 0.0, 0.8]),
            "geometry": {"type": "board", "board": {"kind": "chessboard", "rows": 6, "cols": 9, "square_size_m": 0.02}}}]
    })
}

fn scene(v: serde_json::Value) -> SceneSpec {
    serde_json::from_value(v).expect("scene parses")
}

fn messages(e: &ValidationError) -> String {
    e.to_string()
}

#[test]
fn eye_in_hand_scene_is_valid() {
    scene(eye_in_hand()).validate().expect("valid scene");
}

#[test]
fn duplicate_ids_are_rejected_across_kinds() {
    let mut v = eye_in_hand();
    v["targets"][0]["id"] = json!("cam0");
    let err = scene(v).validate().unwrap_err();
    assert!(messages(&err).contains("duplicate id `cam0`"), "{err}");
}

#[test]
fn dangling_and_bare_robot_parents_are_rejected() {
    let mut v = eye_in_hand();
    v["cameras"][0]["parent"] = json!("nope");
    v["rigs"][0]["parent"] = json!("ur");
    let err = scene(v).validate().unwrap_err();
    let m = messages(&err);
    assert!(m.contains("unknown parent frame `nope`"), "{m}");
    assert!(m.contains("`ur` is a robot, not a frame"), "{m}");
}

#[test]
fn attachment_cycles_are_rejected() {
    let mut v = eye_in_hand();
    v["frames"] = json!([
        {"id": "a", "parent": "b", "parent_se3_self": iso([0.0, 0.0, 0.0])},
        {"id": "b", "parent": "a", "parent_se3_self": iso([0.0, 0.0, 0.0])}
    ]);
    v["targets"][0]["parent"] = json!("world");
    let err = scene(v).validate().unwrap_err();
    assert!(messages(&err).contains("attachment cycle"), "{err}");
}

#[test]
fn non_unit_quaternions_are_rejected() {
    let mut v = eye_in_hand();
    v["rigs"][0]["parent_se3_self"]["rotation"] = json!([0.0, 0.0, 0.7, 0.7]);
    let err = scene(v).validate().unwrap_err();
    assert!(messages(&err).contains("unit length"), "{err}");
}

#[test]
fn bad_entity_parameters_are_rejected() {
    let mut v = eye_in_hand();
    v["cameras"][0]["resolution"] = json!([0, 1024]);
    v["lasers"] = json!([{"id": "l", "parent": "world", "parent_se3_self": iso([0.0, 0.0, 0.0]),
        "fan_half_angle": 2.0, "fan_length": 1.0, "wavelength_nm": 660.0, "beam_waist_m": 0.0}]);
    let err = scene(v).validate().unwrap_err();
    let m = messages(&err);
    assert!(m.contains("cameras[0].resolution"), "{m}");
    assert!(m.contains("lasers[0].fan_half_angle"), "{m}");
    assert!(m.contains("lasers[0].beam_waist_m"), "{m}");
}

#[test]
fn frame_graph_orders_parents_first_and_composes_poses() {
    let s = scene(eye_in_hand());
    let links = vec![vec!["base_link".to_owned(), "tool0".to_owned()]];
    let graph = FrameGraph::build(&s, &links).expect("graph");
    let names: Vec<&str> = graph.names().collect();
    assert_eq!(names[0], "world");
    for (i, name) in names.iter().enumerate() {
        // Every frame comes after its parent.
        let parent = match *name {
            "world" => continue,
            "table" | "ur/base_link" | "ur/tool0" => "world",
            "rig" => "ur/tool0",
            "cam0" | "cam1" => "rig",
            "board" => "table",
            other => panic!("unexpected frame {other}"),
        };
        assert!(
            graph.index_of(parent).unwrap() < i,
            "{name} before its parent {parent}"
        );
    }

    // FK stand-in: tool0 is 0.5 m above the base, rotated 90° about +Z.
    let base_se3_tool0 = Isometry3::from_parts(
        Translation3::new(0.0, 0.0, 0.5),
        UnitQuaternion::from_axis_angle(&Vector3::z_axis(), std::f64::consts::FRAC_PI_2),
    );
    let world = graph.resolve(|_robot, link| {
        if link == 1 {
            base_se3_tool0
        } else {
            Isometry3::identity()
        }
    });
    let cam0 = world[graph.index_of("cam0").unwrap()];
    // world ← base (z + 0.8) ← tool0 (z + 0.5, Rz 90°) ← rig (z + 0.05) ← cam0 (x − 0.05).
    // Rz(90°) maps cam0's −0.05 x offset onto −0.05 y.
    assert_relative_eq!(
        cam0 * Point3::origin(),
        Point3::new(0.0, -0.05, 1.35),
        epsilon = 1e-12
    );
    let board = world[graph.index_of("board").unwrap()];
    assert_relative_eq!(
        board * Point3::origin(),
        Point3::new(0.6, 0.0, 0.8),
        epsilon = 1e-15
    );
}

#[test]
fn frame_graph_checks_link_names() {
    let s = scene(eye_in_hand());
    let err = FrameGraph::build(&s, &[vec!["base_link".to_owned()]]).unwrap_err();
    assert!(
        err.to_string().contains("robot `ur` has no link `tool0`"),
        "{err}"
    );
}

#[test]
fn scenario_validation() {
    let s = scene(eye_in_hand());
    let ok: ScenarioSpec = serde_json::from_value(json!({
        "version": 1, "dt": 0.01,
        "steps": [
            {"type": "ptp_joints", "robot": "ur", "q": [0, -1.57, 1.57, 0, 1.57, 0]},
            {"type": "capture"},
            {"type": "lin", "robot": "ur", "base_se3_tool": iso([0.4, 0.1, 0.3]), "speed_scale": 0.5},
            {"type": "capture", "id": "second"},
            {"type": "wait", "duration_s": 0.5}
        ]
    }))
    .unwrap();
    ok.validate(&s).expect("valid scenario");

    let bad: ScenarioSpec = serde_json::from_value(json!({
        "version": 1, "dt": 0.0,
        "steps": [
            {"type": "ptp_joints", "robot": "kuka", "q": [0], "speed_scale": 1.5},
            {"type": "capture", "id": "cap_001"},
            {"type": "capture"}
        ]
    }))
    .unwrap();
    let m = bad.validate(&s).unwrap_err().to_string();
    assert!(m.contains("dt"), "{m}");
    assert!(m.contains("unknown robot `kuka`"), "{m}");
    assert!(m.contains("speed_scale"), "{m}");
    // The second capture defaults to `cap_001`, colliding with the explicit id.
    assert!(m.contains("duplicate capture id `cap_001`"), "{m}");
}

#[test]
fn scenario_from_tool_poses() {
    let s = scene(eye_in_hand());
    let a = Isometry3::translation(0.4, 0.0, 0.5);
    let b = Isometry3::translation(0.5, 0.1, 0.4);
    let spec = ScenarioSpec::from_poses("ur", &[(None, a), (Some("near".into()), b)], 0.5, 0.01);
    spec.validate(&s).expect("valid scenario");
    let json = serde_json::to_value(&spec).unwrap();
    let steps = json["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 4);
    assert_eq!(steps[0]["type"], "ptp_pose");
    assert_eq!(steps[0]["speed_scale"], 0.5);
    assert_eq!(steps[1], json!({"type": "capture"}));
    assert_eq!(steps[3], json!({"type": "capture", "id": "near"}));
    // An unknown robot is the validator's to reject.
    let bad = ScenarioSpec::from_poses("kuka", &[(None, a)], 1.0, 0.01);
    assert!(bad.validate(&s).is_err());
}
