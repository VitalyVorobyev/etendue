//! `etendue` — command-line interface.
//!
//! ```text
//! etendue validate <scene.json> [<scenario.json>]
//! etendue bake <scene.json> <scenario.json> -o <baked.json> [--pretty]
//! etendue render <scene.json> <scenario.json> -o <out_dir> [--samples N] [--supersample S]
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

mod measure;
mod render;

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
    /// Render every capture of a scenario with Blender (ADR 0005): canonical
    /// pinhole EXRs, remapped onto each calibrated camera as PNGs.
    Render {
        /// Scene file.
        scene: PathBuf,
        /// Scenario file.
        scenario: PathBuf,
        /// Output directory (job.json, exr/, images/).
        #[arg(short, long)]
        output: PathBuf,
        /// Render backend.
        #[arg(long, value_enum, default_value_t = Backend::Blender)]
        backend: Backend,
        /// Cycles samples per pixel.
        #[arg(long, default_value_t = 64)]
        samples: u32,
        /// Cycles seed.
        #[arg(long, default_value_t = 0)]
        seed: u32,
        /// Render on the CPU instead of the GPU.
        #[arg(long)]
        cpu: bool,
        /// Canonical pixels per camera pixel at the image centre.
        #[arg(long, default_value_t = 1.0)]
        supersample: f64,
        /// Linear scale before sRGB encoding (placeholder for the P4-6 sensor model).
        #[arg(long, default_value_t = 1.0)]
        exposure: f32,
        /// Radiance of a uniform white environment (0 = scene lights only).
        #[arg(long, default_value_t = 1.0)]
        ambient: f64,
        /// Blender executable (default: $ETENDUE_BLENDER, then the platform default).
        #[arg(long)]
        blender: Option<PathBuf>,
        /// Render even if Blender's version differs from the etendue.toml pin.
        #[arg(long)]
        allow_blender_version: bool,
        /// Render only these cameras (repeatable).
        #[arg(long = "camera")]
        cameras: Vec<String>,
        /// Sensor model (JSON, etendue-synth `SensorModel`): exposure, noise, quantisation;
        /// images become raw mono PNGs. Without it, `--exposure` and sRGB PNGs.
        #[arg(long)]
        sensor: Option<PathBuf>,
    },
    /// Measure a gate that needs Blender (local only; results go to
    /// docs/measurements/).
    Measure {
        /// Which gate.
        #[arg(value_enum)]
        gate: Gate,
        /// Output directory.
        #[arg(short, long)]
        output: PathBuf,
        /// Cycles samples per pixel.
        #[arg(long, default_value_t = 256)]
        samples: u32,
        /// Canonical supersampling.
        #[arg(long, default_value_t = 4.0)]
        supersample: f64,
        /// Blender executable.
        #[arg(long)]
        blender: Option<PathBuf>,
        /// Run even if Blender's version differs from the pin.
        #[arg(long)]
        allow_blender_version: bool,
        /// Scene (gates that render a scene).
        #[arg(long, default_value = "examples/eye_in_hand_ur5e/scene.json")]
        scene: PathBuf,
        /// Scenario (gates that render a scene).
        #[arg(long, default_value = "examples/eye_in_hand_ur5e/scenario.json")]
        scenario: PathBuf,
    },
}

/// Gates measured by `etendue measure`.
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum Gate {
    /// G4.1 (P4-2): pixel-centre convention probe with emissive spheres.
    #[value(name = "g4-1")]
    G41,
    /// P4-5: render determinism, GPU and CPU (needs --scene / --scenario).
    #[value(name = "p4-5")]
    P45,
}

/// Render backends (ADR 0005: Blender is the only photometric one).
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum Backend {
    /// Blender / Cycles.
    Blender,
}

/// A scene with its robot models and manifests, read from disk.
pub(crate) struct Loaded {
    pub scene: SceneSpec,
    pub models: Vec<RobotModel>,
    /// Per scene robot: the manifest and the directory it was read from.
    pub manifests: Vec<(RobotManifest, PathBuf)>,
    /// Directory of the scene file.
    pub dir: PathBuf,
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path, what: &str) -> Result<T> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading {what} {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {what} {}", path.display()))
}

/// Load and validate the scene and every robot model it references.
fn load_scene(path: &Path) -> Result<(SceneSpec, Vec<RobotModel>)> {
    let l = load(path)?;
    Ok((l.scene, l.models))
}

fn load(path: &Path) -> Result<Loaded> {
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
        Command::Render {
            scene,
            scenario,
            output,
            backend: Backend::Blender,
            samples,
            seed,
            cpu,
            supersample,
            exposure,
            ambient,
            blender,
            allow_blender_version,
            cameras,
            sensor,
        } => render_command(
            &scene,
            &scenario,
            &render::RenderArgs {
                output,
                samples,
                seed,
                cpu,
                supersample,
                exposure,
                ambient,
                blender,
                allow_blender_version,
                cameras,
                sensor,
            },
        ),
        Command::Measure {
            gate,
            output,
            samples,
            supersample,
            blender,
            allow_blender_version,
            scene,
            scenario,
        } => {
            let args = measure::ProbeArgs {
                output,
                samples,
                supersample,
                blender,
                allow_blender_version,
            };
            match gate {
                Gate::G41 => measure::g4_1(&args),
                Gate::P45 => {
                    let loaded = load(&scene)?;
                    let scenario_spec: ScenarioSpec = read_json(&scenario, "scenario")?;
                    let baked = bake(&loaded.scene, &scenario_spec, &loaded.models)
                        .map_err(|e| anyhow!("scenario {}: {e}", scenario.display()))?;
                    measure::determinism(&loaded, &baked, &args)
                }
            }
        }
    }
}

fn render_command(scene: &Path, scenario: &Path, args: &render::RenderArgs) -> Result<()> {
    let loaded = load(scene)?;
    let scenario_spec: ScenarioSpec = read_json(scenario, "scenario")?;
    let baked = bake(&loaded.scene, &scenario_spec, &loaded.models)
        .map_err(|e| anyhow!("scenario {}: {e}", scenario.display()))?;
    render::run(&loaded, &baked, args)
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
