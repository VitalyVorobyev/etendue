//! P20b in CI, without real data: the twin of a calibration of `closed_loop_ur5e`, built by
//! `etendue_synth::twin` from the truth, the robot poses and each view's seen points as a
//! calibration takes them (the reference camera's frame as the rig's).
//! - On exact observations it recovers its truth (T1's first half, G5.1's limits).
//! - Against a calibration with white noise of the same σ, its residuals have the same level and
//!   both are white ([`etendue_cli::real::versus`]).
//!
//! The real datasets' T1 and T2 are `tests/t1.rs` (local).

use std::path::{Path, PathBuf};

use etendue_cli::calibrate::{
    Options, RunOptions, TWIN_GT, Workflow, calibrate, read_runs, run_trials, write_twin_run,
};
use etendue_cli::progress::Control;
use etendue_cli::{bake_file, gt, load, real};
use etendue_kinematics::BakeOptions;
use etendue_scene::SceneSpec;
use etendue_synth::dataset::{GroundTruth, GtHandeye};
use etendue_synth::observe::{ObservationNoise, observe};
use etendue_synth::plan::NoiseModel;
use etendue_synth::twin::{TwinCamera, TwinView, twin};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `closed_loop_ur5e`'s ground truth (its robot manifest read from the example's folder), made
/// in `target/<dir>`.
fn truth(dir: &str) -> GroundTruth {
    let name = "closed_loop_ur5e";
    let text = std::fs::read_to_string(root().join(format!("examples/{name}/scene.json"))).unwrap();
    let mut scene: SceneSpec = serde_json::from_str(&text).unwrap();
    for r in &mut scene.robots {
        r.manifest = root()
            .join(format!("examples/{name}"))
            .join(&r.manifest)
            .to_string_lossy()
            .into_owned();
    }
    let dir = root().join("target").join(dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("scene.json");
    std::fs::write(&path, serde_json::to_string_pretty(&scene).unwrap()).unwrap();
    let loaded = load(&path).unwrap();
    let scenario = root().join(format!("examples/{name}/scenario.json"));
    let baked = bake_file(&loaded, &scenario, BakeOptions::default()).unwrap();
    gt::write(&loaded, &baked, &dir.join("gt")).unwrap();
    serde_json::from_str(&std::fs::read_to_string(dir.join("gt/gt.json")).unwrap()).unwrap()
}

/// What a calibration of `gt` takes and gives, in its frames (the rig is the first camera), as
/// the twin's inputs: the cameras, the hand-eye, and the views a camera sees the target in.
fn as_calibrated(gt: &GroundTruth) -> (Vec<TwinCamera>, GtHandeye, Vec<TwinView>) {
    let rig_se3_ref = gt.cameras[0].cam_se3_rig.unwrap().inverse();
    let cameras = gt
        .cameras
        .iter()
        .map(|c| TwinCamera {
            id: c.id.clone(),
            params: c.params.clone(),
            resolution: c.resolution,
            cam_se3_rig: c.cam_se3_rig.unwrap() * rig_se3_ref,
        })
        .collect();
    let Some(GtHandeye::EyeInHand {
        robot,
        tcp_link,
        gripper_se3_rig,
        base_se3_target,
    }) = gt.handeye.clone()
    else {
        panic!("closed_loop_ur5e is eye-in-hand");
    };
    let handeye = GtHandeye::EyeInHand {
        robot,
        tcp_link,
        gripper_se3_rig: gripper_se3_rig * rig_se3_ref,
        base_se3_target,
    };
    let views = gt
        .captures
        .iter()
        .map(|c| TwinView {
            base_se3_tcp: c.base_se3_tcp.unwrap(),
            points: c
                .views
                .iter()
                .map(|v| {
                    let seen: Vec<[f64; 3]> = v
                        .points
                        .iter()
                        .filter(|p| p.visible())
                        .map(|p| gt.target.points[p.point].xyz().coords.into())
                        .collect();
                    (seen.len() >= 4).then_some(seen)
                })
                .collect(),
        })
        .filter(|v: &TwinView| v.points.iter().any(Option::is_some))
        .collect();
    (cameras, handeye, views)
}

#[test]
fn the_twin_of_a_calibration_recovers_its_truth_and_matches_white_noise() {
    let gt = truth("twin_test");
    let (cameras, handeye, views) = as_calibrated(&gt);
    let twin_gt = twin(gt.topology, &cameras, handeye, &views).unwrap();
    assert_eq!(twin_gt.captures.len(), views.len());
    // The twin sees what the truth sees, where the truth sees it.
    let seen = |g: &GroundTruth| -> Vec<[f64; 2]> {
        g.captures
            .iter()
            .flat_map(|c| &c.views)
            .flat_map(|v| v.points.iter().filter(|p| p.visible()))
            .map(|p| p.pixel.unwrap())
            .collect()
    };
    let (ours, theirs) = (seen(&twin_gt), seen(&gt));
    assert_eq!(ours.len(), theirs.len());
    let worst = ours
        .iter()
        .zip(&theirs)
        .map(|(a, b)| (a[0] - b[0]).hypot(a[1] - b[1]))
        .fold(0.0, f64::max);
    assert!(worst < 1e-9, "the twin's pixels differ by {worst:e} px");

    // T1, first half: exact observations recover the truth within G5.1's limits.
    let exact = calibrate(
        &twin_gt,
        &observe(&twin_gt, &ObservationNoise::default()),
        &Options::default(),
    )
    .unwrap();
    print!("{}", exact.table());
    assert_eq!(exact.workflow, Workflow::RigHandeye);
    assert!(exact.gated && exact.pass);

    // A calibration with white noise (standing in for a real one), against its twin at the same
    // σ: the same residual level, and both white.
    let noise = ObservationNoise {
        pixel_sigma_px: 0.3,
        seed: 7,
        ..ObservationNoise::default()
    };
    let ctl = Control::new(&|_| {}, None);
    let options = |seed| RunOptions {
        noise: ObservationNoise { seed, ..noise },
        trials: 3,
        ..RunOptions::default()
    };
    let stand_in = run_trials(&gt, None, &options(7), &ctl).unwrap();
    let twin_run = run_trials(&twin_gt, None, &options(100), &ctl).unwrap();
    let v = real::versus(&stand_in.reports[0], &twin_gt, &twin_run);
    assert_eq!(v.cameras.len(), 2);
    for c in &v.cameras {
        println!("{c:?}");
        assert!((c.rms_ratio - 1.0).abs() < 0.05, "{c:?}");
        for w in [&c.real, &c.twin] {
            assert!((0.3..1.5).contains(&w.spatial), "{c:?}");
            assert!((0.3..1.5).contains(&w.view), "{c:?}");
        }
    }
    assert!(!v.params.is_empty());
    for p in &v.params {
        assert!(p.twin_std > 0.0 && p.distance_sigma.is_finite(), "{p:?}");
    }

    // Kept as a run (version 3, naming the real run), with the twin's ground truth beside it.
    let runs = root().join("target/twin_test/runs");
    let _ = std::fs::remove_dir_all(&runs);
    let source = real::DatasetSource {
        id: Some("kuka_1".into()),
        manifest: "data/kuka_1/dataset.toml".into(),
    };
    let kept_twin = real::Twin {
        source: source.clone(),
        real: stand_in.reports[0].clone(),
        gt: twin_gt.clone(),
        noise,
        view_scatter: None,
        observed: observe(&twin_gt, &noise),
    };
    let opts = options(100);
    write_twin_run(
        &runs.join("twin"),
        Path::new("runs/real"),
        &kept_twin,
        &opts,
        &twin_run,
    )
    .unwrap();
    let listing = read_runs(&runs).unwrap();
    assert!(listing.unreadable.is_empty(), "{:?}", listing.unreadable);
    let config = &listing.runs[0].config;
    assert_eq!(
        (config.version, config.twin.as_deref()),
        (3, Some("runs/real"))
    );
    assert_eq!(config.real.as_ref(), Some(&source));
    assert_eq!(
        (config.dataset.as_str(), &config.options),
        ("data/kuka_1", &opts)
    );
    let kept_gt: GroundTruth =
        serde_json::from_str(&std::fs::read_to_string(runs.join("twin").join(TWIN_GT)).unwrap())
            .unwrap();
    assert_eq!(kept_gt.captures.len(), twin_gt.captures.len());
}

/// P20c in CI: on the twin's own white-noise observations, the split-half σ agrees with the
/// planner's (gate P1's bar), and so does calibration-rs's covariance.
#[test]
fn on_white_noise_the_split_half_sigma_is_the_planners() {
    let gt = truth("split_half_test");
    let (cameras, handeye, views) = as_calibrated(&gt);
    let twin_gt = twin(gt.topology, &cameras, handeye, &views).unwrap();
    let noise = ObservationNoise {
        pixel_sigma_px: 0.5,
        seed: 11,
        ..ObservationNoise::default()
    };
    let obs = observe(&twin_gt, &noise);
    let ctl = Control::new(&|_| {}, None);
    let options = real::SplitHalfOptions {
        random: 4,
        ..real::SplitHalfOptions::default()
    };
    let s = real::split_half(&twin_gt, &obs, &NoiseModel::white(0.5), &options, &ctl).unwrap();
    println!("{}", serde_json::to_string_pretty(&s).unwrap());
    assert_eq!(s.procedure, "rig_handeye_refine_rig");
    assert!(s.failed.is_empty(), "{:?}", s.failed);
    assert_eq!(s.splits.len(), 6);
    assert_eq!(s.planner_mm, s.planner_exact_robot_mm);
    assert!(s.pass(), "planner / split-half = {:.3}", s.ratio());
    let real = s.real_mm.unwrap() / s.planner_mm;
    assert!((real - 1.0).abs() < 0.15, "real / planner = {real:.3}");
    assert!(!s.params.is_empty());
}
