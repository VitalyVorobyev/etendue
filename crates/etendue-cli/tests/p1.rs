//! Gate P1 (P20c): the planner on real poses.
//!
//! For each dataset (`kuka_1` and `DS8`, or the ids in `ETENDUE_P1_DATASETS`, resolved as
//! `etendue real calibrate` resolves them from the workspace root), this calibrates the real
//! dataset (detections cached in `target/p1/<id>/real/.detections`), builds its twin, and:
//! - **P1:** the planner's system σ, predicted on the twin, is within 25 % of the split-half σ of
//!   the real views ([`real::split_half_run`]), the calibrations being the planner's model (a rig
//!   with its rig refined, as T1 judges it). A rig is also run with calibration-rs's default
//!   workflow (the rig held), recorded beside it, not gated.
//! - **The estimator on the twin:** the same split-half check on the twin's own observations
//!   (the real run's pixel noise and robot pose error, white), over `ETENDUE_P1_SEEDS` noise draws
//!   (default 5, seeds 200, 201, …): what the split-half σ gives where the planner's model holds,
//!   so how far it scatters about the planner's σ by chance.
//! - prints the tables of `docs/measurements/p20c_planner.md`.
//!
//! The data is local, so this is ignored:
//! `cargo test --release -p etendue-cli --test p1 -- --ignored --nocapture`.

use std::path::{Path, PathBuf};

use etendue_cli::calibrate::{self, RunProcedure};
use etendue_cli::progress::Control;
use etendue_cli::real::{self, SplitHalf, SplitHalfOptions};
use etendue_synth::observe::{ObservationNoise, observe};
use etendue_synth::plan::RobotPrior;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn opt(x: Option<f64>, digits: usize) -> String {
    x.map_or("—".into(), |x| format!("{x:.digits$}"))
}

/// calibration-rs's default rig hand-eye (the rig held): the real run's workflow.
fn default_rig() -> RunProcedure {
    RunProcedure {
        id: "auto_rig".into(),
        procedure: serde_json::from_value(serde_json::json!({
            "stages": [{"id": "rig_handeye", "problem": "rig_handeye"}]
        }))
        .unwrap(),
    }
}

fn print(id: &str, s: &SplitHalf) {
    println!(
        "{id}, {}: {} views; planner noise {:.4} px, robot {}",
        s.procedure,
        s.views,
        s.noise_px,
        s.robot.map_or("exact".into(), |r| format!(
            "{:.3e} rad, {:.3e} m",
            r.rot_sigma_rad, r.trans_sigma_m
        ))
    );
    println!(
        "\n| Split | Views | Halves differ (mm) | Predicted (mm) | Ratio |\n|---|---|---|---|---|"
    );
    for r in &s.splits {
        println!(
            "| {} | {} + {} | {:.4} | {:.4} | {:.2} |",
            r.split,
            r.views[0],
            r.views[1],
            r.measured_mm,
            r.predicted_mm,
            r.measured_mm / r.predicted_mm
        );
    }
    for f in &s.failed {
        println!("left out: {f}");
    }
    let scatter = s.view_scatter;
    println!(
        "\nSystem σ: planner {:.4} mm (robot exact: {:.4}), real {} mm, split-half {:.4} mm; \
         planner / split-half {:.3} → {}; view scatter {} (planner × √ = {} mm, split-half / that {}).\n",
        s.planner_mm,
        s.planner_exact_robot_mm,
        opt(s.real_mm, 4),
        s.split_half_mm,
        s.ratio(),
        if s.pass() { "PASS" } else { "FAIL" },
        opt(scatter, 2),
        opt(scatter.map(|v| s.planner_mm * v.sqrt()), 4),
        opt(
            scatter.map(|v| s.split_half_mm / (s.planner_mm * v.sqrt())),
            2
        ),
    );
    println!(
        "| Group | Unit | planner σ | real σ | split-half σ | real / planner | split-half / planner |\n|---|---|---|---|---|---|---|"
    );
    for g in s.groups() {
        println!(
            "| {} | {} | {} | {} | {} | {} | {:.2} |",
            g.group,
            g.unit,
            opt(g.planner, 4),
            opt(g.real, 4),
            opt(g.split_half, 4),
            opt(g.real_ratio, 2),
            g.split_ratio
        );
    }
    println!();
}

/// P1 on `id`; the failures, as messages.
fn dataset(id: &str, seeds: u64) -> Vec<String> {
    let source = real::resolve(id, &root()).unwrap();
    let real_run = root().join("target/p1").join(id).join("real");
    let run = real::calibrate(&source, &real_run).unwrap();
    calibrate::write_real_run(&real_run, &source, &run).unwrap();
    let ctl = Control::new(&|_| {}, None);
    println!("## {id}\n");

    let (twin, s) = real::split_half_run(&real_run, &SplitHalfOptions::default(), &ctl).unwrap();
    print(id, &s);
    let mut failures = Vec::new();
    if !s.pass() {
        failures.push(format!("{id}: planner / split-half = {:.3}", s.ratio()));
    }
    if twin.gt.cameras.len() > 1 {
        let options = SplitHalfOptions {
            procedure: Some(default_rig()),
            ..SplitHalfOptions::default()
        };
        let (_, d) = real::split_half_run(&real_run, &options, &ctl).unwrap();
        println!("Default workflow (not gated):\n");
        print(id, &d);
    }

    // The estimator where the planner's model holds: the twin's own white observations.
    let n = twin.noise;
    let robot = RobotPrior {
        rot_sigma_rad: n.robot_rot_sigma_rad,
        trans_sigma_m: n.robot_trans_sigma_m,
    };
    println!(
        "The estimator on the twin (white noise at the real run's level), {seeds} draws:\n\n\
         | Seed | planner (mm) | real (mm) | split-half (mm) | planner / split-half |\n|---|---|---|---|---|"
    );
    let mut ratios = Vec::new();
    for k in 0..seeds {
        let obs = observe(&twin.gt, &ObservationNoise { seed: 200 + k, ..n });
        let t = real::split_half(
            &twin.gt,
            &obs,
            n.pixel_sigma_px,
            Some(robot),
            &SplitHalfOptions::default(),
            &ctl,
        )
        .unwrap();
        println!(
            "| {} | {:.4} | {} | {:.4} | {:.3} |",
            200 + k,
            t.planner_mm,
            opt(t.real_mm, 4),
            t.split_half_mm,
            t.ratio()
        );
        ratios.push(t.ratio());
    }
    let mean = ratios.iter().sum::<f64>() / ratios.len() as f64;
    let sd = (ratios.iter().map(|r| (r - mean).powi(2)).sum::<f64>()
        / (ratios.len() as f64 - 1.0).max(1.0))
    .sqrt();
    println!("\nplanner / split-half on the twin: mean {mean:.3}, std {sd:.3}\n");
    failures
}

#[test]
#[ignore = "local: real datasets (DS8's images are not in git)"]
fn p1_the_planners_sigma_is_the_split_half_sigma() {
    let ids = std::env::var("ETENDUE_P1_DATASETS").unwrap_or_else(|_| "kuka_1,DS8".into());
    let seeds = std::env::var("ETENDUE_P1_SEEDS")
        .ok()
        .map_or(5, |t| t.parse().unwrap());
    let failures: Vec<String> = ids
        .split(',')
        .flat_map(|id| dataset(id.trim(), seeds))
        .collect();
    assert!(failures.is_empty(), "P1 fails: {failures:#?}");
}
