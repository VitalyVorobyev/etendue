//! `etendue render --backend blender` (ADR 0005): bake, choose each camera's
//! canonical pinhole (ADR 0004), write `job.json`, run Blender with the
//! embedded script, then resample every EXR through the camera's remap LUT
//! into `images/<camera>/<capture>.png`.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use etendue_scene::{BakedScenario, SceneSpec};
use etendue_synth::images::{read_exr_combined, remap_image_box, write_png_raw, write_png_srgb};
use etendue_synth::job::{Device, JobMesh, ROBOT_PASS_INDEX, RenderJob, RenderSettings, build_job};
use etendue_synth::sensor::SensorModel;
use etendue_synth::{CanonicalCamera, CanonicalSpec, PixelCentre, RemapLut, remap_lut};
use serde::Serialize;

use crate::load::Loaded;
use crate::progress::{Cancelled, Control, Stage};

const SCRIPT: &str = include_str!("../blender/etendue_blender/render.py");
const CONVERT: &str = include_str!("../blender/etendue_blender/convert.py");

/// Pixel-centre convention of the Blender backend's LUTs. **Provisional**:
/// probe P4-2 measures it (ADR 0004).
pub const PIXEL_CENTRE: PixelCentre = PixelCentre::Integer;

/// Options of a render.
#[derive(Clone, Debug)]
pub struct RenderOptions {
    /// Output directory (`job.json`, `exr/`, `images/`).
    pub output: PathBuf,
    /// Cycles samples per canonical pixel.
    pub samples: u32,
    /// Cycles seed.
    pub seed: u32,
    /// Render on the CPU (bit-exact, P4-5) instead of the GPU.
    pub cpu: bool,
    /// Canonical pixels per camera pixel at the image centre.
    pub supersample: f64,
    /// Linear scale before sRGB encoding (without a sensor model).
    pub exposure: f32,
    /// Radiance of a uniform white environment (0 = scene lights only).
    pub ambient: f64,
    /// Render only these cameras (all if empty).
    pub cameras: Vec<String>,
    /// Sensor model: images become raw mono PNGs at its bit depth. Without
    /// one, `exposure` and sRGB PNGs.
    pub sensor: Option<SensorModel>,
}

/// A Blender to render with.
#[derive(Clone, Debug, Serialize)]
pub struct Blender {
    /// The executable.
    pub exe: PathBuf,
    /// Its version, from `--version`.
    pub version: String,
    /// The `etendue.toml` pin and the file it is in, if one was found.
    pub pin: Option<(String, PathBuf)>,
}

impl Blender {
    /// Find Blender (`arg`, then `$ETENDUE_BLENDER`, then the platform
    /// default), read its version, and look for a pin in the nearest
    /// `etendue.toml` above each of `search`, in order.
    ///
    /// # Errors
    ///
    /// If Blender does not run or report a version, or an `etendue.toml`
    /// does not parse.
    pub fn find(arg: Option<&Path>, search: &[&Path]) -> Result<Self> {
        let exe = blender_path(arg);
        let version = blender_version(&exe)?;
        Ok(Self {
            exe,
            version,
            pin: pinned_version(search)?,
        })
    }

    /// Whether this Blender may render: `Ok(None)` if it matches its pin,
    /// `Ok(Some(warning))` if there is no pin or `allow_other` lets a
    /// mismatch through.
    ///
    /// # Errors
    ///
    /// If the version differs from the pin and `allow_other` is off.
    pub fn check(&self, allow_other: bool) -> Result<Option<String>> {
        let version = &self.version;
        match &self.pin {
            Some((pin, file)) if pin != version && !allow_other => bail!(
                "Blender {version} at {} but {} pins {pin}; install it, point --blender at it, \
                 or pass --allow-blender-version",
                self.exe.display(),
                file.display()
            ),
            Some((pin, _)) if pin != version => Ok(Some(format!(
                "warning: rendering with Blender {version}, pinned {pin}"
            ))),
            None => Ok(Some("warning: no etendue.toml Blender pin found".into())),
            _ => Ok(None),
        }
    }
}

/// The Blender executable: `arg`, then `$ETENDUE_BLENDER`, then the
/// platform default.
#[must_use]
pub fn blender_path(arg: Option<&Path>) -> PathBuf {
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

/// The pinned Blender version from the nearest `etendue.toml` above each of
/// `search`, in order.
fn pinned_version(search: &[&Path]) -> Result<Option<(String, PathBuf)>> {
    for &start in search {
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

/// The job meshes of `loaded`'s robots and parts.
///
/// # Errors
///
/// If a mesh file is missing (robot meshes are built by
/// `tools/robot-assets/build.py`).
pub fn meshes(loaded: &Loaded) -> Result<Vec<JobMesh>> {
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

/// Write `job` and the embedded script into `out`, and run Blender on them.
/// Blender's output goes to `ctl` as log lines, and every image it finishes
/// as a [`Stage::Render`] step. Cancelling stops Blender.
///
/// # Errors
///
/// If Blender does not start or fails, a file cannot be written, or the run
/// is cancelled ([`Cancelled`]).
pub fn run_blender(exe: &Path, job: &RenderJob, out: &Path, ctl: &Control) -> Result<()> {
    std::fs::create_dir_all(out)?;
    let job_path = out.join("job.json");
    std::fs::write(&job_path, serde_json::to_string_pretty(job)? + "\n")?;
    let scripts = out.join(".etendue_blender");
    std::fs::create_dir_all(&scripts)?;
    std::fs::write(scripts.join("render.py"), SCRIPT)?;
    std::fs::write(scripts.join("convert.py"), CONVERT)?;
    let mut child = Command::new(exe)
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
        .stdout(Stdio::piped())
        .spawn()
        .with_context(|| format!("running {}", exe.display()))?;
    let stdout = child.stdout.take().expect("stdout is piped");
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let total: usize = job.shots.iter().map(|s| s.outputs.len()).sum();
    let mut done = 0;
    loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => {
                // The script's marker, one per image (render.py).
                if line.starts_with("etendue: rendered ") {
                    done += 1;
                    ctl.step(Stage::Render, done, total);
                }
                ctl.log(line);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if ctl.cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return Err(Cancelled.into());
        }
    }
    let _ = reader.join();
    let status = child.wait()?;
    if !status.success() {
        bail!("Blender failed ({status})");
    }
    Ok(())
}

/// What [`run`] rendered.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RenderSummary {
    /// Images written under `images/<camera>/<capture>.png`.
    pub images: usize,
    /// Cameras rendered.
    pub cameras: Vec<String>,
}

/// Render every capture of `baked` with `blender` (ADR 0005): choose each
/// camera's canonical pinhole, write `job.json`, run Blender, then resample
/// every EXR through the camera's remap LUT into
/// `images/<camera>/<capture>.png`.
///
/// # Errors
///
/// If no camera is selected, a mesh is missing, Blender fails, a file cannot
/// be read or written, or the run is cancelled ([`Cancelled`]).
pub fn run(
    loaded: &Loaded,
    baked: &BakedScenario,
    blender: &Blender,
    args: &RenderOptions,
    ctl: &Control,
) -> Result<RenderSummary> {
    let scene: &SceneSpec = &loaded.scene;

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
    ctl.log(format!(
        "rendering {n} image(s) with Blender {}: {} shot(s) × {} camera(s), {} samples",
        blender.version,
        job.shots.len(),
        job.cameras.len(),
        args.samples
    ));
    run_blender(&blender.exe, &job, out, ctl)?;

    let sensor = args.sensor.as_ref();
    let mut frame = 0u64;
    for shot in &job.shots {
        for o in &shot.outputs {
            ctl.check()?;
            let (_, _, lut) = canonical
                .iter()
                .find(|(id, _, _)| *id == o.camera)
                .expect("outputs name job cameras");
            let render = read_exr_combined(&out.join(&o.path))?;
            // Box-filter the supersampled render over each pixel (validated by G4.1).
            let image = remap_image_box(&render, lut, args.supersample.ceil() as u32);
            let png = out.join(format!("images/{}/{}.png", o.camera, shot.capture));
            match sensor {
                // Temporal noise per image: the frame counter is unique within the job.
                Some(model) => {
                    let raw = model.expose(&image.rgb, image.width, image.height, frame)?;
                    write_png_raw(&raw, &png)?;
                }
                None => write_png_srgb(&image, args.exposure, &png)?,
            }
            frame += 1;
            ctl.step(Stage::Resample, frame as usize, n);
        }
    }
    ctl.log(format!(
        "wrote {n} image(s) under {}",
        out.join("images").display()
    ));
    Ok(RenderSummary {
        images: n,
        cameras: canonical.into_iter().map(|(id, _, _)| id).collect(),
    })
}
