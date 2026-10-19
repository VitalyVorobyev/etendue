//! Project folders (ADR 0009): checking one as a whole.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Result, anyhow, bail};
use etendue_core::from_scene::check_derived;
use etendue_kinematics::compile;
use etendue_scene::{
    KINDS, LibraryItem, PROJECT_FILE, ProjectManifest, RobotManifest, ScenarioSpec, library_file,
};

use crate::load::{load, read_json};

/// What a project holds, once checked.
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
}

/// Check the project folder `dir`:
/// - its manifest;
/// - every cell (the scene with its robots and optics, and the scenario compiled in full);
/// - every library item (its version, its id against its file name, its kind against its
///   folder, a camera design's params against its optics);
/// - the defaults (they name existing items of the right kind);
/// - the project's robots.
///
/// # Errors
///
/// Every problem found, one per line.
pub fn check(dir: &Path) -> Result<ProjectSummary> {
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

    for (kind, folder) in KINDS {
        let at = dir.join("library").join(folder);
        let Ok(entries) = std::fs::read_dir(&at) else {
            continue;
        };
        let mut files: Vec<_> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
        files.sort();
        for path in files
            .into_iter()
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
        {
            let name = format!(
                "library/{folder}/{}",
                path.file_name().unwrap_or_default().to_string_lossy()
            );
            let text = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(e) => {
                    problems.push(format!("{name}: {e}"));
                    continue;
                }
            };
            let item = match LibraryItem::from_json(&text) {
                Ok(i) => i,
                Err(e) => {
                    problems.extend(
                        e.issues
                            .iter()
                            .map(|i| format!("{name}: {}: {}", i.path, i.message)),
                    );
                    continue;
                }
            };
            let mut issues = item.validate().err().map(|e| e.issues).unwrap_or_default();
            if path
                .file_stem()
                .is_none_or(|s| s.to_string_lossy() != item.id)
            {
                problems.push(format!("{name}: its id `{}` is not its file name", item.id));
            }
            if item.kind().is_some_and(|k| k != kind) {
                problems.push(format!(
                    "{name}: a {} in the {folder} folder",
                    item.kind().unwrap_or_default()
                ));
            }
            if let Some(c) = &item.camera
                && let Some(o) = &c.optics
            {
                issues.extend(check_derived(o, &c.params, "camera"));
            }
            if let Some(r) = &item.rig
                && let Some(o) = &r.layout.camera.optics
            {
                issues.extend(check_derived(
                    o,
                    &r.layout.camera.params,
                    "rig.layout.camera",
                ));
            }
            problems.extend(
                issues
                    .iter()
                    .map(|i| format!("{name}: {}: {}", i.path, i.message)),
            );
            *summary.library.entry(folder).or_default() += 1;
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
