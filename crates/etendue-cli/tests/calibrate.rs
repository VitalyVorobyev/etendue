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

#[test]
fn a_dataset_directory_runs_trials_and_keeps_the_run() {
    use etendue_cli::calibrate::{CALIBRATION, Run, RunOptions, run_dir};
    use etendue_cli::progress::Control;
    use std::sync::atomic::AtomicBool;

    truth(&example("closed_loop_ur5e"), "closed_loop_ur5e", "trials");
    let dir = root().join("target/calibrate_test/trials/gt");
    let options = RunOptions {
        noise: ObservationNoise {
            pixel_sigma_px: 0.3,
            seed: 9,
            ..ObservationNoise::default()
        },
        trials: 3,
        ..RunOptions::default()
    };
    let steps = std::sync::Mutex::new(Vec::new());
    let report = |p: etendue_cli::progress::Progress| steps.lock().unwrap().push(p);
    let run = run_dir(&dir, &options, &Control::new(&report, None)).unwrap();
    assert_eq!(run.reports.len(), 3);
    let stats = run.row_stats.as_ref().unwrap();
    assert_eq!(stats.len(), run.reports[0].rows.len());
    let written: Run =
        serde_json::from_str(&std::fs::read_to_string(dir.join(CALIBRATION)).unwrap()).unwrap();
    assert_eq!(written.reports.len(), 3);
    assert!(
        steps.lock().unwrap().len() >= 4,
        "a step per trial and the end"
    );
    // Cancelled before the first trial.
    let cancel = AtomicBool::new(true);
    let err = run_dir(&dir, &options, &Control::new(&report, Some(&cancel))).unwrap_err();
    assert!(
        err.downcast_ref::<etendue_cli::progress::Cancelled>()
            .is_some()
    );
}

fn row(report: &etendue_cli::calibrate::Report, label: &str) -> f64 {
    report
        .rows
        .iter()
        .find(|r| r.label == label)
        .unwrap_or_else(|| panic!("no row `{label}`"))
        .value
}

#[test]
fn exact_offsets_are_identities_and_the_planes_agree() {
    let gt = truth(&example("laser_ur5e"), "laser_ur5e", "offsets");
    let report = calibrate(
        &gt,
        &observe(&gt, &ObservationNoise::default()),
        &Options::default(),
    )
    .unwrap();
    let offsets = report.estimate.offsets.expect("a hand-eye and a target");
    assert_eq!(offsets.cameras.len(), report.cameras.len());
    for o in offsets.cameras.iter().chain([&offsets.target]) {
        assert!(o.rotation.angle() < 1e-6, "{o:?}");
        assert!(o.translation.vector.norm() < 1e-6, "{o:?}");
    }
    assert_eq!(offsets.planes.len(), report.cameras.len());
    for [t, e] in offsets
        .planes
        .iter()
        .map(|p| p.expect("a laser per camera"))
    {
        let (nt, ne) = (
            nalgebra::Vector3::from(t.normal),
            nalgebra::Vector3::from(e.normal),
        );
        assert!((nt.norm() - 1.0).abs() < 1e-12 && nt.dot(&ne) > 1.0 - 1e-10);
        assert!((t.distance - e.distance).abs() < 1e-6);
    }
}

#[test]
fn the_reference_cameras_offset_is_the_hand_eye_error() {
    // Eye-in-hand: the reference camera's offset is gripper_se3_camera's error, in its own frame.
    let gt = truth(
        &example("closed_loop_ur5e"),
        "closed_loop_ur5e",
        "offsets_noisy",
    );
    let noise = ObservationNoise {
        pixel_sigma_px: 0.5,
        seed: 3,
        ..ObservationNoise::default()
    };
    let report = calibrate(&gt, &observe(&gt, &noise), &Options::default()).unwrap();
    let o = report.estimate.offsets.as_ref().unwrap().cameras[0];
    let (deg, mm) = (
        row(&report, "hand-eye rotation deg"),
        row(&report, "hand-eye translation mm"),
    );
    assert!(deg > 1e-4, "noise moves the estimate: {deg}");
    assert!((o.rotation.angle().to_degrees() - deg).abs() < 1e-9 * deg.max(1.0));
    assert!((o.translation.vector.norm() * 1e3 - mm).abs() < 1e-9 * mm.max(1.0));
}

#[test]
fn eye_to_hand_offsets_place_the_static_camera() {
    // Eye-to-hand: the camera's offset is camera_se3_base's error moved into the camera frame,
    // the same rotation; exact data gives identities.
    let gt = truth(
        &example("eye_to_hand_ur5e"),
        "eye_to_hand_ur5e",
        "offsets_eth",
    );
    let exact = calibrate(
        &gt,
        &observe(&gt, &ObservationNoise::default()),
        &Options::default(),
    )
    .unwrap();
    for o in &exact.estimate.offsets.as_ref().unwrap().cameras {
        assert!(
            o.rotation.angle() < 1e-6 && o.translation.vector.norm() < 1e-5,
            "{o:?}"
        );
    }
    let noise = ObservationNoise {
        pixel_sigma_px: 0.5,
        seed: 5,
        ..ObservationNoise::default()
    };
    let report = calibrate(&gt, &observe(&gt, &noise), &Options::default()).unwrap();
    let o = report.estimate.offsets.as_ref().unwrap().cameras[0];
    let deg = row(&report, "hand-eye rotation deg");
    assert!(deg > 1e-4);
    assert!((o.rotation.angle().to_degrees() - deg).abs() < 1e-9 * deg.max(1.0));
}
