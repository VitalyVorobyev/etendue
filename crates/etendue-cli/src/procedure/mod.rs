//! Calibration procedures (ADR 0011): run a [`ProcedurePart`]'s stages against a dataset.
//!
//! Each stage is resolved against the dataset into calibration-rs's own config (a patch over the
//! workflow's defaults) and seeds (`StageRun`), then run by the workflow functions of
//! [`crate::calibrate`]. The procedure's report is its last stage's, with the laser planes of the
//! latest stage that estimated them, and a summary of every stage.
//!
//! `auto` is today's choice by the dataset's shape, written as built-in procedures
//! ([`auto`]); [`builtin`] are the procedures etendue ships for people to start from.

use anyhow::{Context, Result, bail, ensure};
use etendue_scene::{
    Freedoms, Problem, ProcedurePart, Seed, Seeds, SensorChoice, SolverChoice, Stage,
};
use etendue_synth::dataset::{GroundTruth, GtHandeye};
use etendue_synth::observe::{ObservationNoise, Observations};
use serde_json::Value;

use crate::calibrate::{
    Options, Report, SeedValues, StageRun, StageSummary, Truth, check_observations, robot_poses,
    with_planes,
};

mod builtin;
mod patch;
mod seeds;

pub use builtin::{builtin, find_builtin};
use patch::patch;
use seeds::{Done, seeds};

fn stage(id: &str, problem: Problem) -> Stage {
    Stage {
        id: id.into(),
        problem,
        camera: None,
        model: None,
        free: Freedoms::default(),
        solver: None,
        seed: Seeds::default(),
    }
}

/// The procedure `etendue calibrate` runs without `--procedure`: by the dataset's shape, as before
/// procedures existed. Returns its id and stages.
///
/// # Errors
///
/// `options.camera` on a laser dataset, or a camera the dataset does not have.
pub fn auto(gt: &GroundTruth, options: &Options) -> Result<(String, ProcedurePart)> {
    let ids: Vec<&str> = gt.cameras.iter().map(|c| c.id.as_str()).collect();
    if let Some(name) = &options.camera {
        ensure!(
            gt.lasers.is_empty(),
            "--camera does not apply to a laser dataset"
        );
        ensure!(
            ids.contains(&name.as_str()),
            "no camera `{name}` (cameras: {ids:?})"
        );
        let mut s = stage("single_cam_handeye", Problem::SingleCamHandeye);
        s.camera = Some(name.clone());
        return Ok(("auto_single".into(), ProcedurePart { stages: vec![s] }));
    }
    let truth = Truth::new(gt);
    let all: Vec<usize> = (0..gt.cameras.len()).collect();
    let problem = Problem::for_dataset(gt.cameras.len(), !gt.lasers.is_empty());
    let one = |id: &str, s: Stage| (id.to_owned(), ProcedurePart { stages: vec![s] });
    Ok(match problem {
        Problem::SingleCamHandeye => one("auto_single", stage("single_cam_handeye", problem)),
        Problem::RigHandeye => one("auto_rig", stage("rig_handeye", problem)),
        Problem::LaserlineDevice => {
            let mut s = stage("laserline_device", problem);
            // No target pose is held: a single camera has no gauge to fix, and the linear
            // estimate of any one pose is not exact.
            s.free.fix_poses = Some(vec![]);
            s.solver = Some(SolverChoice {
                max_iters: Some(200),
                robust_loss: None,
            });
            s.seed.sensor = Some(Seed::Nominal);
            one("auto_head", s)
        }
        Problem::RigHandeyeLaserline => {
            let mut s = stage("rig_handeye_laserline", problem);
            if truth.scheimpflug(&all) {
                // A Scheimpflug rig is seeded from nominal values (calibration-rs ADR 0022).
                s.free.refine_intrinsics = Some(true);
                s.free.refine_sensor = Some(true);
                s.seed.intrinsics = Some(Seed::Nominal);
                s.seed.sensor = Some(Seed::Nominal);
                one("auto_rig_laser_scheimpflug", s)
            } else {
                one("auto_rig_laser", s)
            }
        }
        Problem::PlanarIntrinsics | Problem::RigExtrinsics => {
            unreachable!("a dataset's one-stage workflow solves its hand-eye")
        }
    })
}

/// Whether stage `s` calibrates a tilted (Scheimpflug) model of cameras `cams`.
fn tilted(truth: &Truth, s: &Stage, cams: &[usize]) -> bool {
    match s.model.as_ref().map(|m| m.sensor) {
        None | Some(SensorChoice::Design) => truth.scheimpflug(cams),
        Some(SensorChoice::Identity) => false,
        Some(SensorChoice::Scheimpflug) => true,
    }
}

/// The cameras a stage runs on, each run separately for a single-camera workflow.
fn cameras_of(truth: &Truth, s: &Stage) -> Result<Vec<usize>> {
    let ids: Vec<&str> = truth.gt.cameras.iter().map(|c| c.id.as_str()).collect();
    if s.problem.rig() {
        return Ok((0..ids.len()).collect());
    }
    Ok(match &s.camera {
        Some(name) => {
            vec![ids.iter().position(|c| c == name).with_context(|| {
                format!("stage `{}`: no camera `{name}` (cameras: {ids:?})", s.id)
            })?]
        }
        None => (0..ids.len()).collect(),
    })
}

/// Check that `procedure` fits the dataset before anything runs.
///
/// # Errors
///
/// Every mismatch, in one error.
pub fn check(gt: &GroundTruth, procedure: &ProcedurePart) -> Result<()> {
    // A procedure passed by value (the studio's edits) has not been through a library file's
    // checks.
    procedure.validate().map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut problems = Vec::new();
    let truth = Truth::new(gt);
    for s in &procedure.stages {
        let name = etendue_scene::procedure::problem_name(s.problem);
        if s.problem.handeye() && gt.handeye.is_none() {
            problems.push(format!(
                "stage `{}` ({name}) needs a robot: the dataset has none",
                s.id
            ));
        }
        if s.problem.laser() && gt.lasers.is_empty() {
            problems.push(format!(
                "stage `{}` ({name}) needs a laser: the dataset has none",
                s.id
            ));
        }
        if s.problem == Problem::LaserlineDevice && gt.cameras.len() != 1 {
            problems.push(format!(
                "stage `{}` (laserline_device) calibrates a one-camera head; the dataset has {} cameras",
                s.id,
                gt.cameras.len()
            ));
        }
        match cameras_of(&truth, s) {
            Err(e) => problems.push(format!("{e}")),
            // calibration-rs reads every option the stage sets (seeds aside: they come from the
            // stages before it).
            Ok(cams) => {
                let eye_in_hand = !matches!(gt.handeye, Some(GtHandeye::EyeToHand { .. }));
                let tilt = tilted(&truth, s, &cams);
                let config = patch(s, eye_in_hand, tilt, &SeedValues::default(), cams.len());
                if let Err(e) = crate::calibrate::check_config(s.problem, &config) {
                    problems.push(format!("stage `{}`: {e:#}", s.id));
                }
            }
        }
    }
    ensure!(
        problems.is_empty(),
        "the procedure does not fit the dataset:\n  {}",
        problems.join("\n  ")
    );
    Ok(())
}

/// Run one stage: one report, or one per camera for a single-camera workflow on several.
fn run_stage(
    truth: &Truth,
    obs: &Observations,
    s: &Stage,
    done: &Done,
    gated: bool,
) -> Result<Vec<Report>> {
    let cams = cameras_of(truth, s)?;
    let n = truth.gt.cameras.len();
    let scheimpflug = |cams: &[usize]| tilted(truth, s, cams);
    let eye_in_hand = if s.problem.handeye() {
        truth.eye_in_hand()?
    } else {
        true
    };
    let poses = if s.problem.handeye() {
        robot_poses(obs)?
    } else {
        vec![]
    };
    let resolve = |cams: &[usize]| -> Result<StageRun> {
        let sch = scheimpflug(cams);
        let seeds = seeds(truth, s, cams, done, sch)?;
        Ok(StageRun {
            patch: patch(s, eye_in_hand, sch, &seeds, n),
            seeds,
            scheimpflug: sch,
        })
    };
    use crate::calibrate as c;
    Ok(match s.problem {
        Problem::RigHandeye => vec![c::rig(truth, obs, &poses, gated, &resolve(&cams)?)?],
        Problem::RigHandeyeLaserline => {
            vec![c::rig_laser(truth, obs, &poses, gated, &resolve(&cams)?)?]
        }
        Problem::RigExtrinsics => vec![c::rig_extrinsics(truth, obs, gated, &resolve(&cams)?)?],
        Problem::LaserlineDevice => vec![c::head(
            truth,
            obs,
            &poses,
            eye_in_hand,
            gated,
            &resolve(&cams)?,
        )?],
        Problem::SingleCamHandeye => cams
            .iter()
            .map(|&i| c::single(truth, obs, &poses, i, eye_in_hand, gated, &resolve(&[i])?))
            .collect::<Result<_>>()?,
        Problem::PlanarIntrinsics => cams
            .iter()
            .map(|&i| c::intrinsics(truth, obs, i, gated, &resolve(&[i])?))
            .collect::<Result<_>>()?,
    })
}

/// Several single-camera reports of one stage as one: rows prefixed with their camera, counts
/// and residuals pooled.
fn pooled(mut reports: Vec<Report>) -> Report {
    if reports.len() == 1 {
        return reports.pop().expect("one");
    }
    let mut out = reports[0].clone();
    out.rows.clear();
    out.cameras.clear();
    out.estimate.cameras.clear();
    out.estimate.laser_planes.clear();
    out.estimate.handeye = None;
    out.estimate.offsets = None;
    let (mut views, mut points, mut pixels) = (0, 0, 0);
    let mut errors = Vec::new();
    let mut features = Vec::new();
    let (mut sum, mut sq, mut count) = (0.0, 0.0, 0usize);
    let mut per_camera = Vec::new();
    let mut max: f64 = 0.0;
    let mut export = serde_json::Map::new();
    for r in reports {
        let id = r.cameras.first().cloned().unwrap_or_default();
        for mut row in r.rows {
            if !row.label.starts_with(&format!("{id} ")) {
                row.label = format!("{id} {}", row.label);
            }
            out.rows.push(row);
        }
        views += r.views;
        points += r.points;
        pixels += r.laser_pixels;
        let k = r.residuals.count;
        sum += r.residuals.mean_px * k as f64;
        sq += r.residuals.rms_axis_px.powi(2) * k as f64;
        count += k;
        max = max.max(r.residuals.max_px);
        per_camera.extend(r.residuals.per_camera_mean_px);
        errors.extend(r.residuals.errors_px);
        features.extend(r.residuals.features);
        out.estimate.cameras.extend(r.estimate.cameras);
        out.estimate.laser_planes.extend(r.estimate.laser_planes);
        export.insert(id.clone(), r.export);
        out.cameras.push(id);
    }
    out.views = views;
    out.points = points;
    out.laser_pixels = pixels;
    out.residuals = crate::calibrate::Residuals {
        count,
        mean_px: if count == 0 {
            f64::NAN
        } else {
            sum / count as f64
        },
        rms_axis_px: if count == 0 {
            f64::NAN
        } else {
            (sq / count as f64).sqrt()
        },
        max_px: max,
        per_camera_mean_px: per_camera,
        features,
        errors_px: errors,
    };
    out.pass = !out.gated || out.rows.iter().all(crate::calibrate::Row::within);
    out.export = Value::Object(export);
    out
}

/// Run `procedure` (named `id`) on `obs` of `gt`'s dataset and compare its result with the truth.
///
/// # Errors
///
/// When the procedure does not fit the dataset, the observations are not the truth's, or a
/// calibration-rs workflow fails.
pub fn run(
    gt: &GroundTruth,
    obs: &Observations,
    options: &Options,
    id: &str,
    procedure: &ProcedurePart,
) -> Result<Report> {
    check_observations(gt, obs)?;
    check(gt, procedure)?;
    let truth = Truth::new(gt);
    let gated = obs.noise == ObservationNoise::default() && !options.detected;
    let mut done: Done = Vec::new();
    let mut summaries = Vec::new();
    for s in &procedure.stages {
        let reports =
            run_stage(&truth, obs, s, &done, gated).with_context(|| format!("stage `{}`", s.id))?;
        for r in &reports {
            summaries.push(StageSummary {
                id: s.id.clone(),
                workflow: r.workflow,
                cameras: r.cameras.clone(),
                views: r.views,
                points: r.points,
                mean_reproj_px: r.residuals.mean_px,
            });
        }
        done.push((s.id.clone(), reports));
    }
    let (_, last) = done.pop().context("the procedure has no stage")?;
    let mut report = pooled(last);
    // Laser planes the last stage did not estimate come from the latest stage that did.
    for (_, earlier) in done.iter().rev() {
        let had = pooled(earlier.clone());
        report = with_planes(&truth, report, &had);
    }
    let mut report = crate::calibrate::with_triangulation(&truth, report)?;
    if !done.is_empty() {
        // Several stages: every stage's export, by stage id.
        let mut all = serde_json::Map::new();
        for (sid, reports) in &done {
            all.insert(sid.clone(), pooled(reports.clone()).export);
        }
        let last_id = procedure.stages.last().expect("a stage").id.clone();
        all.insert(last_id, std::mem::take(&mut report.export));
        report.export = Value::Object(all);
    }
    report.procedure = id.to_owned();
    report.stages = summaries;
    Ok(report)
}

/// A procedure named on the command line: a built-in's id, or a library item file.
///
/// # Errors
///
/// When it is neither, or the file does not read or hold a procedure.
pub fn named(name: &str) -> Result<(String, ProcedurePart)> {
    if let Some(item) = find_builtin(name) {
        return Ok((item.id, item.procedure.expect("a procedure")));
    }
    let path = std::path::Path::new(name);
    if !path.is_file() {
        let ids: Vec<String> = builtin().into_iter().map(|p| p.id).collect();
        bail!(
            "`{name}` is neither a procedure file nor a built-in procedure ({})",
            ids.join(", ")
        );
    }
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {name}"))?;
    let item = crate::library::read_item(&text).map_err(|e| anyhow::anyhow!("{name}: {e}"))?;
    item.validate()
        .map_err(|e| anyhow::anyhow!("{name}: {e}"))?;
    let part = item
        .procedure
        .with_context(|| format!("{name}: holds no procedure"))?;
    Ok((item.id, part))
}
