//! P13: `etendue calibrate` natively. The examples' exact observations recover the truth (G5.1,
//! G5.3), a single triangulation head too (the laserline device workflow), and noisy runs are
//! reproducible (G13.2).

use std::path::{Path, PathBuf};

use etendue_cli::calibrate::{Options, Workflow, calibrate};
use etendue_cli::{bake_file, gt, load};
use etendue_kinematics::BakeOptions;
use etendue_scene::{FrameRef, SceneSpec};
use etendue_synth::dataset::GroundTruth;
use etendue_synth::observe::{ObservationNoise, observe};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The ground truth of `scene` (written next to `name`) with example `example`'s scenario.
fn truth(scene: &SceneSpec, example: &str, name: &str) -> GroundTruth {
    let dir = root().join("target/calibrate_test").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("scene.json");
    std::fs::write(&path, serde_json::to_string_pretty(scene).unwrap()).unwrap();
    let loaded = load(&path).unwrap();
    let scenario = root().join(format!("examples/{example}/scenario.json"));
    let baked = bake_file(&loaded, &scenario, BakeOptions::default()).unwrap();
    gt::write(&loaded, &baked, &dir.join("gt")).unwrap();
    serde_json::from_str(&std::fs::read_to_string(dir.join("gt/gt.json")).unwrap()).unwrap()
}

fn example(name: &str) -> SceneSpec {
    let text = std::fs::read_to_string(root().join(format!("examples/{name}/scene.json"))).unwrap();
    let mut scene: SceneSpec = serde_json::from_str(&text).unwrap();
    // The scene's robot manifest paths are relative to its folder.
    for r in &mut scene.robots {
        r.manifest = root()
            .join(format!("examples/{name}"))
            .join(&r.manifest)
            .to_string_lossy()
            .into_owned();
    }
    scene
}

#[test]
fn a_rig_recovers_its_truth() {
    let gt = truth(&example("closed_loop_ur5e"), "closed_loop_ur5e", "rig");
    let report = calibrate(
        &gt,
        &observe(&gt, &ObservationNoise::default()),
        &Options::default(),
    )
    .unwrap();
    print!("{}", report.table());
    assert_eq!(report.workflow, Workflow::RigHandeye);
    assert!(report.gated && report.pass);
}

#[test]
fn a_single_head_recovers_its_plane_and_hand_eye() {
    // laser_ur5e with one camera, the laser on its body: a triangulation head.
    let mut scene = example("laser_ur5e");
    scene.cameras.retain(|c| c.id == "cam_left");
    let cam = scene.cameras[0].clone();
    let laser = &mut scene.lasers[0];
    laser.parent_se3_self = cam.parent_se3_self.inverse() * laser.parent_se3_self;
    laser.parent = FrameRef::from(cam.id.as_str());
    laser.camera = Some(cam.id.clone());
    let gt = truth(&scene, "laser_ur5e", "head");
    let report = calibrate(
        &gt,
        &observe(&gt, &ObservationNoise::default()),
        &Options::default(),
    )
    .unwrap();
    print!("{}", report.table());
    assert_eq!(report.workflow, Workflow::LaserlineDevice);
    assert!(
        report
            .rows
            .iter()
            .any(|r| r.label == "cam_left laser normal deg")
    );
    assert!(report.gated && report.pass);
}

#[test]
fn g13_2_the_same_seed_gives_the_same_report() {
    let gt = truth(&example("closed_loop_ur5e"), "closed_loop_ur5e", "seeded");
    let noise = ObservationNoise {
        pixel_sigma_px: 0.5,
        seed: 4,
        ..ObservationNoise::default()
    };
    let run = || {
        let r = calibrate(&gt, &observe(&gt, &noise), &Options::default()).unwrap();
        serde_json::to_string(&r).unwrap()
    };
    let first = run();
    assert_eq!(first, run());
    let report: etendue_cli::calibrate::Report = serde_json::from_str(&first).unwrap();
    assert!(!report.gated && report.noise.is_some());
}
