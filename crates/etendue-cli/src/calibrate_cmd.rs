//! `etendue calibrate`: a dataset's calibration, printed as `closed_loop.py` prints it.

use std::path::Path;

use anyhow::{Context, Result, anyhow};
use etendue_cli::{calibrate, detect, read_json};

/// `etendue calibrate`: one run (or `trials`), printed as `closed_loop.py` prints it; fails when
/// a gated run fails.
pub fn run_calibrate(
    dir: &Path,
    noise: Option<&Path>,
    trials: u32,
    (camera, procedure): (Option<String>, Option<String>),
    (features, laser_features): (Option<&Path>, Option<&Path>),
    (output, run): (Option<&Path>, Option<&Path>),
) -> Result<()> {
    use etendue_synth::dataset::GroundTruth;
    use etendue_synth::observe::ObservationNoise;
    let gt: GroundTruth = read_json(&dir.join("gt.json"), "ground truth")?;
    let noise: ObservationNoise = match noise {
        Some(p) => read_json(p, "observation noise")?,
        None => ObservationNoise::default(),
    };
    if features.is_some() && (noise != ObservationNoise::default() || trials > 1) {
        return Err(anyhow!(
            "--noise and --trials apply to the analytic observations, not to --features"
        ));
    }
    let procedure = procedure
        .map(|name| etendue_cli::procedure::named(&name))
        .transpose()?;
    if let Some((id, p)) = &procedure {
        println!("procedure: {id} ({} stage(s))", p.stages.len());
    }
    let detected = features
        .map(|f| {
            let laser = laser_features
                .map(Path::to_path_buf)
                .unwrap_or_else(|| f.with_file_name(detect::LASER_FEATURES));
            calibrate::read_detected(&gt, f, &laser)
        })
        .transpose()?;
    let options = calibrate::RunOptions {
        noise,
        trials: trials.max(1),
        camera,
        detected: features.is_some(),
        procedure: procedure.map(|(id, procedure)| calibrate::RunProcedure { id, procedure }),
    };
    let doc = calibrate::run_trials(
        &gt,
        detected,
        &options,
        &etendue_cli::progress::Control::new(&|_| {}, None),
    )?;
    let reports = &doc.reports;
    let report = &reports[0];
    println!(
        "{}: {} ({} camera(s): {}; {} views, {} points, {} laser pixels){}",
        dir.display(),
        report.workflow,
        report.cameras.len(),
        report.cameras.join(", "),
        report.views,
        report.points,
        report.laser_pixels,
        if report.noise.is_some() {
            format!(", noise seed {}", noise.seed)
        } else {
            String::new()
        },
    );
    print!("{}", report.table());
    println!(
        "  reprojection: mean {:.4} px, rms per axis {:.4} px over {} points",
        report.residuals.mean_px, report.residuals.rms_axis_px, report.residuals.count
    );
    if let Some(stats) = &doc.row_stats {
        println!("{} trials:", reports.len());
        for s in stats {
            println!(
                "  {:<28} mean {:10.3e}  std {:10.3e}  max {:10.3e}",
                s.label, s.mean, s.std, s.max
            );
        }
    }
    let failed = reports
        .iter()
        .any(|r| r.gated && !r.pass)
        .then(|| reports[0].check_name());
    if let Some(params) = &doc.param_stats {
        println!(
            "parameters over {} trials (predicted σ: calibration-rs's covariance):",
            reports.len()
        );
        for p in params {
            let predicted = p.predicted_sigma.map_or(String::new(), |s| {
                format!("  predicted σ {s:10.3e}  std/σ {:5.2}", p.std / s)
            });
            println!(
                "  {:<22} {:>3}  truth {:11.4e}  bias {:10.3e}  std {:10.3e}{predicted}",
                p.label, p.unit, p.truth, p.bias, p.std
            );
        }
    }
    if let Some(out) = output {
        std::fs::write(out, serde_json::to_string_pretty(&doc)? + "\n")
            .with_context(|| format!("writing {}", out.display()))?;
        println!("report → {}", out.display());
    }
    if let Some(folder) = run {
        calibrate::write_run(folder, dir, &options, &doc)?;
        println!("run → {}", folder.display());
    }
    if let Some(check) = failed {
        return Err(anyhow!(
            "{check} failed: the calibration did not recover the ground truth within tolerance"
        ));
    }
    Ok(())
}
