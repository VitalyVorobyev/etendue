//! `etendue` — command-line interface.
//!
//! ```text
//! etendue validate <scene.json> [<scenario.json>]
//! etendue bake <scene.json> <scenario.json> -o <baked.json> [--pretty]
//! etendue render <scene.json> <scenario.json> -o <out_dir> [--samples N] [--supersample S]
//! etendue gt <scene.json> <scenario.json> -o <out_dir>
//! etendue detect <dataset_dir>
//! etendue scenario from-poses <poses.json> --robot <id> -o <scenario.json> [--scene <scene.json>]
//! ```
//!
//! The pipeline itself is the `etendue_cli` library; this is its command
//! line, plus the gate measurements (`measure`).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use etendue_cli::progress::{self, Control};
use etendue_cli::render::{Blender, RenderOptions};
use etendue_cli::{bake_file, detect, gt, load, poses, read_json, render};
use etendue_kinematics::{RobotModel, compile};
use etendue_scene::{ScenarioSpec, SceneSpec};
use etendue_synth::sensor::SensorModel;

mod corners;
mod measure;

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
    /// Write the analytic ground truth of a scenario (ADR 0006): `dataset.json`
    /// (calibration-rs `DatasetSpec`), `robot_poses.json` and `gt.json`. The
    /// images it names are what `etendue render` writes into the same directory.
    Gt {
        /// Scene file.
        scene: PathBuf,
        /// Scenario file.
        scenario: PathBuf,
        /// Output directory.
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Detect the chessboard in a rendered dataset (`etendue gt` plus
    /// `etendue render` in one directory) and write `features.json`: the
    /// detected pixel of each board point per view, checked against the
    /// analytic ground truth.
    Detect {
        /// Dataset directory (dataset.json, gt.json, images/).
        dir: PathBuf,
    },
    /// Build scenarios.
    Scenario {
        #[command(subcommand)]
        command: ScenarioCommand,
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

/// `etendue scenario …`.
#[derive(Subcommand)]
enum ScenarioCommand {
    /// A stop-and-shoot scenario from tool poses: for each pose a `ptp_pose`
    /// move, then a capture. The poses file is a JSON array of
    /// `base_se3_tool` in the SE3 wire format, or `robot_poses.json` rows
    /// (`tx … qw`, optional `capture` id).
    FromPoses {
        /// Poses file.
        poses: PathBuf,
        /// Robot id in the scene.
        #[arg(long)]
        robot: String,
        /// Output scenario file.
        #[arg(short, long)]
        output: PathBuf,
        /// Fraction of the joint velocity and acceleration limits.
        #[arg(long, default_value_t = 0.5)]
        speed_scale: f64,
        /// Baking sample period, seconds.
        #[arg(long, default_value_t = 0.01)]
        dt: f64,
        /// Check the scenario against this scene (IK reachability, limits).
        #[arg(long)]
        scene: Option<PathBuf>,
    },
}

/// Gates measured by `etendue measure`.
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum Gate {
    /// G4.1 (P4-2): pixel-centre convention probe with emissive spheres.
    #[value(name = "g4-1")]
    G41,
    /// G4.2 (P4-3): chess-corners bias against the analytic ground truth,
    /// supersampling 1, 2, 4, 8 (ignores --supersample).
    #[value(name = "g4-2")]
    G42,
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

/// Load and validate the scene and every robot model it references.
fn load_scene(path: &Path) -> Result<(SceneSpec, Vec<RobotModel>)> {
    let l = load(path)?;
    Ok((l.scene, l.models))
}

/// The CLI's progress: log lines to stdout, never cancelled.
pub(crate) fn console() -> Control<'static> {
    Control::new(&progress::print, None)
}

/// Blender, checked against the `etendue.toml` pin nearest `from` (then the
/// working directory); warnings go to stderr.
pub(crate) fn checked_blender(
    arg: Option<&Path>,
    allow_other: bool,
    from: &Path,
) -> Result<Blender> {
    let cwd = std::env::current_dir()?;
    let blender = Blender::find(arg, &[from, &cwd])?;
    if let Some(warning) = blender.check(allow_other)? {
        eprintln!("{warning}");
    }
    Ok(blender)
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
            let baked = bake_file(&load(&scene)?, &scenario)?;
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
        Command::Gt {
            scene,
            scenario,
            output,
        } => {
            let loaded = load(&scene)?;
            let baked = bake_file(&loaded, &scenario)?;
            let summary = gt::write(&loaded, &baked, &output)?;
            println!("{summary} → {}", output.display());
            Ok(())
        }
        Command::Detect { dir } => {
            let features = detect::run(&dir, &console())?;
            for line in detect::summary_table(&features.summary()) {
                println!("{line}");
            }
            println!("features → {}", dir.join("features.json").display());
            Ok(())
        }
        Command::Scenario {
            command:
                ScenarioCommand::FromPoses {
                    poses,
                    robot,
                    output,
                    speed_scale,
                    dt,
                    scene,
                },
        } => {
            let text = std::fs::read_to_string(&poses)
                .with_context(|| format!("reading {}", poses.display()))?;
            let spec = poses::scenario(&text, &robot, speed_scale, dt)
                .with_context(|| format!("poses {}", poses.display()))?;
            if let Some(scene) = scene {
                let (scene_spec, models) = load_scene(&scene)?;
                let trajectory = compile(&scene_spec, &spec, &models)
                    .map_err(|e| anyhow!("the poses on {}: {e}", scene.display()))?;
                println!(
                    "reachable: {:.3} s of motion",
                    (trajectory.samples.len() - 1) as f64 * trajectory.dt
                );
            }
            std::fs::write(&output, serde_json::to_string_pretty(&spec)? + "\n")
                .with_context(|| format!("writing {}", output.display()))?;
            println!("{} capture(s) → {}", spec.steps.len() / 2, output.display());
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
        } => {
            // The sensor model first: a bad file should not cost a render.
            let sensor: Option<SensorModel> = match &sensor {
                Some(p) => Some(read_json(p, "sensor model")?),
                None => None,
            };
            let loaded = load(&scene)?;
            let baked = bake_file(&loaded, &scenario)?;
            let blender = checked_blender(blender.as_deref(), allow_blender_version, &loaded.dir)?;
            render::run(
                &loaded,
                &baked,
                &blender,
                &RenderOptions {
                    output,
                    samples,
                    seed,
                    cpu,
                    supersample,
                    exposure,
                    ambient,
                    cameras,
                    sensor,
                },
                &console(),
            )?;
            Ok(())
        }
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
                Gate::G42 => corners::g4_2(&args),
                Gate::P45 => {
                    let loaded = load(&scene)?;
                    let baked = bake_file(&loaded, &scenario)?;
                    measure::determinism(&loaded, &baked, &args)
                }
            }
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
