//! `etendue export`: a scenario's motion as files a controller or a
//! spreadsheet reads — the tool pose and joints at every capture, the timed
//! joint trajectory, the motion report, and the scenario with every IK
//! solution pinned.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use etendue_kinematics::{
    BakeOptions, ReportOptions, bake_trajectory, compile, report_trajectory, resolve,
};
use etendue_scene::{ScenarioSpec, export};

use crate::load::Loaded;

/// What to write.
#[derive(Clone, Debug, Default)]
pub struct ExportOptions {
    /// The robot whose motion is exported (default: the scene's only robot).
    pub robot: Option<String>,
    /// Tool pose and joints at every capture: `.csv` or `.json`.
    pub captures: Option<PathBuf>,
    /// Timed joint trajectory (CSV): `t`, capture, positions, velocities,
    /// accelerations.
    pub trajectory: Option<PathBuf>,
    /// Motion report (JSON).
    pub report: Option<PathBuf>,
    /// The scenario with every `ptp_pose`'s configuration stored (JSON).
    pub resolved: Option<PathBuf>,
}

fn write(path: &Path, text: &str) -> Result<()> {
    std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

fn json<T: serde::Serialize>(doc: &T) -> Result<String> {
    Ok(serde_json::to_string_pretty(doc)? + "\n")
}

/// Write what `options` asks for; returns one line per file written.
///
/// # Errors
///
/// If the scenario does not compile, the robot is unknown or ambiguous, or
/// a file cannot be written.
pub fn run(
    loaded: &Loaded,
    scenario: &ScenarioSpec,
    options: &ExportOptions,
) -> Result<Vec<String>> {
    let scene = &loaded.scene;
    let r = match &options.robot {
        Some(id) => scene
            .robots
            .iter()
            .position(|r| &r.id == id)
            .ok_or_else(|| anyhow!("no robot `{id}` in the scene"))?,
        None if scene.robots.len() == 1 => 0,
        None if scene.robots.is_empty() => bail!("the scene has no robot"),
        None => bail!("the scene has several robots; choose one with --robot"),
    };
    let id = &scene.robots[r].id;
    let model = &loaded.models[r];
    let trajectory =
        compile(scene, scenario, &loaded.models).map_err(|e| anyhow!("scenario: {e}"))?;
    let mut done = Vec::new();
    if options.captures.is_some() || options.trajectory.is_some() {
        let baked = bake_trajectory(
            scene,
            &trajectory,
            &loaded.models,
            BakeOptions { derivatives: true },
        )
        .map_err(|e| anyhow!("scenario: {e}"))?;
        if let Some(path) = &options.captures {
            let links = model.links();
            let rows = export::captures(
                &baked,
                id,
                &links[model.base_link()],
                &links[model.tcp_link()],
            )?;
            let text = match path.extension().and_then(|e| e.to_str()) {
                Some("csv") => rows.to_csv(),
                Some("json") => json(&rows)?,
                _ => bail!("{}: captures go to a .csv or .json file", path.display()),
            };
            write(path, &text)?;
            done.push(format!(
                "{} capture(s) of `{id}` → {}",
                rows.captures.len(),
                path.display()
            ));
        }
        if let Some(path) = &options.trajectory {
            write(path, &export::trajectory_csv(&baked, id)?)?;
            done.push(format!(
                "{} sample(s) of `{id}` → {}",
                baked.samples.len(),
                path.display()
            ));
        }
    }
    if let Some(path) = &options.report {
        let report = report_trajectory(
            scene,
            scenario,
            &trajectory,
            &loaded.models,
            &ReportOptions::default(),
        );
        let warnings: usize = report.steps.iter().map(|s| s.warnings.len()).sum();
        write(path, &json(&report)?)?;
        done.push(format!(
            "motion report ({:.2} s, {warnings} warning(s)) → {}",
            report.duration_s,
            path.display()
        ));
    }
    if let Some(path) = &options.resolved {
        let resolved =
            resolve(scene, scenario, &loaded.models).map_err(|e| anyhow!("scenario: {e}"))?;
        write(path, &json(&resolved)?)?;
        done.push(format!("resolved scenario → {}", path.display()));
    }
    if done.is_empty() {
        bail!("nothing to export: give --captures, --trajectory, --report or --resolved");
    }
    Ok(done)
}
