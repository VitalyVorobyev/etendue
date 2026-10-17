//! P7-6: sphere poses around a target. Geometry, feasibility, visibility in
//! every camera at every kept capture, and a short tour.

use std::path::Path;

use etendue_kinematics::{RobotModel, bake, compile};
use etendue_scene::{RobotManifest, SceneSpec, SphereGenerator};
use etendue_synth::PixelCentre;
use etendue_synth::board::layout;
use etendue_synth::gt::{VisibilitySpec, project_points};
use etendue_synth::poses::{CandidateStatus, generate};
use nalgebra::{Point3, Vector3};

fn read(path: &str) -> String {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    std::fs::read_to_string(root.join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn setup() -> (SceneSpec, RobotModel) {
    let scene: SceneSpec =
        serde_json::from_str(&read("examples/closed_loop_ur5e/scene.json")).unwrap();
    let manifest: RobotManifest =
        serde_json::from_str(&read("assets/robots/ur5e/robot.json")).unwrap();
    let model =
        RobotModel::from_urdf_str(&read("assets/robots/ur5e/robot.urdf"), &manifest).unwrap();
    (scene, model)
}

fn spec() -> SphereGenerator {
    serde_json::from_value(serde_json::json!({
        "robot": "ur5e", "look": "rig", "target": "board",
        "radii_m": [0.45, 0.55],
        "theta": {"min_rad": 0.17, "max_rad": 0.6, "steps": 3},
        "phi": {"min_rad": 0.0, "max_rad": std::f64::consts::TAU, "steps": 8},
        "roll_rad": [0.0, 0.3]
    }))
    .unwrap()
}

#[test]
fn every_candidate_looks_at_the_centre() {
    let (scene, model) = setup();
    let generated = generate(&scene, std::slice::from_ref(&model), &spec(), 0.01).unwrap();
    assert_eq!(generated.candidates.len(), 2 * 3 * 8 * 2);
    let board = scene.targets.iter().find(|t| t.id == "board").unwrap();
    let centre = board.parent_se3_self * Point3::origin();
    for c in &generated.candidates {
        let eye = Point3::from(c.world_se3_sensor.translation.vector);
        let axis = c.world_se3_sensor.rotation * Vector3::z();
        let to_centre = centre - eye;
        assert!((to_centre.norm() - c.radius_m).abs() < 1e-12);
        assert!(
            (to_centre.normalize() - axis).norm() < 1e-12,
            "the sensor looks at the centre"
        );
    }
}

#[test]
fn kept_poses_are_feasible_and_every_camera_sees_the_whole_board() {
    let (scene, model) = setup();
    let robots = std::slice::from_ref(&model);
    let generated = generate(&scene, robots, &spec(), 0.01).unwrap();
    let kept = generated
        .candidates
        .iter()
        .filter(|c| c.status == CandidateStatus::Ok)
        .count();
    let rejected: Vec<_> = generated
        .candidates
        .iter()
        .filter(|c| c.status != CandidateStatus::Ok)
        .map(|c| &c.status)
        .collect();
    println!(
        "{kept} kept of {}; rejected: {rejected:?}",
        generated.candidates.len()
    );
    assert!(kept >= 20, "{kept} poses kept");

    // The scenario compiles with its stored configurations, and at every
    // capture each camera sees every board point.
    let scenario = &generated.scenario;
    let trajectory = compile(&scene, scenario, robots).unwrap();
    assert_eq!(
        trajectory
            .samples
            .iter()
            .filter(|s| s.capture.is_some())
            .count(),
        kept
    );
    let baked = bake(&scene, scenario, robots).unwrap();
    let points = layout(&scene.targets[0].geometry).unwrap().unwrap().points;
    let frame = |name: &str| baked.frames.iter().position(|f| f == name).unwrap();
    for k in baked.capture_indices() {
        let sample = &baked.samples[k];
        for camera in &scene.cameras {
            let gt = project_points(
                &camera.params.build().unwrap(),
                camera.resolution,
                &sample.world_se3_frame[frame(&camera.id)],
                &sample.world_se3_frame[frame("board")],
                &points,
                &VisibilitySpec::default(),
                PixelCentre::Integer.edge(),
            );
            assert!(gt.iter().all(|p| p.visible()), "{} at {k}", camera.id);
        }
    }
}

#[test]
fn the_tour_is_no_longer_than_the_grid_order() {
    let (scene, model) = setup();
    let generated = generate(&scene, std::slice::from_ref(&model), &spec(), 0.01).unwrap();
    let travel = |a: &[f64], b: &[f64]| {
        a.iter()
            .zip(b)
            .zip(model.active_joints())
            .map(|((x, y), j)| (x - y).abs() / j.max_velocity)
            .fold(0.0, f64::max)
    };
    let length = |qs: &[&Vec<f64>]| -> f64 {
        let start = scene.robots[0].initial_q.clone().unwrap();
        let mut here = start;
        let mut total = 0.0;
        for q in qs {
            total += travel(&here, q);
            here = (*q).clone();
        }
        total
    };
    let grid: Vec<&Vec<f64>> = generated
        .candidates
        .iter()
        .filter(|c| c.status == CandidateStatus::Ok)
        .map(|c| c.q.as_ref().unwrap())
        .collect();
    let tour: Vec<&Vec<f64>> = generated
        .scenario
        .steps
        .iter()
        .filter_map(|s| match s {
            etendue_scene::Step::PtpPose { q, .. } => q.as_ref(),
            _ => None,
        })
        .collect();
    assert_eq!(tour.len(), grid.len());
    let (t, g) = (length(&tour), length(&grid));
    println!("tour {t:.2} s vs grid order {g:.2} s (slowest-joint travel)");
    assert!(t <= g + 1e-9);
}

#[test]
fn eye_to_hand_and_bad_specs_are_refused() {
    let (scene, model) = setup();
    let robots = std::slice::from_ref(&model);
    let mut s = spec();
    s.look = "board".into();
    assert!(
        generate(&scene, robots, &s, 0.01).is_err(),
        "the board is not on the tool"
    );
    let mut s = spec();
    s.radii_m.clear();
    assert!(generate(&scene, robots, &s, 0.01).is_err());
    let mut s = spec();
    s.robot = "nope".into();
    assert!(generate(&scene, robots, &s, 0.01).is_err());
}
