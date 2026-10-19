//! `etendue gt`: write a scenario's analytic ground truth (ADR 0006) as a
//! calibration-rs dataset.

use std::path::Path;

use anyhow::{Context, Result};
use etendue_scene::{BakedScenario, RobotManifest};
use etendue_synth::dataset::{EmitOptions, emit_scene};
use serde::Serialize;

use crate::load::Loaded;

/// What [`write()`] wrote.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GtSummary {
    /// Captures.
    pub captures: usize,
    /// Views (captures × cameras).
    pub views: usize,
    /// Visible target points over all views.
    pub visible: usize,
    /// Points on the target.
    pub points: usize,
}

impl std::fmt::Display for GtSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ground truth: {} capture(s), {} view(s), {} visible point(s) of {}",
            self.captures, self.views, self.visible, self.points
        )
    }
}

/// Write `dataset.json` (calibration-rs `DatasetSpec`), `robot_poses.json`
/// (hand-eye topologies) and `gt.json` into `output`. The images the dataset
/// names are what [`crate::render::run`] writes into the same directory.
///
/// # Errors
///
/// If the scene poses no calibration problem calibration-rs can take (see
/// `etendue_synth::dataset::emit`), has other than one board target, or a
/// file cannot be written.
pub fn write(loaded: &Loaded, baked: &BakedScenario, output: &Path) -> Result<GtSummary> {
    let manifests: Vec<RobotManifest> = loaded.manifests.iter().map(|(m, _)| m.clone()).collect();
    let mut bundle = emit_scene(&loaded.scene, baked, &manifests, &EmitOptions::STANDARD)?;
    // The scene was resolved into projection frames: say where each body is.
    for camera in &mut bundle.gt.cameras {
        camera.mech_se3_projection = loaded.mech_se3_projection.get(&camera.id).copied();
    }
    std::fs::create_dir_all(output).with_context(|| format!("creating {}", output.display()))?;
    let write = |name: &str, text: String| {
        let path = output.join(name);
        std::fs::write(&path, text + "\n").with_context(|| format!("writing {}", path.display()))
    };
    // A target calibration-rs does not take yet has ground truth but no dataset.
    let dataset = output.join("dataset.json");
    match &bundle.dataset {
        Some(d) => write("dataset.json", serde_json::to_string_pretty(d)?)?,
        None if dataset.is_file() => std::fs::remove_file(&dataset)
            .with_context(|| format!("removing {}", dataset.display()))?,
        None => {}
    }
    if let Some(poses) = &bundle.robot_poses {
        write("robot_poses.json", serde_json::to_string_pretty(poses)?)?;
    }
    write("gt.json", serde_json::to_string_pretty(&bundle.gt)?)?;
    Ok(GtSummary {
        captures: bundle.gt.captures.len(),
        views: bundle.gt.captures.iter().map(|c| c.views.len()).sum(),
        visible: bundle
            .gt
            .captures
            .iter()
            .flat_map(|c| &c.views)
            .map(|v| v.points.iter().filter(|p| p.visible()).count())
            .sum(),
        points: bundle.gt.target.points.len(),
    })
}
