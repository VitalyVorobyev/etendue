//! `etendue real …`: real datasets (calibration-rs's `dataset.toml` manifests, P20).

use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::Subcommand;
use etendue_cli::calibrate::{self, Report, RunOptions};
use etendue_cli::progress::Control;
use etendue_cli::real;
use etendue_synth::observe::ObservationNoise;

/// `etendue real …`.
#[derive(Subcommand)]
pub enum RealCommand {
    /// Calibrate a real dataset with calibration-rs's dataset runner (detection, then the
    /// manifest's workflow) and keep the run: residuals, σ, the quality verdict and the views
    /// left out. There is no truth, so there is no closed-loop check. A laser manifest's stripes
    /// are found by etendue's stripe detector, and the run reports each camera's laser plane and
    /// stripe residuals.
    Calibrate {
        /// The dataset: a manifest (`dataset.toml`) or its folder, an id of
        /// `local/datasets.toml`, or a public id (`kuka_1`, `DS8`: in `../calibration-rs/data`).
        dataset: String,
        /// The run's folder (a project's `runs/<name>`): config.json, report.json and
        /// calibration-rs's export as result.json, with the detection cache in `.detections`.
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Build the twin of a real run (`real calibrate`'s output): its result as the truth, its
    /// robot poses, and in each view the points its detector found, observed with the noise the
    /// real residuals imply. Calibrate it with the real run's workflow and configuration over
    /// seeded trials, keep the run, and set the real run against it.
    Twin {
        /// The real run's folder (with its `.detections`; the dataset's images must still be
        /// where it found them).
        run: PathBuf,
        /// Trials, with seeds `seed, seed + 1, …`.
        #[arg(long, default_value_t = 1)]
        trials: u32,
        /// The first trial's seed.
        #[arg(long, default_value_t = 0)]
        seed: u64,
        /// Calibrate exact (noise-free) observations once: the closed-loop check (G5.1).
        #[arg(long, conflicts_with_all = ["trials", "seed"])]
        exact: bool,
        /// The twin run's folder: config.json, report.json, result.json and the twin's gt.json.
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Check the pose planner's σ on a real run's own poses (P20c): its system σ, predicted on
    /// the twin, against the real calibration's σ and a split-half estimate (each half of the
    /// views calibrated on its own, several splits pooled). Gate P1: within 25 %.
    SplitHalf {
        /// The real run's folder (`real calibrate`'s, with its `.detections`).
        run: PathBuf,
        /// Random splits, beside the interleaved one and the halves in capture order.
        #[arg(long, default_value_t = 8)]
        random: usize,
        /// The first random split's seed.
        #[arg(long, default_value_t = 0)]
        seed: u64,
        /// The procedure of every calibration (a built-in id or a procedure file, as
        /// `calibrate --procedure`); by default the planner's model: a rig's hand-eye with the
        /// rig refined, a single camera's own workflow.
        #[arg(long)]
        procedure: Option<String>,
        /// The report (JSON): planner, real and split-half σ, per split and per parameter.
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
        RealCommand::Twin {
            run,
            trials,
            seed,
            exact,
            output,
        } => twin(&run, trials, seed, exact, &output),
        RealCommand::SplitHalf {
            run,
            random,
            seed,
            procedure,
            output,
        } => {
            let procedure = procedure
                .map(|name| {
                    let (id, procedure) = etendue_cli::procedure::named(&name)?;
                    anyhow::Ok(calibrate::RunProcedure { id, procedure })
                })
                .transpose()?;
            let options = real::SplitHalfOptions {
                random,
                seed,
                procedure,
            };
            let (twin, s) = real::split_half_run(&run, &options, &Control::new(&|_| {}, None))?;
            print_split_half(&twin.source.name(), &s);
            std::fs::write(&output, serde_json::to_string_pretty(&s)? + "\n")?;
            println!("report → {}", output.display());
            Ok(())
        }
    }
}

fn print_split_half(dataset: &str, s: &real::SplitHalf) {
    println!(
        "{dataset}: {} views, {}; planner noise {:.4} px{}, view scatter {:.2}",
        s.views,
        s.procedure,
        s.model.pixel_sigma_px,
        s.model.robot.map_or(String::new(), |r| format!(
            ", robot {:.3e} rad, {:.3e} m",
            r.rot_sigma_rad, r.trans_sigma_m
        )),
        s.model.view_scatter
    );
    for r in &s.splits {
        println!(
            "  split {:<12} {:>3} + {:<3} views: halves differ by {:.4} mm, predicted {:.4} mm ({:.2})",
            r.split,
            r.views[0],
            r.views[1],
            r.measured_mm,
            r.predicted_mm,
            r.measured_mm / r.predicted_mm
        );
    }
    for f in &s.failed {
        println!("  split left out: {f}");
    }
    let opt = |x: Option<f64>| x.map_or("—".to_owned(), |x| format!("{x:.4}"));
    println!(
        "system σ: planner {:.4} mm (exact robot poses {:.4}), real {} mm, split-half {:.4} mm",
        s.planner_mm,
        s.planner_exact_robot_mm,
        opt(s.real_mm),
        s.split_half_mm
    );
    if let (Some(w), Some(m)) = (s.white_model, s.planner_white_mm) {
        println!(
            "  white views and the run's robot error{} (P20c's model): {m:.4} mm",
            w.robot.map_or(String::new(), |r| format!(
                " ({:.3e} rad, {:.3e} m)",
                r.rot_sigma_rad, r.trans_sigma_m
            ))
        );
    }
    println!(
        "P1: planner / split-half = {:.3} (within {:.0} %): {}",
        s.ratio(),
        real::P1 * 100.0,
        if s.pass() { "PASS" } else { "FAIL" }
    );
    println!(
        "parameters (RMS over each group): σ planner, real, split-half; real / planner, split-half / planner"
    );
    for g in s.groups() {
        println!(
            "  {:<28} {:>3} {:>10} {:>10} {:>10}   {:>5}  {:.2}",
            g.group,
            g.unit,
            opt(g.planner),
            opt(g.real),
            opt(g.split_half),
            g.real_ratio.map_or("—".to_owned(), |x| format!("{x:.2}")),
            g.split_ratio
        );
    }
}

fn twin(run: &Path, trials: u32, seed: u64, exact: bool, output: &Path) -> Result<()> {
    let twin = real::twin(run)?;
    let noise = if exact {
        ObservationNoise::default()
    } else {
        ObservationNoise { seed, ..twin.noise }
    };
    println!(
        "twin of {} ({}): {} views, {} points; noise σ {:.4} px, robot {:.3e} rad, {:.3e} m",
        run.display(),
        twin.source.name(),
        twin.gt.captures.len(),
        twin.gt
            .captures
            .iter()
            .flat_map(|c| &c.views)
            .map(|v| v.points.len())
            .sum::<usize>(),
        noise.pixel_sigma_px,
        noise.robot_rot_sigma_rad,
        noise.robot_trans_sigma_m,
    );
    let options = RunOptions {
        noise,
        trials: trials.max(1),
        ..RunOptions::default()
    };
    let doc = calibrate::run_trials(&twin.gt, None, &options, &Control::new(&|_| {}, None))?;
    let first = &doc.reports[0];
    println!("{}:", first.workflow);
    print!("{}", first.table());
    calibrate::write_twin_run(output, run, &twin, &options, &doc)?;
    println!("run → {}", output.display());
    if exact {
        return if first.pass {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "the twin did not recover its truth ({})",
                first.check_name()
            ))
        };
    }
    let v = real::versus(&twin.real, &twin.gt, &doc);
    println!("real against twin, per camera (whiteness: 1 when white):");
    for c in &v.cameras {
        println!(
            "  {}: rms {:.4} / {:.4} px = {:.2}; across the image {:.2} / {:.2}, within views {:.2} / {:.2}",
            c.camera,
            c.real.rms_axis_px,
            c.twin.rms_axis_px,
            c.rms_ratio,
            c.real.spatial,
            c.twin.spatial,
            c.real.view,
            c.twin.view
        );
    }
    if let Some(scatter) = twin.view_scatter {
        println!(
            "the real σ is scaled by its view scatter, {scatter:.2} (its views vary more than its \
             residual noise predicts)"
        );
    }
    if doc.reports.len() > 1 {
        println!(
            "parameters: real σ, twin σ and std over {} trials, real − twin mean:",
            doc.reports.len()
        );
        for p in &v.params {
            let s = |x: Option<f64>| x.map_or("—".to_owned(), |x| format!("{x:.3e}"));
            println!(
                "  {:<22} {:>3}  real σ {:>10}  twin σ {:>10}  std {:.3e}  {:+.2} σ",
                p.label,
                p.unit,
                s(p.real_sigma),
                s(p.twin_sigma),
                p.twin_std,
                p.distance_sigma
            );
        }
    }
    Ok(())
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
    for l in &summary.lasers {
        println!(
            "laser {}: {} views, {} stripe pixels, rms {:.3} px from the line ({:.1} µm from the \
             plane), max {:.2} px",
            l.camera,
            l.views,
            l.pixels,
            l.rms_px,
            l.rms_m * 1e6,
            l.max_px
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
