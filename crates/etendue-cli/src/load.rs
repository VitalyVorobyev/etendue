//! Reading a scene and everything it references from disk.
//!
//! Robot manifests (`robot.json`) are resolved relative to the scene file;
//! each manifest's URDF relative to the manifest.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use etendue_core::from_scene::resolve;
use etendue_kinematics::{BakeOptions, RobotModel, bake_with};
use etendue_scene::{BakedScenario, FrameGraph, RobotManifest, ScenarioSpec, SceneSpec};
use nalgebra::Isometry3;

/// A scene with its robot models and manifests, read from disk.
pub struct Loaded {
    /// The validated scene, in projection frames (`etendue-core::from_scene::resolve`): a camera
    /// with optics sits at its entrance pupil with its equivalent pinhole, so everything that
    /// bakes, projects or renders from it sees the physical camera.
    pub scene: SceneSpec,
    /// For each camera with optics: its projection frame in its body's frame.
    pub mech_se3_projection: BTreeMap<String, Isometry3<f64>>,
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
    let resolved = resolve(&scene).map_err(|e| anyhow!("scene {}: {e}", path.display()))?;
    let (scene, mech_se3_projection) = (resolved.scene, resolved.mech_se3_projection);
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
        mech_se3_projection,
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
pub fn bake_file(loaded: &Loaded, path: &Path, options: BakeOptions) -> Result<BakedScenario> {
    let scenario: ScenarioSpec = read_json(path, "scenario")?;
    bake_with(&loaded.scene, &scenario, &loaded.models, options)
        .map_err(|e| anyhow!("scenario {}: {e}", path.display()))
}
