//! Trials of a dataset and the runs kept from them (ADR 0009 `runs/<name>/`).

use anyhow::{Context, Result, ensure};
use etendue_synth::dataset::GroundTruth;
use etendue_synth::observe::{ObservationNoise, Observations};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::observations::from_features;
use super::report::Report;
use super::stats::{ParamStats, RowStats, param_stats, row_stats};
use super::{Options, calibrate};

/// How [`run_dir`] calibrates a dataset directory.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct RunOptions {
    /// Noise on the analytic observations (`seed` is the first trial's).
    pub noise: ObservationNoise,
    /// Trials, with seeds `seed, seed + 1, …` (at least one).
    pub trials: u32,
    /// Calibrate this camera of a rig alone.
    pub camera: Option<String>,
    /// Calibrate `etendue detect`'s observations (`features.json`, `laser_features.json`)
    /// instead of the ground truth's.
    pub detected: bool,
    /// The procedure to run, by value (so a kept run says exactly what ran); `None` for `auto`.
    pub procedure: Option<RunProcedure>,
}

/// A procedure as a run keeps it: its id and its stages.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunProcedure {
    pub id: String,
    pub procedure: etendue_scene::ProcedurePart,
}

/// What [`run_dir`] found: every trial's report and, with several, each row's spread.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Run {
    pub reports: Vec<Report>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_stats: Option<Vec<RowStats>>,
    /// With several trials, each estimated parameter's spread ([`param_stats`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub param_stats: Option<Vec<ParamStats>>,
}

/// Calibrate the dataset in `dir` (`gt.json`, and `etendue detect`'s files when `detected`),
/// reporting each trial through `ctl`. [`write_run`] keeps the run.
///
/// # Errors
///
/// Unreadable files, bad noise, a failed workflow, or cancellation (`progress::Cancelled`).
pub fn run_dir(
    dir: &std::path::Path,
    options: &RunOptions,
    ctl: &crate::progress::Control,
) -> Result<Run> {
    let gt: GroundTruth = crate::read_json(&dir.join("gt.json"), "ground truth")?;
    let detected = if options.detected {
        Some(read_detected(
            &gt,
            &dir.join("features.json"),
            &dir.join(crate::detect::LASER_FEATURES),
        )?)
    } else {
        None
    };
    run_trials(&gt, detected, options, ctl)
}

/// `etendue detect`'s observations of `gt`'s dataset: `features`, and `laser` when the dataset
/// has lasers.
///
/// # Errors
///
/// Unreadable files, or files that do not match the ground truth.
pub fn read_detected(
    gt: &GroundTruth,
    features: &std::path::Path,
    laser: &std::path::Path,
) -> Result<Observations> {
    let found: crate::detect::Features = crate::read_json(features, "features")?;
    let stripes: Option<crate::detect::laser::LaserFeatures> = if gt.lasers.is_empty() {
        None
    } else {
        Some(crate::read_json(laser, "laser features")?)
    };
    from_features(gt, &found, stripes.as_ref())
}

/// Calibrate `gt`'s dataset `options.trials` times: on `detected` observations (once), or on
/// the truth's own with `options.noise`, seeded `seed, seed + 1, …`. Each trial is reported
/// through `ctl`.
///
/// # Errors
///
/// Bad noise, noise or trials on detected observations, a failed workflow, or cancellation
/// (`progress::Cancelled`).
pub fn run_trials(
    gt: &GroundTruth,
    detected: Option<Observations>,
    options: &RunOptions,
    ctl: &crate::progress::Control,
) -> Result<Run> {
    use crate::progress::Stage;
    let problems = options.noise.problems();
    ensure!(
        problems.is_empty(),
        "observation noise: {}",
        problems.join("; ")
    );
    ensure!(
        options.detected == detected.is_some(),
        "the run's options and its observations disagree on whether they are detected"
    );
    let trials = options.trials.max(1);
    ensure!(
        detected.is_none() || (options.noise == ObservationNoise::default() && trials == 1),
        "noise and trials apply to the analytic observations, not to detected ones"
    );
    let calibrate_options = Options {
        camera: options.camera.clone(),
        detected: options.detected,
        procedure: options
            .procedure
            .as_ref()
            .map(|p| (p.id.clone(), p.procedure.clone())),
    };
    let mut reports = Vec::with_capacity(trials as usize);
    for k in 0..trials {
        ctl.check()?;
        ctl.step(Stage::Calibrate, k as usize, trials as usize);
        let obs = match &detected {
            Some(obs) => obs.clone(),
            None => etendue_synth::observe::observe(
                gt,
                &ObservationNoise {
                    seed: options.noise.seed.wrapping_add(u64::from(k)),
                    ..options.noise
                },
            ),
        };
        let mut report = calibrate(gt, &obs, &calibrate_options)?;
        if k > 0 {
            // Every point's residual is kept for the first trial only.
            report.residuals.features.clear();
        }
        ctl.log(format!(
            "trial {}: {}, mean reprojection {:.4} px, {}",
            k + 1,
            report.workflow,
            report.residuals.mean_px,
            if !report.gated {
                "closed-loop check not applied"
            } else if report.pass {
                "closed-loop check: PASS"
            } else {
                "closed-loop check: FAIL"
            }
        ));
        reports.push(report);
    }
    ctl.step(Stage::Calibrate, trials as usize, trials as usize);
    Ok(Run {
        row_stats: (reports.len() > 1).then(|| row_stats(&reports)),
        param_stats: (reports.len() > 1).then(|| param_stats(gt, &reports)),
        reports,
    })
}

/// A kept run's version: the newest this etendue reads. A run is written with the oldest version
/// that reads it: 1 for a synthetic dataset's, 2 for a real one's ([`RunConfig::real`], P20a), so
/// an etendue before P20a lists a real run as unreadable and still reads the others.
pub const RUN_VERSION: u32 = 2;

/// A kept run's files ([ADR 0009](https://github.com/VitalyVorobyev/etendue/blob/main/docs/adrs/0009-project-folder.md),
/// `runs/<name>/`): what was calibrated and how; the report ([`Run`]); calibration-rs's own
/// export of the first trial.
pub const RUN_CONFIG: &str = "config.json";

pub const RUN_REPORT: &str = "report.json";

pub const RUN_RESULT: &str = "result.json";

/// What a kept run calibrated, and how.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunConfig {
    pub version: u32,
    /// The dataset directory, as given (a real dataset's: its manifest's folder).
    pub dataset: String,
    /// How a synthetic dataset was calibrated (the defaults for a real one).
    pub options: RunOptions,
    /// The real dataset calibrated (P20a): its manifest, and the id that named it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub real: Option<crate::real::DatasetSource>,
}

/// Keep `run` of `dataset` in `folder` (created): [`RUN_CONFIG`], [`RUN_REPORT`] and
/// [`RUN_RESULT`].
///
/// # Errors
///
/// When a file cannot be written.
pub fn write_run(
    folder: &std::path::Path,
    dataset: &std::path::Path,
    options: &RunOptions,
    run: &Run,
) -> Result<()> {
    let config = RunConfig {
        version: 1,
        dataset: dataset.to_string_lossy().into_owned(),
        options: options.clone(),
        real: None,
    };
    keep(folder, &config, run)
}

/// Keep `run`, a calibration of the real dataset `source` ([`crate::real`]), in `folder`
/// (created): the files of [`write_run`].
///
/// # Errors
///
/// When a file cannot be written.
pub fn write_real_run(
    folder: &std::path::Path,
    source: &crate::real::DatasetSource,
    run: &Run,
) -> Result<()> {
    let config = RunConfig {
        version: 2,
        dataset: source.base_dir().to_string_lossy().into_owned(),
        options: RunOptions::default(),
        real: Some(source.clone()),
    };
    keep(folder, &config, run)
}

fn keep(folder: &std::path::Path, config: &RunConfig, run: &Run) -> Result<()> {
    std::fs::create_dir_all(folder).with_context(|| format!("creating {}", folder.display()))?;
    let result = run
        .reports
        .first()
        .map_or(Value::Null, |r| r.export.clone());
    for (name, text) in [
        (RUN_CONFIG, serde_json::to_string_pretty(config)?),
        (RUN_REPORT, serde_json::to_string_pretty(run)?),
        (RUN_RESULT, serde_json::to_string_pretty(&result)?),
    ] {
        let path = folder.join(name);
        std::fs::write(&path, text + "\n")
            .with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}

/// A kept run, read back.
#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct StoredRun {
    /// Its folder's name.
    pub name: String,
    pub config: RunConfig,
    pub run: Run,
}

/// The runs kept under a folder, and the folders that hold a run this etendue cannot read.
#[derive(Clone, Debug, Default, Serialize, schemars::JsonSchema)]
pub struct KeptRuns {
    /// By name.
    pub runs: Vec<StoredRun>,
    /// By name: each folder with a run config that did not read, and why.
    pub unreadable: Vec<UnreadableRun>,
}

/// A kept run that does not read: a newer etendue's, or damaged.
#[derive(Clone, Debug, PartialEq, Serialize, schemars::JsonSchema)]
pub struct UnreadableRun {
    /// Its folder's name.
    pub name: String,
    pub reason: String,
}

/// The run kept in `path`, checked: its config's version and its reports' are ones this etendue
/// reads.
fn read_run(path: &std::path::Path, name: String) -> Result<StoredRun> {
    #[derive(Deserialize)]
    struct Versioned {
        version: u32,
    }
    let newer = |what: &str, v: u32, reads: u32| {
        anyhow::anyhow!(
            "written by a newer etendue ({what} version {v}; this one reads up to {reads})"
        )
    };
    let v: Versioned = crate::read_json(&path.join(RUN_CONFIG), "run config")?;
    if v.version > RUN_VERSION {
        return Err(newer("run", v.version, RUN_VERSION));
    }
    #[derive(Deserialize)]
    struct Reports {
        reports: Vec<Versioned>,
    }
    let r: Reports = crate::read_json(&path.join(RUN_REPORT), "run report")?;
    if let Some(v) = r.reports.iter().map(|r| r.version).max()
        && v > super::REPORT_VERSION
    {
        return Err(newer("report", v, super::REPORT_VERSION));
    }
    Ok(StoredRun {
        name,
        config: crate::read_json(&path.join(RUN_CONFIG), "run config")?,
        run: crate::read_json(&path.join(RUN_REPORT), "run report")?,
    })
}

/// The runs kept under `folder` (one subfolder each, holding a [`RUN_CONFIG`]), by name. A run
/// that does not read is listed as [`UnreadableRun`] with the reason, never dropped silently; a
/// subfolder without a run config is not a run. A missing `folder` has none.
///
/// # Errors
///
/// When `folder` exists but cannot be listed.
pub fn read_runs(folder: &std::path::Path) -> Result<KeptRuns> {
    let mut kept = KeptRuns::default();
    if !folder.is_dir() {
        return Ok(kept);
    }
    for entry in
        std::fs::read_dir(folder).with_context(|| format!("listing {}", folder.display()))?
    {
        let path = entry?.path();
        if !path.join(RUN_CONFIG).is_file() {
            continue;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        match read_run(&path, name.clone()) {
            Ok(run) => kept.runs.push(run),
            Err(e) => kept.unreadable.push(UnreadableRun {
                name,
                reason: format!("{e:#}"),
            }),
        }
    }
    kept.runs.sort_by(|a, b| a.name.cmp(&b.name));
    kept.unreadable.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(kept)
}
