//! P3-2: synthetic datasets from the repository examples are accepted by
//! `vision_calibration_dataset::validate`, and their ground truth agrees with
//! the scene.

use std::path::Path;

use etendue_kinematics::{RobotModel, bake};
use etendue_scene::{RobotManifest, ScenarioSpec, SceneSpec};
use etendue_synth::PixelCentre;
use etendue_synth::dataset::{EmitOptions, GtHandeye, emit};
use etendue_synth::gt::{BoardPoint, VisibilitySpec};
use nalgebra::Point3;
use vision_calibration_dataset::{Topology, validate};

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn read(path: &str) -> String {
    std::fs::read_to_string(root().join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// Inner corners of the examples' 9 × 6 chessboard (25 mm), centred on the
/// target origin. A test stand-in: the board layout's single source is P3-3.
fn chessboard_points() -> Vec<BoardPoint> {
    let s = 0.025;
    (0..6)
        .flat_map(|r| {
            (0..9).map(move |c| BoardPoint {
                position_m: [(f64::from(c) - 4.0) * s, (f64::from(r) - 2.5) * s],
                grid: Some([c, r]),
                id: None,
            })
        })
        .collect()
}

fn example(name: &str) -> (SceneSpec, etendue_scene::BakedScenario, Vec<RobotManifest>) {
    let scene: SceneSpec =
        serde_json::from_str(&read(&format!("examples/{name}/scene.json"))).unwrap();
    let scenario: ScenarioSpec =
        serde_json::from_str(&read(&format!("examples/{name}/scenario.json"))).unwrap();
    let manifest: RobotManifest =
        serde_json::from_str(&read("assets/robots/ur5e/robot.json")).unwrap();
    let model =
        RobotModel::from_urdf_str(&read("assets/robots/ur5e/robot.urdf"), &manifest).unwrap();
    let baked = bake(&scene, &scenario, &[model]).unwrap();
    (scene, baked, vec![manifest])
}

const OPTIONS: EmitOptions = EmitOptions {
    visibility: VisibilitySpec { margin_px: 2.0 },
    pixel_centre: PixelCentre::Integer,
    laser_spacing_m: 1e-3,
};

#[test]
fn eye_in_hand_example_emits_a_valid_rig_handeye_dataset() {
    let (scene, baked, manifests) = example("eye_in_hand_ur5e");
    let points = chessboard_points();
    let bundle = emit(&scene, &baked, &manifests, &points, &OPTIONS).unwrap();
    validate(&bundle.dataset).expect("calibration-rs accepts the manifest");
    assert_eq!(bundle.dataset.topology, Topology::RigHandeye);

    let gt = &bundle.gt;
    let captures = baked.capture_indices().count();
    assert_eq!(gt.captures.len(), captures);
    assert_eq!(
        bundle
            .robot_poses
            .as_ref()
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        captures
    );
    assert_eq!(gt.rig.as_deref(), Some("rig"));

    // The rig hangs on ur5e/tool0 at +3 cm: that is the true hand-eye.
    let Some(GtHandeye::EyeInHand {
        gripper_se3_rig,
        tcp_link,
        ..
    }) = &gt.handeye
    else {
        panic!("eye-in-hand expected, got {:?}", gt.handeye);
    };
    assert_eq!(tcp_link, "tool0");
    let rig = &scene.rigs[0];
    assert!(
        (gripper_se3_rig.translation.vector - rig.parent_se3_self.translation.vector).norm()
            < 1e-12
    );
    // cam_se3_rig is the inverse of the camera mount.
    let cam = &gt.cameras[0];
    let mount = scene.cameras[0].parent_se3_self;
    assert!((cam.cam_se3_rig.unwrap() * mount).translation.vector.norm() < 1e-12);

    // Every visible point re-projects to its pixel; the board is seen at
    // every capture by both cameras.
    for (capture, c) in gt.captures.iter().enumerate() {
        for view in &c.views {
            let camera = scene.cameras.iter().find(|x| x.id == view.camera).unwrap();
            let model = camera.params.build().unwrap();
            let visible: Vec<_> = view.points.iter().filter(|p| p.visible()).collect();
            assert!(
                visible.len() >= 20,
                "capture {capture} {}: {} visible",
                view.camera,
                visible.len()
            );
            for p in visible {
                let bp = &points[p.point];
                let world =
                    c.world_se3_target * Point3::new(bp.position_m[0], bp.position_m[1], 0.0);
                let cam = view.world_se3_camera.inverse() * world;
                let px = model.project_point_c(&cam.coords).unwrap();
                let [u, v] = p.pixel.unwrap();
                assert!((px.x - u).abs() < 1e-9 && (px.y - v).abs() < 1e-9);
            }
            assert_eq!(view.image, format!("images/{}/{}.png", view.camera, c.id));
        }
    }

    // gt.json round-trips.
    let text = serde_json::to_string(gt).unwrap();
    let back: etendue_synth::dataset::GroundTruth = serde_json::from_str(&text).unwrap();
    assert_eq!(back.captures.len(), gt.captures.len());
}

#[test]
fn eye_to_hand_example_emits_an_eye_to_hand_dataset() {
    let (scene, baked, manifests) = example("eye_to_hand_ur5e");
    let bundle = emit(&scene, &baked, &manifests, &chessboard_points(), &OPTIONS).unwrap();
    validate(&bundle.dataset).expect("calibration-rs accepts the manifest");
    assert_eq!(bundle.dataset.topology, Topology::RigHandeye);
    let Some(GtHandeye::EyeToHand {
        gripper_se3_target, ..
    }) = &bundle.gt.handeye
    else {
        panic!("eye-to-hand expected");
    };
    // The board hangs on ur5e/tool0 at +1 cm.
    let target = &scene.targets[0];
    assert!(
        (gripper_se3_target.translation.vector - target.parent_se3_self.translation.vector).norm()
            < 1e-12
    );
}

#[test]
fn rejects_scenes_that_are_not_one_calibration_problem() {
    let (mut scene, baked, manifests) = example("eye_in_hand_ur5e");
    assert!(emit(&scene, &baked, &manifests, &chessboard_points(), &OPTIONS).is_ok());
    assert!(emit(&scene, &baked, &[], &chessboard_points(), &OPTIONS).is_err());
    let mut two_targets = scene.clone();
    let mut extra = two_targets.targets[0].clone();
    extra.id = "board2".into();
    two_targets.targets.push(extra);
    assert!(
        emit(
            &two_targets,
            &baked,
            &manifests,
            &chessboard_points(),
            &OPTIONS
        )
        .is_err()
    );
    scene.cameras.clear();
    assert!(emit(&scene, &baked, &manifests, &chessboard_points(), &OPTIONS).is_err());
}

/// The laser example's board points, from its layout (P3-3).
fn laser_board_points(scene: &SceneSpec) -> Vec<BoardPoint> {
    etendue_synth::board::layout(&scene.targets[0].geometry)
        .unwrap()
        .unwrap()
        .points
}

#[test]
fn laser_example_emits_a_valid_rig_handeye_laserline_dataset() {
    for name in ["laser_ur5e", "laser_scheimpflug_ur5e"] {
        let (scene, baked, manifests) = example(name);
        let points = laser_board_points(&scene);
        let bundle = emit(&scene, &baked, &manifests, &points, &OPTIONS).unwrap();
        validate(&bundle.dataset).expect("calibration-rs accepts the manifest");
        assert_eq!(bundle.dataset.topology, Topology::RigHandeyeLaserline);
        for c in &bundle.dataset.cameras {
            assert!(c.laser_images.is_some(), "{name} {}: laser images", c.id);
        }

        let gt = &bundle.gt;
        let [laser] = gt.lasers.as_slice() else {
            panic!("{name}: one laser expected");
        };
        assert_eq!(laser.rig_plane.frame, "rig");
        for c in &gt.captures {
            let [stripe] = c.laser_stripes.as_slice() else {
                panic!("{name} {}: one stripe expected", c.id);
            };
            assert!(stripe.points.len() > 100, "{name} {}: stripe", c.id);
            for view in &c.views {
                assert_eq!(
                    view.laser_image.as_deref(),
                    Some(format!("images/{}/{}_laser.png", view.camera, c.id).as_str())
                );
                let camera = scene.cameras.iter().find(|x| x.id == view.camera).unwrap();
                let model = camera.params.build().unwrap();
                let plane = laser
                    .camera_planes
                    .iter()
                    .find(|p| p.frame == view.camera)
                    .unwrap();
                let n = nalgebra::Vector3::from(plane.normal);
                let [seen] = view.laser_points.as_slice() else {
                    panic!("one stripe view expected");
                };
                let visible: Vec<_> = seen.points.iter().filter(|p| p.visible()).collect();
                assert!(visible.len() > 100, "{name} {} {}", c.id, view.camera);
                for p in visible {
                    // On the true plane in the camera frame, and re-projecting to its pixel.
                    let bp = &stripe.points[p.point];
                    let world =
                        c.world_se3_target * Point3::new(bp.position_m[0], bp.position_m[1], 0.0);
                    let cam = view.world_se3_camera.inverse() * world;
                    assert!((n.dot(&cam.coords) + plane.distance).abs() < 1e-12);
                    let px = model.project_point_c(&cam.coords).unwrap();
                    let [u, v] = p.pixel.unwrap();
                    assert!((px.x - u).abs() < 1e-9 && (px.y - v).abs() < 1e-9);
                }
            }
        }

        // gt.json round-trips, laser fields included.
        let text = serde_json::to_string(gt).unwrap();
        let back: etendue_synth::dataset::GroundTruth = serde_json::from_str(&text).unwrap();
        assert_eq!(back.lasers.len(), 1);
        assert_eq!(back.captures[0].laser_stripes.len(), 1);
    }
}

#[test]
fn rejects_laser_topologies_not_emitted_yet() {
    let (scene, baked, manifests) = example("laser_ur5e");
    let points = laser_board_points(&scene);
    assert!(emit(&scene, &baked, &manifests, &points, &OPTIONS).is_ok());
    // A laser fixed in the world is not on the rig.
    let mut fixed = scene.clone();
    fixed.lasers[0].parent = etendue_scene::FrameRef::world();
    assert!(emit(&fixed, &baked, &manifests, &points, &OPTIONS).is_err());
    // One camera: rig hand-eye laserline needs two or more.
    let mut single = scene.clone();
    single.cameras.truncate(1);
    assert!(emit(&single, &baked, &manifests, &points, &OPTIONS).is_err());
    // Two lasers.
    let mut two = scene.clone();
    let mut extra = two.lasers[0].clone();
    extra.id = "laser2".into();
    two.lasers.push(extra);
    assert!(emit(&two, &baked, &manifests, &points, &OPTIONS).is_err());
}
