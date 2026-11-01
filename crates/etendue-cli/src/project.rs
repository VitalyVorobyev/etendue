//! Project folders (ADR 0009): checking one as a whole.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Result, anyhow, bail};
use etendue_kinematics::compile;
use etendue_scene::{
    Layer, PROJECT_FILE, ProjectManifest, RobotManifest, ScenarioSpec, library_file,
};

use crate::library;
use crate::load::{load, read_json};

#[derive(Clone, Debug, Default)]
pub struct ProjectSummary {
    /// The project's name.
    pub name: String,
    /// Cells checked.
    pub cells: usize,
    /// Library items, by folder (`lenses`).
    pub library: BTreeMap<&'static str, usize>,
    /// Robots the project carries.
    pub robots: usize,
    /// With a bank: copies whose source in the bank has changed or gone (ADR 0010 §3). These
    /// are not problems: the project is self-contained.
    pub notes: Vec<String>,
}

/// Check the project folder `dir`:
/// - its manifest;
/// - every cell (the scene with its robots and optics, and the scenario compiled in full);
/// - every library item (its version, its id against its file name, its kind against its
///   folder, a camera design's params against its optics);
/// - the defaults (they name existing items of the right kind);
/// - the project's robots.
///
/// With `bank`, also note the items copied from it whose source has changed since.
///
/// # Errors
///
/// Every problem found, one per line.
pub fn check(dir: &Path, bank: Option<&Path>) -> Result<ProjectSummary> {
    let manifest: ProjectManifest = read_json(&dir.join(PROJECT_FILE), "project manifest")?;
    let mut problems: Vec<String> = Vec::new();
    if let Err(e) = manifest.validate() {
        problems.extend(
            e.issues
                .iter()
                .map(|i| format!("{PROJECT_FILE}: {}: {}", i.path, i.message)),
        );
    }
    let mut summary = ProjectSummary {
        name: manifest.name.clone(),
        ..ProjectSummary::default()
    };

    for cell in &manifest.cells {
        let scene = dir.join(cell).join("scene.json");
        let loaded = match load(&scene) {
            Ok(l) => l,
            Err(e) => {
                problems.push(format!("{cell}: {e:#}"));
                continue;
            }
        };
        let scenario = dir.join(cell).join("scenario.json");
        if scenario.is_file() {
            let checked = read_json::<ScenarioSpec>(&scenario, "scenario").and_then(|s| {
                compile(&loaded.scene, &s, &loaded.models).map_err(|e| anyhow!("{e}"))
            });
            if let Err(e) = checked {
                problems.push(format!("{cell}/scenario.json: {e:#}"));
            }
        }
        summary.cells += 1;
    }

    let (contents, found) = library::scan(dir);
    problems.extend(found);
    summary.library = contents.counts();
    if let Some(bank) = bank {
        let (banked, _) = library::scan(bank);
        for (reference, item) in &contents.items {
            let Some(origin) = item.origin.as_ref().filter(|o| o.layer == Layer::Bank) else {
                continue;
            };
            match banked.items.get(&origin.reference) {
                Some(source) if etendue_core::library::content_hash(source) != origin.sha256 => {
                    summary.notes.push(format!(
                        "library/{reference}.json: the bank's {} has changed since it was copied",
                        origin.reference
                    ))
                }
                None => summary.notes.push(format!(
                    "library/{reference}.json: the bank no longer has {}",
                    origin.reference
                )),
                Some(_) => {}
            }
        }
    }

    let defaults = &manifest.defaults;
    for (field, reference, folder) in [
        ("camera", &defaults.camera, "cameras"),
        ("laser", &defaults.laser, "lasers"),
        ("target", &defaults.target, "targets"),
    ] {
        if let Some(r) = reference
            && let Some(file) = library_file(r, &[folder])
            && !dir.join(&file).is_file()
        {
            problems.push(format!("{PROJECT_FILE}: defaults.{field}: no {file}"));
        }
    }

    for robot in &manifest.robots {
        let path = dir.join(robot).join("robot.json");
        match read_json::<RobotManifest>(&path, "robot manifest") {
            Ok(m) => {
                if let Err(e) = m.validate() {
                    problems.push(format!("{robot}/robot.json: {e}"));
                }
                summary.robots += 1;
            }
            Err(e) => problems.push(format!("{robot}: {e:#}")),
        }
    }

    if !problems.is_empty() {
        bail!(
            "project {}: {} problem(s)\n  {}",
            dir.display(),
            problems.len(),
            problems.join("\n  ")
        );
    }
    Ok(summary)
}
