//! `etendue render --backend blender` (ADR 0005): bake, choose each camera's
//! canonical pinhole (ADR 0004), write `job.json`, run Blender with the
//! embedded script, then resample every EXR through the camera's remap LUT
//! into `images/<camera>/<capture>.png`.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use etendue_scene::{BakedScenario, SceneSpec};
use etendue_synth::images::{read_exr_combined, remap_image_box, write_png_srgb};
use etendue_synth::job::{Device, JobMesh, ROBOT_PASS_INDEX, RenderJob, RenderSettings, build_job};
use etendue_synth::{CanonicalCamera, CanonicalSpec, PixelCentre, RemapLut, remap_lut};

use crate::Loaded;

const SCRIPT: &str = include_str!("../blender/etendue_blender/render.py");
const CONVERT: &str = include_str!("../blender/etendue_blender/convert.py");

/// Pixel-centre convention of the Blender backend's LUTs. **Provisional**:
/// probe P4-2 measures it (ADR 0004).
const PIXEL_CENTRE: PixelCentre = PixelCentre::Integer;

/// Options of `etendue render`.
pub struct RenderArgs {
    pub output: PathBuf,
    pub samples: u32,
    pub seed: u32,
    pub cpu: bool,
    pub supersample: f64,
    pub exposure: f32,
    pub ambient: f64,
    pub blender: Option<PathBuf>,
    pub allow_blender_version: bool,
    pub cameras: Vec<String>,
}

/// The Blender executable: `--blender`, then `$ETENDUE_BLENDER`, then the
/// platform default.
fn blender_path(arg: Option<&Path>) -> PathBuf {
    if let Some(p) = arg {
        return p.to_owned();
    }
    if let Some(p) = std::env::var_os("ETENDUE_BLENDER") {
        return PathBuf::from(p);
    }
    if cfg!(target_os = "macos") {
        PathBuf::from("/Applications/Blender.app/Contents/MacOS/Blender")
    } else {
        PathBuf::from("blender")
    }
}

/// The pinned Blender version from the nearest `etendue.toml` above `from`
/// (then above the working directory).
fn pinned_version(from: &Path) -> Result<Option<(String, PathBuf)>> {
    let cwd = std::env::current_dir()?;
    for start in [from, cwd.as_path()] {
        let mut dir = Some(start);
        while let Some(d) = dir {
            let file = d.join("etendue.toml");
            if file.is_file() {
                let text = std::fs::read_to_string(&file)?;
                let doc: toml::Table = text
                    .parse()
                    .with_context(|| format!("parsing {}", file.display()))?;
                let version = doc
                    .get("blender")
                    .and_then(|b| b.get("version"))
                    .and_then(|v| v.as_str())
                    .map(str::to_owned);
                return Ok(version.map(|v| (v, file)));
            }
            dir = d.parent();
        }
    }
    Ok(None)
}

/// `Blender X.Y.Z` from `blender --version`.
fn blender_version(exe: &Path) -> Result<String> {
    let out = Command::new(exe)
        .arg("--version")
        .output()
        .with_context(|| {
            format!(
                "running {} (set --blender or $ETENDUE_BLENDER)",
                exe.display()
            )
        })?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .find_map(|l| l.strip_prefix("Blender "))
        .and_then(|v| v.split_whitespace().next())
        .map(str::to_owned)
        .ok_or_else(|| {
            anyhow!(
                "could not read the Blender version from `{} --version`",
                exe.display()
            )
        })
}

fn meshes(loaded: &Loaded) -> Result<Vec<JobMesh>> {
    let mut out = Vec::new();
    for (robot, (manifest, dir)) in loaded.scene.robots.iter().zip(&loaded.manifests) {
        for v in &manifest.visuals {
            let path = dir.join(&v.mesh);
            if !path.is_file() {
                bail!(
                    "robot `{}`: mesh {} is missing; build the robot assets \
                     (tools/robot-assets/build.py)",
                    robot.id,
                    path.display()
                );
            }
            out.push(JobMesh {
                id: format!("{}/{}", robot.id, v.link),
                frame: format!("{}/{}", robot.id, v.link),
                path: path.canonicalize()?.display().to_string(),
                pass_index: ROBOT_PASS_INDEX,
            });
        }
    }
    for (i, part) in loaded.scene.parts.iter().enumerate() {
        let path = loaded.dir.join(&part.mesh);
        if !path.is_file() {
            bail!(
                "parts[{i}] `{}`: mesh {} is missing",
                part.id,
                path.display()
            );
        }
        out.push(JobMesh {
            id: part.id.clone(),
            frame: part.id.clone(),
            path: path.canonicalize()?.display().to_string(),
            pass_index: ROBOT_PASS_INDEX + 1 + i as u32,
        });
    }
    Ok(out)
}

/// The Blender to run and its version, checked against the `etendue.toml` pin
/// nearest `from`.
pub fn checked_blender(
    arg: Option<&Path>,
    allow_other: bool,
    from: &Path,
) -> Result<(PathBuf, String)> {
    let exe = blender_path(arg);
    let version = blender_version(&exe)?;
    match pinned_version(from)? {
        Some((pin, file)) if pin != version && !allow_other => bail!(
            "Blender {version} at {} but {} pins {pin}; install it, point --blender at it, \
             or pass --allow-blender-version",
            exe.display(),
            file.display()
        ),
        Some((pin, _)) if pin != version => {
            eprintln!("warning: rendering with Blender {version}, pinned {pin}");
        }
        None => eprintln!("warning: no etendue.toml Blender pin found"),
        _ => {}
    }
    Ok((exe, version))
}

/// Write `job` and the embedded script into `out`, and run Blender on them.
pub fn run_blender(exe: &Path, job: &RenderJob, out: &Path) -> Result<()> {
    std::fs::create_dir_all(out)?;
    let job_path = out.join("job.json");
    std::fs::write(&job_path, serde_json::to_string_pretty(job)? + "\n")?;
    let scripts = out.join(".etendue_blender");
    std::fs::create_dir_all(&scripts)?;
    std::fs::write(scripts.join("render.py"), SCRIPT)?;
    std::fs::write(scripts.join("convert.py"), CONVERT)?;
    let status = Command::new(exe)
        .args([
            "-b",
            "--factory-startup",
            "--python-exit-code",
            "1",
            "--python",
        ])
        .arg(scripts.join("render.py"))
        .arg("--")
        .arg(&job_path)
        .arg(out)
        .status()
        .with_context(|| format!("running {}", exe.display()))?;
    if !status.success() {
        bail!("Blender failed ({status})");
    }
    Ok(())
}

pub fn run(loaded: &Loaded, baked: &BakedScenario, args: &RenderArgs) -> Result<()> {
    let scene: &SceneSpec = &loaded.scene;
    let (exe, version) = checked_blender(
        args.blender.as_deref(),
        args.allow_blender_version,
        &loaded.dir,
    )?;

    let selected: Vec<_> = scene
        .cameras
        .iter()
        .filter(|c| args.cameras.is_empty() || args.cameras.contains(&c.id))
        .collect();
    if selected.is_empty() {
        bail!("no camera to render");
    }
    let spec = CanonicalSpec {
        supersample: args.supersample,
        ..CanonicalSpec::default()
    };
    let mut canonical: Vec<(String, CanonicalCamera, RemapLut)> = Vec::new();
    for c in &selected {
        let cam = CanonicalCamera::cover(&c.params, c.resolution, &spec, PIXEL_CENTRE)
            .with_context(|| format!("camera `{}`", c.id))?;
        let lut = remap_lut(&c.params, c.resolution, &cam)?;
        canonical.push((c.id.clone(), cam, lut));
    }

    let job = build_job(
        scene,
        baked,
        meshes(loaded)?,
        &canonical
            .iter()
            .map(|(id, cam, _)| (id.as_str(), cam))
            .collect::<Vec<_>>(),
        RenderSettings {
            samples: args.samples,
            seed: args.seed,
            device: if args.cpu { Device::Cpu } else { Device::Gpu },
            ambient: args.ambient,
            ..RenderSettings::default()
        },
    )?;

    let out = &args.output;
    let n: usize = job.shots.iter().map(|s| s.outputs.len()).sum();
    println!(
        "rendering {n} image(s) with Blender {version}: {} shot(s) × {} camera(s), {} samples",
        job.shots.len(),
        job.cameras.len(),
        args.samples
    );
    run_blender(&exe, &job, out)?;

    for shot in &job.shots {
        for o in &shot.outputs {
            let (_, _, lut) = canonical
                .iter()
                .find(|(id, _, _)| *id == o.camera)
                .expect("outputs name job cameras");
            let render = read_exr_combined(&out.join(&o.path))?;
            // Box-filter the supersampled render over each pixel (validated by G4.1).
            let image = remap_image_box(&render, lut, args.supersample.ceil() as u32);
            let png = out.join(format!("images/{}/{}.png", o.camera, shot.capture));
            write_png_srgb(&image, args.exposure, &png)?;
        }
    }
    println!("wrote {n} image(s) under {}", out.join("images").display());
    Ok(())
}
