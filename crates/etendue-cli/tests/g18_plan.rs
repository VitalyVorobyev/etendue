//! Gates G18.1 and G18.2 (PLAN P18).
//!
//! - **G18.1:** for the same number of poses, the planned ones predict a system σ no larger than
//!   the generator's grid thinned evenly, and calibrating noisy observations from them confirms
//!   it: the measured system error is within 15 % of the prediction, and below the grid's.
//! - **G18.2:** the eye-to-hand example (the target on the robot) yields a feasible tour, and the
//!   planner plans it. The G18.1 check is run on it too.
//!
//! ```bash
//! cargo test --release -p etendue-cli --test g18_plan -- --ignored --nocapture
//! ```
//!
//! `ETENDUE_G18_TRIALS` sets the trials per pose set (default 100).

use std::path::{Path, PathBuf};

use etendue_cli::calibrate::{Options, Report, calibrate};
use etendue_cli::{Loaded, bake_file, gt, load};
use etendue_kinematics::BakeOptions;
use etendue_scene::{GeneratorSpec, ProcedurePart, ScenarioSpec, SphereGenerator, Step};
use etendue_synth::dataset::GroundTruth;
use etendue_synth::observe::{ObservationNoise, observe};
use etendue_synth::plan::{EstimatedCell, PlanSpec, plan, subset, system_error};
use etendue_synth::poses::{CandidateStatus, Generated};
use vision_calibration_core::{BrownConrady5, DistortionParams, FxFyCxCy, SensorParams};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// An example's scene, the sphere generator its `spec` file keeps (a generator spec, or a
/// scenario with a `generator`), and what the generator makes of it.
fn setup(example: &str, spec: &str) -> (Loaded, SphereGenerator, Generated) {
    let dir = root().join("examples").join(example);
    let loaded = load(&dir.join("scene.json")).unwrap();
    let mut json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join(spec)).unwrap()).unwrap();
    let dt = json
        .get("dt")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.01);
    if let Some(generator) = json.get("generator") {
        json = generator.clone();
    }
    let GeneratorSpec::Sphere(sphere) = serde_json::from_value(json).unwrap();
    let generated =
        etendue_synth::poses::generate(&loaded.scene, &loaded.models, &sphere, dt).unwrap();
    (loaded, sphere, generated)
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

/// The ground truth of `example`'s scene with `scenario`.
fn truth(example: &str, scenario: &ScenarioSpec, name: &str) -> GroundTruth {
    let scene = root().join("examples").join(example).join("scene.json");
    let dir = root().join("target/g18_plan").join(example).join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("scenario.json");
    std::fs::write(&path, serde_json::to_string_pretty(scenario).unwrap()).unwrap();
    let loaded = load(&scene).unwrap();
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

fn trials() -> u64 {
    std::env::var("ETENDUE_G18_TRIALS")
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or(100)
}

/// G18.1's check on `example`: 8 planned poses against the grid thinned to 8, predicted and
/// measured over noisy calibrations. Also compares the planner's per-parameter σ with
/// calibration-rs's covariance of a calibration of the planned poses.
fn planned_poses_beat_the_grid_and_the_prediction_holds(example: &str, spec: &str) {
    let trials = trials();
    let (loaded, sphere, generated) = setup(example, spec);
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
        "{example}: predicted system σ, {budget} poses: planned {predicted:.4} mm, grid {:.4} mm",
        planned.grid_sigma_mm
    );
    assert!(predicted <= planned.grid_sigma_mm);

    let options = Options {
        procedure: Some(procedure()),
        ..Options::default()
    };
    let mut measured = Vec::new();
    for (name, keep) in [("planned", &planned.chosen), ("grid", &planned.grid)] {
        let gt = truth(example, &subset(&generated, keep, name.into()), name);
        let mut sq = 0.0;
        for k in 0..trials {
            let noise = ObservationNoise {
                pixel_sigma_px: 0.5,
                seed: 1000 + k,
                ..ObservationNoise::default()
            };
            let report = calibrate(&gt, &observe(&gt, &noise), &options).unwrap();
            if name == "planned" && k == 0 {
                compare_sigma(&planned.labels, &planned.sigma, &report);
            }
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
        println!("{example}: measured system error, {name}: {rms:.4} mm over {trials} trials");
        measured.push(rms);
    }
    let ratio = measured[0] / predicted;
    println!("{example}: planned: measured / predicted = {ratio:.3}");
    assert!((0.85..=1.15).contains(&ratio), "{ratio}");
    assert!(measured[0] <= measured[1], "{measured:?}");
}

/// The planner's σ against calibration-rs's, for every label both have.
fn compare_sigma(labels: &[String], sigma: &[f64], report: &Report) {
    let u = report
        .uncertainty
        .as_ref()
        .expect("calibration-rs's covariance");
    let mut ratios = Vec::new();
    for (label, s) in labels.iter().zip(sigma) {
        if let Some(k) = u.labels.iter().position(|l| l == label) {
            ratios.push((label.as_str(), s / u.sigma[k]));
        }
    }
    let (lo, hi) = ratios
        .iter()
        .fold((f64::INFINITY, 0.0f64), |(lo, hi), (_, r)| {
            (lo.min(*r), hi.max(*r))
        });
    println!(
        "planner σ / calibration-rs σ over {} of {} parameters: {lo:.3}–{hi:.3}",
        ratios.len(),
        labels.len()
    );
    for (label, r) in &ratios {
        if !(0.8..=1.25).contains(r) {
            println!("  {label}: {r:.3}");
        }
    }
    assert_eq!(
        ratios.len(),
        labels.len(),
        "calibration-rs names every parameter"
    );
    assert!(lo >= 0.8 && hi <= 1.25, "{lo}–{hi}");
}

#[test]
#[ignore = "a gate: hundreds of calibrations (run with --release --ignored)"]
fn g18_1_planned_poses_beat_the_grid_and_the_prediction_holds() {
    planned_poses_beat_the_grid_and_the_prediction_holds("closed_loop_ur5e", "scenario.json");
}

#[test]
#[ignore = "a gate: hundreds of calibrations (run with --release --ignored)"]
fn g18_2_eye_to_hand_planned_poses_beat_the_grid_and_the_prediction_holds() {
    planned_poses_beat_the_grid_and_the_prediction_holds("eye_to_hand_ur5e", "sphere.json");
}

#[test]
fn g18_2_the_eye_to_hand_example_yields_a_feasible_tour() {
    let (loaded, sphere, generated) = setup("eye_to_hand_ur5e", "sphere.json");
    let feasible = generated
        .candidates
        .iter()
        .filter(|c| c.status == CandidateStatus::Ok)
        .count();
    println!(
        "{feasible} of {} candidates feasible",
        generated.candidates.len()
    );
    assert!(feasible >= 24, "{feasible}");
    // The tour bakes, and its ground truth sees the whole board in both cameras at every capture.
    let gt = truth("eye_to_hand_ur5e", &generated.scenario, "tour");
    assert_eq!(gt.captures.len(), feasible);
    for capture in &gt.captures {
        assert_eq!(capture.views.len(), gt.cameras.len());
        for view in &capture.views {
            assert!(
                view.points.iter().all(|p| p.visible()),
                "{} in {}",
                view.camera,
                capture.id
            );
        }
    }
    // The planner plans it, in eye-to-hand terms.
    let planned = plan(
        &loaded.scene,
        &loaded.models,
        &generated,
        &sphere,
        &PlanSpec {
            budget: 6,
            noise_px: 0.5,
        },
    )
    .unwrap();
    assert!(planned.labels.iter().any(|l| l == "rig_se3_base.tz"));
    assert!(planned.labels.iter().any(|l| l == "gripper_se3_target.rx"));
    assert!(planned.sigma_mm.windows(2).all(|w| w[1] <= w[0]));
    assert!(*planned.sigma_mm.last().unwrap() <= planned.grid_sigma_mm);
    let captures = planned
        .scenario
        .steps
        .iter()
        .filter(|s| matches!(s, Step::Capture { .. }))
        .count();
    assert_eq!(captures, 6);
}

#[test]
fn the_planner_picks_its_budget_lowering_the_system_sigma_each_time() {
    let (loaded, sphere, generated) = setup("closed_loop_ur5e", "scenario.json");
    let spec = PlanSpec {
        budget: 5,
        noise_px: 0.5,
    };
    let planned = plan(&loaded.scene, &loaded.models, &generated, &sphere, &spec).unwrap();
    assert_eq!(planned.chosen.len(), 5);
    assert!(
        planned.sigma_mm.windows(2).all(|w| w[1] <= w[0]),
        "{:?}",
        planned.sigma_mm
    );
    assert!(*planned.sigma_mm.last().unwrap() <= planned.grid_sigma_mm);
    assert!(planned.all_sigma_mm <= *planned.sigma_mm.last().unwrap());
    // The same inputs, the same plan (no random numbers).
    let again = plan(&loaded.scene, &loaded.models, &generated, &sphere, &spec).unwrap();
    assert_eq!(again.chosen, planned.chosen);
    // The scenario holds the chosen captures only, and compiles.
    let captures = planned
        .scenario
        .steps
        .iter()
        .filter(|s| matches!(s, Step::Capture { .. }))
        .count();
    assert_eq!(captures, 5);
    etendue_kinematics::compile(&loaded.scene, &planned.scenario, &loaded.models).unwrap();
    // A budget beyond the feasible poses is refused.
    let too_many = PlanSpec {
        budget: planned.feasible + 1,
        ..spec
    };
    assert!(
        plan(
            &loaded.scene,
            &loaded.models,
            &generated,
            &sphere,
            &too_many
        )
        .is_err()
    );
}
