//! Reading a scene and everything it references from disk.
//!
//! Robot manifests (`robot.json`) are resolved relative to the scene file;
//! each manifest's URDF relative to the manifest.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use etendue_kinematics::{RobotModel, bake};
use etendue_scene::{BakedScenario, FrameGraph, RobotManifest, ScenarioSpec, SceneSpec};

/// A scene with its robot models and manifests, read from disk.
pub struct Loaded {
    /// The validated scene.
    pub scene: SceneSpec,
    /// One model per scene robot.
    pub models: Vec<RobotModel>,
    /// Per scene robot: the manifest and the directory it was read from.
    pub manifests: Vec<(RobotManifest, PathBuf)>,
    /// Directory of the scene file.
    pub dir: PathBuf,
}

/// Read and parse a JSON document; `what` names it in errors.
///
/// # Errors
///
/// If the file does not read or parse.
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path, what: &str) -> Result<T> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading {what} {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {what} {}", path.display()))
}

/// Load and validate the scene at `path` and every robot model it references.
///
/// # Errors
///
/// If a file does not read or parse, the scene is invalid, a robot model does
/// not load, or the frame tree does not resolve.
pub fn load(path: &Path) -> Result<Loaded> {
    let scene: SceneSpec = read_json(path, "scene")?;
    scene
        .validate()
        .map_err(|e| anyhow!("scene {}: {e}", path.display()))?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut models = Vec::with_capacity(scene.robots.len());
    let mut manifests = Vec::with_capacity(scene.robots.len());
    for robot in &scene.robots {
        let manifest_path = dir.join(&robot.manifest);
        let manifest: RobotManifest = read_json(&manifest_path, "robot manifest")?;
        let urdf_path = manifest_path
            .parent()
            .unwrap_or(Path::new("."))
            .join(&manifest.urdf);
        let urdf = std::fs::read_to_string(&urdf_path)
            .with_context(|| format!("reading URDF {}", urdf_path.display()))?;
        let model = RobotModel::from_urdf_str(&urdf, &manifest)
            .with_context(|| format!("robot `{}` ({})", robot.id, manifest_path.display()))?;
        if let Some(q) = &robot.initial_q {
            model
                .check_q(q, 0.0)
                .with_context(|| format!("robot `{}` initial_q", robot.id))?;
        }
        models.push(model);
        manifests.push((
            manifest,
            manifest_path.parent().unwrap_or(Path::new(".")).to_owned(),
        ));
    }
    let links: Vec<Vec<String>> = models.iter().map(|m| m.links().to_vec()).collect();
    FrameGraph::build(&scene, &links).map_err(|e| anyhow!("scene {}: {e}", path.display()))?;
    Ok(Loaded {
        scene,
        models,
        manifests,
        dir: dir.to_owned(),
    })
}

/// Read the scenario at `path` and bake it against `loaded`.
///
/// # Errors
///
/// If the scenario does not read, parse or compile.
pub fn bake_file(loaded: &Loaded, path: &Path) -> Result<BakedScenario> {
    let scenario: ScenarioSpec = read_json(path, "scenario")?;
    bake(&loaded.scene, &scenario, &loaded.models)
        .map_err(|e| anyhow!("scenario {}: {e}", path.display()))
}
