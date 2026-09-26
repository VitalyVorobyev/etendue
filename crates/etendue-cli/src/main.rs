//! `etendue` — command-line interface.
//!
//! ```text
//! etendue validate <scene.json> [<scenario.json>]
//! etendue bake <scene.json> <scenario.json> -o <baked.json> [--pretty]
//! ```
//!
//! Robot manifests (`robot.json`) are resolved relative to the scene file;
//! each manifest's URDF relative to the manifest.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use etendue_kinematics::{RobotModel, bake, compile};
use etendue_scene::{FrameGraph, RobotManifest, ScenarioSpec, SceneSpec};

#[derive(Parser)]
#[command(
    name = "etendue",
    version,
    about = "etendue scene, scenario, and baking tools"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validate a scene (robot models, frame tree) and optionally a
    /// scenario (compiled in full: limits, IK reachability).
    Validate {
        /// Scene file.
        scene: PathBuf,
        /// Scenario file.
        scenario: Option<PathBuf>,
    },
    /// Compile a scenario and write the baked scenario (`world_se3_frame`
    /// for every frame at every sample).
    Bake {
        /// Scene file.
        scene: PathBuf,
        /// Scenario file.
        scenario: PathBuf,
        /// Output file.
        #[arg(short, long)]
        output: PathBuf,
        /// Indent the JSON output.
        #[arg(long)]
        pretty: bool,
    },
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path, what: &str) -> Result<T> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading {what} {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {what} {}", path.display()))
}

/// Load and validate the scene and every robot model it references.
fn load_scene(path: &Path) -> Result<(SceneSpec, Vec<RobotModel>)> {
    let scene: SceneSpec = read_json(path, "scene")?;
    scene
        .validate()
        .map_err(|e| anyhow!("scene {}: {e}", path.display()))?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut models = Vec::with_capacity(scene.robots.len());
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
    }
    let links: Vec<Vec<String>> = models.iter().map(|m| m.links().to_vec()).collect();
    FrameGraph::build(&scene, &links).map_err(|e| anyhow!("scene {}: {e}", path.display()))?;
    Ok((scene, models))
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Validate { scene, scenario } => {
            let (spec, models) = load_scene(&scene)?;
            println!(
                "scene ok: {} robot(s), {} rig(s), {} camera(s), {} laser(s), {} light(s), {} target(s), {} part(s)",
                spec.robots.len(),
                spec.rigs.len(),
                spec.cameras.len(),
                spec.lasers.len(),
                spec.lights.len(),
                spec.targets.len(),
                spec.parts.len()
            );
            if let Some(path) = scenario {
                let scenario: ScenarioSpec = read_json(&path, "scenario")?;
                let trajectory = compile(&spec, &scenario, &models)
                    .map_err(|e| anyhow!("scenario {}: {e}", path.display()))?;
                let captures = trajectory
                    .samples
                    .iter()
                    .filter(|s| s.capture.is_some())
                    .count();
                println!(
                    "scenario ok: {} step(s), {:.3} s, {} sample(s) at dt = {} s, {captures} capture(s)",
                    scenario.steps.len(),
                    (trajectory.samples.len() - 1) as f64 * trajectory.dt,
                    trajectory.samples.len(),
                    trajectory.dt
                );
            }
            Ok(())
        }
        Command::Bake {
            scene,
            scenario,
            output,
            pretty,
        } => {
            let (spec, models) = load_scene(&scene)?;
            let scenario_spec: ScenarioSpec = read_json(&scenario, "scenario")?;
            let baked = bake(&spec, &scenario_spec, &models)
                .map_err(|e| anyhow!("scenario {}: {e}", scenario.display()))?;
            let text = if pretty {
                serde_json::to_string_pretty(&baked)?
            } else {
                serde_json::to_string(&baked)?
            };
            std::fs::write(&output, text + "\n")
                .with_context(|| format!("writing {}", output.display()))?;
            let captures = baked.capture_indices().count();
            if captures == 0 {
                eprintln!("warning: the scenario has no capture step");
            }
            println!(
                "baked {} sample(s) × {} frame(s), {captures} capture(s) → {}",
                baked.samples.len(),
                baked.frames.len(),
                output.display()
            );
            Ok(())
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}
