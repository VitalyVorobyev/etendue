//! `etendue real …`: real datasets (calibration-rs's `dataset.toml` manifests, P20).

use std::path::PathBuf;

use anyhow::Result;
use clap::Subcommand;
use etendue_cli::calibrate::{self, Report};
use etendue_cli::real;

/// `etendue real …`.
#[derive(Subcommand)]
pub enum RealCommand {
    /// Calibrate a real dataset with calibration-rs's dataset runner (detection, then the
    /// manifest's workflow) and keep the run: residuals, σ, the quality verdict and the views
    /// left out. There is no truth, so there is no closed-loop check.
    Calibrate {
        /// The dataset: a manifest (`dataset.toml`) or its folder, an id of
        /// `local/datasets.toml`, or a public id (`kuka_1`, `DS8`: in `../calibration-rs/data`).
        dataset: String,
        /// The run's folder (a project's `runs/<name>`): config.json, report.json and
        /// calibration-rs's export as result.json, with the detection cache in `.detections`.
        #[arg(short, long)]
        output: PathBuf,
    },
}

pub fn run(command: RealCommand) -> Result<()> {
    match command {
        RealCommand::Calibrate { dataset, output } => {
            let source = real::resolve(&dataset, &std::env::current_dir()?)?;
            match &source.id {
                Some(id) => println!("{id}: {}", source.manifest),
                None => println!("{}", source.manifest),
            }
            let run = real::calibrate(&source, &output)?;
            print(&run.reports[0]);
            calibrate::write_real_run(&output, &source, &run)?;
            println!("run → {}", output.display());
            Ok(())
        }
    }
}

/// A JSON enum's name (`warn`, `view_outliers`).
fn name(v: &impl serde::Serialize) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn print(report: &Report) {
    let Some(summary) = &report.real else {
        return;
    };
    println!(
        "{} ({}): {} of {} views used",
        report.workflow, summary.topology, summary.usable_views, summary.total_views
    );
    for r in &summary.rejected {
        println!("  left out {} {}: {}", r.camera, r.image, r.reason);
    }
    let sigma = |i: usize, label: &str| {
        let u = report.uncertainty.as_ref()?;
        let at = |l: &str| u.labels.iter().position(|x| x == l);
        let k = at(&format!("cam{i}.{label}")).or_else(|| at(label))?;
        u.sigma.get(k).copied()
    };
    for (i, cam) in report.estimate.cameras.iter().enumerate() {
        println!("camera {}:", cam.id);
        for (label, value) in ["fx", "fy", "cx", "cy"].iter().zip(cam.intrinsics) {
            match sigma(i, label) {
                Some(s) => println!("  {label} = {value:.3} ± {s:.3} px"),
                None => println!("  {label} = {value:.3} px"),
            }
        }
    }
    let r = &report.residuals;
    println!(
        "reprojection: mean {:.4} px, rms per axis {:.4} px, max {:.4} px over {} points",
        r.mean_px, r.rms_axis_px, r.max_px, r.count
    );
    if let Some(noise) = report.uncertainty.as_ref().and_then(|u| u.noise_px) {
        println!("  noise σ the residuals imply: {noise:.4} px");
    }
    let mut worst: Vec<_> = summary.views.iter().collect();
    worst.sort_by(|a, b| b.mean_px.total_cmp(&a.mean_px));
    for v in worst.iter().take(3) {
        println!(
            "  view {} {}: mean {:.4} px, max {:.4} px",
            v.view, v.camera, v.mean_px, v.max_px
        );
    }
    if let Some(q) = &report.quality {
        println!("verdict: {}", name(&q.verdict));
        for c in q.checks.iter().filter(|c| name(&c.status) != "pass") {
            let camera = c.camera.map_or(String::new(), |i| format!(" camera {i}"));
            println!(
                "  {} {}{camera}: {}",
                name(&c.status),
                name(&c.kind),
                c.message
            );
        }
    }
}
