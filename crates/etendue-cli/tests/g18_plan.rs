//! Gate G18.1 (PLAN P18): for the same number of poses, the planned ones predict a system σ no
//! larger than the generator's grid thinned evenly, and calibrating noisy observations from them
//! confirms it: the measured system error is within 15 % of the prediction, and below the
//! grid's.
//!
//! ```bash
//! cargo test --release -p etendue-cli --test g18_plan -- --ignored --nocapture
//! ```
//!
//! `ETENDUE_G18_TRIALS` sets the trials per pose set (default 100).

use std::path::{Path, PathBuf};

use etendue_cli::calibrate::{Options, Report, calibrate};
use etendue_cli::{bake_file, gt, load};
use etendue_kinematics::BakeOptions;
use etendue_scene::{GeneratorSpec, ProcedurePart, ScenarioSpec};
use etendue_synth::dataset::GroundTruth;
use etendue_synth::observe::{ObservationNoise, observe};
use etendue_synth::plan::{EstimatedCell, PlanSpec, plan, subset, system_error};
use vision_calibration_core::{BrownConrady5, DistortionParams, FxFyCxCy, SensorParams};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The planner's model of the calibration: the rig hand-eye with the rig refined and the robot
/// poses exact.
fn procedure() -> (String, ProcedurePart) {
    let part: ProcedurePart = serde_json::from_value(serde_json::json!({
        "stages": [{ "id": "rig_handeye", "problem": "rig_handeye",
                     "free": { "refine_rig": true, "refine_robot_poses": false } }]
    }))
    .unwrap();
    ("planner_model".into(), part)
}

/// The ground truth of the example's scene with `scenario`.
fn truth(scenario: &ScenarioSpec, name: &str) -> GroundTruth {
    let example = root().join("examples/closed_loop_ur5e");
    let dir = root().join("target/g18_plan").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("scenario.json");
    std::fs::write(&path, serde_json::to_string_pretty(scenario).unwrap()).unwrap();
    let loaded = load(&example.join("scene.json")).unwrap();
    let baked = bake_file(&loaded, &path, BakeOptions::default()).unwrap();
    gt::write(&loaded, &baked, &dir.join("gt")).unwrap();
    serde_json::from_str(&std::fs::read_to_string(dir.join("gt/gt.json")).unwrap()).unwrap()
}

/// The estimated cell a report gives, in the planner's terms.
fn cell(gt: &GroundTruth, report: &Report) -> EstimatedCell {
    let cameras = report
        .estimate
        .cameras
        .iter()
        .map(|e| {
            let truth = gt.cameras.iter().find(|c| c.id == e.id).unwrap();
            let mut p = truth.params.clone();
            let [fx, fy, cx, cy] = e.intrinsics;
            p.intrinsics = FxFyCxCy { fx, fy, cx, cy };
            let [k1, k2, k3, p1, p2] = e.distortion;
            p.distortion = DistortionParams::BrownConrady5 {
                params: BrownConrady5 {
                    k1,
                    k2,
                    k3,
                    p1,
                    p2,
                    iters: 8,
                },
            };
            if let SensorParams::Scheimpflug { params } = &mut p.sensor {
                params.tilt_x = e.tilt[0];
                params.tilt_y = e.tilt[1];
            }
            (p, e.cam_se3_ref)
        })
        .collect();
    EstimatedCell {
        cameras,
        handeye: report.estimate.handeye.unwrap(),
    }
}

#[test]
#[ignore = "a gate: hundreds of calibrations (run with --release --ignored)"]
fn g18_1_planned_poses_beat_the_grid_and_the_prediction_holds() {
    let trials: u64 = std::env::var("ETENDUE_G18_TRIALS")
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or(100);
    let example = root().join("examples/closed_loop_ur5e");
    let loaded = load(&example.join("scene.json")).unwrap();
    let scenario: ScenarioSpec =
        serde_json::from_str(&std::fs::read_to_string(example.join("scenario.json")).unwrap())
            .unwrap();
    let Some(GeneratorSpec::Sphere(sphere)) = scenario.generator.clone() else {
        panic!("the example's scenario keeps its generator");
    };
    let generated =
        etendue_synth::poses::generate(&loaded.scene, &loaded.models, &sphere, scenario.dt)
            .unwrap();
    let budget = 8;
    let planned = plan(
        &loaded.scene,
        &loaded.models,
        &generated,
        &sphere,
        &PlanSpec {
            budget,
            noise_px: 0.5,
        },
    )
    .unwrap();
    let predicted = *planned.sigma_mm.last().unwrap();
    println!(
        "predicted system σ, {budget} poses: planned {predicted:.4} mm, grid {:.4} mm",
        planned.grid_sigma_mm
    );
    assert!(predicted <= planned.grid_sigma_mm);

    let mut measured = Vec::new();
    for (name, keep) in [("planned", &planned.chosen), ("grid", &planned.grid)] {
        let gt = truth(&subset(&generated, keep, name.into()), name);
        let options = Options {
            procedure: Some(procedure()),
            ..Options::default()
        };
        let mut sq = 0.0;
        for k in 0..trials {
            let noise = ObservationNoise {
                pixel_sigma_px: 0.5,
                seed: 1000 + k,
                ..ObservationNoise::default()
            };
            let report = calibrate(&gt, &observe(&gt, &noise), &options).unwrap();
            let e = system_error(
                &loaded.scene,
                &loaded.models,
                &generated,
                &sphere,
                &cell(&gt, &report),
            )
            .unwrap();
            sq += e * e;
        }
        let rms = (sq / trials as f64).sqrt();
        println!("measured system error, {name}: {rms:.4} mm over {trials} trials");
        measured.push(rms);
    }
    let ratio = measured[0] / predicted;
    println!("planned: measured / predicted = {ratio:.3}");
    assert!((0.85..=1.15).contains(&ratio), "{ratio}");
    assert!(measured[0] <= measured[1], "{measured:?}");
}
