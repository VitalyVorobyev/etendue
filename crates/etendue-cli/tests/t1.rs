//! Gate T1 and characterisation T2 (P20b): the twin of a real calibration.
//!
//! For each dataset (`kuka_1` and `DS8`, or the ids in `ETENDUE_T1_DATASETS`, resolved as
//! `etendue real calibrate` resolves them from the workspace root), this calibrates the real
//! dataset (its detections cached in `target/t1/<id>/real/.detections`), builds its twin, and:
//! - **T1, exact:** the twin's exact observations recover its truth within G5.1's limits;
//! - **T1, scatter:** over `ETENDUE_T1_TRIALS` trials (default 200, seeds 100, 101, …) at the
//!   noise the real run implies, the std of each camera's fx and cx and of the hand-eye's six
//!   components is within 15 % of calibration-rs's predicted σ (G17.1's parameters and bar), as
//!   G17.1 is judged: a rig's trials are one `rig_handeye` stage with the cameras' rig poses
//!   refined ([`scatter_procedure`]), since the default holds them and its covariance is
//!   conditional on them; a single camera's are its own workflow, which holds nothing;
//! - **T2:** prints the real run against its twin ([`etendue_cli::real::versus`]) as the tables of
//!   `docs/measurements/p20_twin.md`.
//!
//! `ETENDUE_T1_PROCEDURE` (a procedure id or file, as `etendue calibrate --procedure` takes it)
//! calibrates the twin's noisy trials with another procedure: a diagnostic, not T1 (a plain
//! `rig_handeye` stage gives the default workflow's understated σ, the finding of
//! `docs/measurements/p20_twin.md`).
//!
//! The data is local (DS8's images are not in git), so this is ignored:
//! `cargo test --release -p etendue-cli --test t1 -- --ignored --nocapture`.

use std::path::{Path, PathBuf};

use etendue_cli::calibrate::{self, ParamStats, RunOptions, run_trials};
use etendue_cli::progress::Control;
use etendue_cli::real;
use etendue_synth::observe::ObservationNoise;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// T1's scatter bar: G17.1's.
const SCATTER: f64 = 0.15;

/// G17.1's parameters: each camera's fx and cx, and the hand-eye.
fn gated(p: &ParamStats) -> bool {
    p.label.ends_with(" fx") || p.label.ends_with(" cx") || p.label.starts_with("hand-eye ")
}

fn opt(x: Option<f64>) -> String {
    x.map_or("—".into(), |x| format!("{x:.3e}"))
}

/// The procedure of T1's noisy trials, as G17.1 judges the scatter: for a rig, one `rig_handeye`
/// stage with `"free": {"refine_rig": true}` (`docs/measurements/g17_sim_lab.md`); for a single
/// camera, `auto`'s (`None`), which has no rig to hold.
fn scatter_procedure(cameras: usize) -> Option<calibrate::RunProcedure> {
    (cameras > 1).then(|| calibrate::RunProcedure {
        id: "rig_handeye_refine_rig".into(),
        procedure: serde_json::from_value(serde_json::json!({
            "stages": [{"id": "rig_handeye", "problem": "rig_handeye", "free": {"refine_rig": true}}]
        }))
        .unwrap(),
    })
}

/// T1 and T2 on `id`; the T1 failures, as messages.
fn dataset(id: &str, trials: u32) -> Vec<String> {
    let source = real::resolve(id, &root()).unwrap();
    let dir = root().join("target/t1").join(id);
    let real_run = dir.join("real");
    let run = real::calibrate(&source, &real_run).unwrap();
    calibrate::write_real_run(&real_run, &source, &run).unwrap();
    let twin = real::twin(&real_run).unwrap();
    let ctl = Control::new(&|_| {}, None);
    let mut failures = Vec::new();

    let exact = run_trials(&twin.gt, None, &RunOptions::default(), &ctl).unwrap();
    println!(
        "## {id}\n\nT1, exact observations:\n\n```\n{}```\n",
        exact.reports[0].table()
    );
    if !(exact.reports[0].gated && exact.reports[0].pass) {
        failures.push(format!(
            "{id}: the twin's exact observations miss its truth"
        ));
    }

    let options = RunOptions {
        noise: ObservationNoise {
            seed: 100,
            ..twin.noise
        },
        trials,
        procedure: match std::env::var("ETENDUE_T1_PROCEDURE") {
            Ok(name) => {
                let (id, procedure) = etendue_cli::procedure::named(&name).unwrap();
                println!("Procedure of the noisy trials: {id} (a diagnostic, not T1).\n");
                Some(calibrate::RunProcedure { id, procedure })
            }
            Err(_) => scatter_procedure(twin.gt.cameras.len()),
        },
        ..RunOptions::default()
    };
    let noisy = run_trials(&twin.gt, None, &options, &ctl).unwrap();
    let n = twin.noise;
    println!(
        "Noise: {:.4} px per axis; robot {:.3e} rad, {:.3e} m. {} views, {} points, {trials} trials.\n",
        n.pixel_sigma_px,
        n.robot_rot_sigma_rad,
        n.robot_trans_sigma_m,
        twin.gt.captures.len(),
        noisy.reports[0].points,
    );
    println!("T1, scatter (std / predicted σ; gated: fx, cx, hand-eye):\n");
    println!(
        "| Parameter | Unit | std | predicted σ | std / σ | gated |\n|---|---|---|---|---|---|"
    );
    for p in noisy.param_stats.as_ref().unwrap() {
        let Some(sigma) = p.predicted_sigma.filter(|_| p.std > 0.0) else {
            continue;
        };
        let ratio = p.std / sigma;
        let gate = gated(p);
        println!(
            "| {} | {} | {:.3e} | {sigma:.3e} | {ratio:.2} | {} |",
            p.label,
            p.unit,
            p.std,
            if gate { "yes" } else { "" }
        );
        if gate && (ratio - 1.0).abs() > SCATTER {
            failures.push(format!("{id}: {} std/σ = {ratio:.3}", p.label));
        }
    }

    let v = real::versus(&twin.real, &twin.gt, &noisy);
    println!("\nT2, residuals (real / twin; whiteness 1 when white):\n");
    println!(
        "| Camera | Points | RMS real (px) | RMS twin (px) | Ratio | Across the image | Within views \
         | Largest bin mean (px) | Radial, centre → edge (px) |\n|---|---|---|---|---|---|---|---|---|"
    );
    for c in &v.cameras {
        let radial = |r: &[f64; 4]| {
            r.iter()
                .map(|x| format!("{x:+.3}"))
                .collect::<Vec<_>>()
                .join(" ")
        };
        println!(
            "| {} | {} | {:.4} | {:.4} | {:.2} | {:.2} / {:.2} | {:.2} / {:.2} | {:.3} / {:.3} | {} / {} |",
            c.camera,
            c.real.points,
            c.real.rms_axis_px,
            c.twin.rms_axis_px,
            c.rms_ratio,
            c.real.spatial,
            c.twin.spatial,
            c.real.view,
            c.twin.view,
            c.real.max_bin_px,
            c.twin.max_bin_px,
            radial(&c.real.radial_px),
            radial(&c.twin.radial_px),
        );
    }
    println!(
        "\nT2, parameters (real σ is calibration-rs's on the real data; distance = (real − twin mean) / twin std):\n"
    );
    println!(
        "| Parameter | Unit | real σ | twin σ | twin std | real σ / twin std | distance (σ) |\n|---|---|---|---|---|---|---|"
    );
    for p in &v.params {
        println!(
            "| {} | {} | {} | {} | {:.3e} | {} | {:+.2} |",
            p.label,
            p.unit,
            opt(p.real_sigma),
            opt(p.twin_sigma),
            p.twin_std,
            p.real_sigma
                .map_or("—".into(), |s| format!("{:.2}", s / p.twin_std)),
            p.distance_sigma
        );
    }
    if let Some(scatter) = twin.view_scatter {
        println!(
            "\nThe real covariance's view scatter (calibration-rs's variance factor): {scatter:.2}."
        );
    }
    println!();
    failures
}

#[test]
#[ignore = "local: real datasets (DS8's images are not in git)"]
fn t1_the_twin_recovers_its_truth_and_its_scatter_is_its_sigma() {
    let ids = std::env::var("ETENDUE_T1_DATASETS").unwrap_or_else(|_| "kuka_1,DS8".into());
    let trials = std::env::var("ETENDUE_T1_TRIALS")
        .ok()
        .map_or(200, |t| t.parse().unwrap());
    let failures: Vec<String> = ids
        .split(',')
        .flat_map(|id| dataset(id.trim(), trials))
        .collect();
    assert!(failures.is_empty(), "T1 fails: {failures:#?}");
}
