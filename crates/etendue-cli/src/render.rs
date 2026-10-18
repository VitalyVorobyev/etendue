//! `etendue render --backend blender` (ADR 0005): bake, choose each camera's
//! canonical pinhole (ADR 0004), write `job.json`, run Blender with the
//! embedded script, then resample every EXR through the camera's remap LUT
//! into `images/<camera>/<capture>.png`. A camera that images a laser also
//! gets `<capture>_laser.png`: the stripe composited in Rust from the
//! render's passes (etendue-synth `laser_render`, P7-7).

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use etendue_scene::{BakedScenario, LaserSpec, SceneSpec};
use etendue_synth::images::{
    LinearImage, read_exr_combined, read_exr_passes, remap_image_box, write_png_raw, write_png_srgb,
};
use etendue_synth::job::{
    Device, JobMesh, LaserView, OutputKind, ROBOT_PASS_INDEX, RenderJob, RenderSettings,
    add_laser_views, build_job,
};
use etendue_synth::laser_render::{Beam, ShadowView, laser_image, laser_radiance, laser_views};
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
    /// Output directory (`job.json`, `exr/`, `images/`). The defaults of the
    /// other fields are [`DEFAULT_SAMPLES`], [`DEFAULT_SEED`],
    /// [`DEFAULT_SUPERSAMPLE`], [`DEFAULT_EXPOSURE`] and [`DEFAULT_AMBIENT`]
    /// (the CLI's and the studio's).
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
    /// Laser images: the fraction of the scene's light left on while the
    /// laser is ([`DEFAULT_LASER_AMBIENT`]).
    pub laser_ambient: f32,
}

/// Default Cycles samples per canonical pixel.
pub const DEFAULT_SAMPLES: u32 = 64;
/// Default Cycles seed.
pub const DEFAULT_SEED: u32 = 0;
/// Default canonical supersampling.
pub const DEFAULT_SUPERSAMPLE: f64 = 1.0;
/// Default linear scale before sRGB encoding.
pub const DEFAULT_EXPOSURE: f32 = 1.0;
/// Default radiance of the uniform white environment.
pub const DEFAULT_AMBIENT: f64 = 1.0;
/// Default fraction of the scene's light in laser images.
pub const DEFAULT_LASER_AMBIENT: f32 = 0.1;

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

    /// [`Self::find`], then [`Self::check`]: Blender ready to render, with
    /// any warning passed to `warn`.
    ///
    /// # Errors
    ///
    /// As [`Self::find`] and [`Self::check`].
    pub fn checked(
        arg: Option<&Path>,
        allow_other: bool,
        search: &[&Path],
        mut warn: impl FnMut(String),
    ) -> Result<Self> {
        let blender = Self::find(arg, search)?;
        if let Some(warning) = blender.check(allow_other)? {
            warn(warning);
        }
        Ok(blender)
    }

    /// Whether this Blender may render: `Ok(None)` if it matches its pin,
    /// `Ok(Some(warning))` if there is no pin or `allow_other` lets a
    /// mismatch through. A pin `X.Y` matches every `X.Y.Z` (a release
    /// series); a pin `X.Y.Z` matches that version only.
    ///
    /// # Errors
    ///
    /// If the version differs from the pin and `allow_other` is off.
    pub fn check(&self, allow_other: bool) -> Result<Option<String>> {
        let version = &self.version;
        match &self.pin {
            Some((pin, file)) if !matches_pin(version, pin) && !allow_other => bail!(
                "Blender {version} at {} but {} pins {pin}; install it, point --blender at it, \
                 or pass --allow-blender-version",
                self.exe.display(),
                file.display()
            ),
            Some((pin, _)) if !matches_pin(version, pin) => Ok(Some(format!(
                "warning: rendering with Blender {version}, pinned {pin}"
            ))),
            None => Ok(Some("warning: no etendue.toml Blender pin found".into())),
            _ => Ok(None),
        }
    }
}

/// Whether `version` is `pin`, or a release of the series `pin` (`5.2.1`
/// matches `5.2`, not `5.20`).
fn matches_pin(version: &str, pin: &str) -> bool {
    version == pin
        || version
            .strip_prefix(pin)
            .is_some_and(|rest| rest.starts_with('.'))
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

/// The pinned Blender version from the nearest `etendue.toml` with a
/// `[blender] version` above each of `search`, in order.
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
                // A file without a pin (other tool settings) does not end the search.
                if let Some(v) = version {
                    return Ok(Some((v, file)));
                }
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
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("running {}", exe.display()))?;
    // Both streams, line by line. Lines need not be UTF-8 (paths, locales):
    // a reader that stopped at one would close the pipe and make Blender's
    // next print fail.
    let (tx, rx) = mpsc::channel::<(bool, String)>();
    let forward = |stream: Box<dyn std::io::Read + Send>, is_err: bool| {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut stream = BufReader::new(stream);
            let mut buf = Vec::new();
            while matches!(stream.read_until(b'\n', &mut buf), Ok(n) if n > 0) {
                let line = String::from_utf8_lossy(&buf).trim_end().to_owned();
                buf.clear();
                if tx.send((is_err, line)).is_err() {
                    break;
                }
            }
        })
    };
    let readers = [
        forward(
            Box::new(child.stdout.take().expect("stdout is piped")),
            false,
        ),
        forward(
            Box::new(child.stderr.take().expect("stderr is piped")),
            true,
        ),
    ];
    drop(tx);
    // The end of stderr, for the error if Blender fails.
    let mut stderr_tail = std::collections::VecDeque::with_capacity(STDERR_TAIL);
    let total: usize = job.shots.iter().map(|s| s.outputs.len()).sum();
    let mut done = 0;
    loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok((is_err, line)) => {
                if is_err {
                    if stderr_tail.len() == STDERR_TAIL {
                        stderr_tail.pop_front();
                    }
                    stderr_tail.push_back(line.clone());
                }
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
            for r in readers {
                let _ = r.join();
            }
            return Err(Cancelled.into());
        }
    }
    for r in readers {
        let _ = r.join();
    }
    let status = child.wait()?;
    if !status.success() {
        let tail: Vec<String> = stderr_tail.into_iter().collect();
        if tail.is_empty() {
            bail!("Blender failed ({status})");
        }
        bail!("Blender failed ({status}):\n{}", tail.join("\n"));
    }
    Ok(())
}

/// Lines of Blender's stderr kept for the error of a failed run.
const STDERR_TAIL: usize = 20;

/// What [`run`] rendered.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RenderSummary {
    /// Images written under `images/<camera>/<capture>.png`.
    pub images: usize,
    /// Laser images written under `images/<camera>/<capture>_laser.png`.
    pub laser_images: usize,
    /// Cameras rendered.
    pub cameras: Vec<String>,
}

/// The lasers camera `camera` images: those linked to it, and those linked
/// to no camera (every camera images them).
fn imaged_by<'a>(scene: &'a SceneSpec, camera: &str) -> Vec<&'a LaserSpec> {
    scene
        .lasers
        .iter()
        .filter(|l| l.camera.as_deref().is_none_or(|c| c == camera))
        .collect()
}

/// Render every capture of `baked` with `blender` (ADR 0005): choose each
/// camera's canonical pinhole, write `job.json`, run Blender, then resample
/// every EXR through the camera's remap LUT into
/// `images/<camera>/<capture>.png`, and composite the laser images.
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

    let mut job = build_job(
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

    // Laser images: every laser a rendered camera images, seen from its apex
    // for its shadows.
    let lasers: Vec<&LaserSpec> = scene
        .lasers
        .iter()
        .filter(|l| {
            canonical
                .iter()
                .any(|(c, _, _)| imaged_by(scene, c).iter().any(|x| x.id == l.id))
        })
        .collect();
    let views: Vec<LaserView> = lasers
        .iter()
        .flat_map(|l| laser_views(l, PIXEL_CENTRE))
        .collect();
    if !lasers.is_empty() {
        add_laser_views(&mut job, baked, &views)?;
    }

    let out = &args.output;
    let images = |o: &&etendue_synth::job::JobOutput| o.kind == OutputKind::Image;
    let n: usize = job
        .shots
        .iter()
        .map(|s| s.outputs.iter().filter(images).count())
        .sum();
    ctl.log(format!(
        "rendering {n} image(s) with Blender {}: {} shot(s) × {} camera(s), {} samples{}",
        blender.version,
        job.shots.len(),
        canonical.len(),
        args.samples,
        if views.is_empty() {
            String::new()
        } else {
            format!(", and {} laser view(s) per shot", views.len())
        }
    ));
    run_blender(&blender.exe, &job, out, ctl)?;

    let frame_of = |name: &str| {
        baked
            .frames
            .iter()
            .position(|f| f == name)
            .ok_or_else(|| anyhow!("frame `{name}` is not in the baked scenario"))
    };
    let sensor = args.sensor.as_ref();
    let taps = args.supersample.ceil() as u32;
    let write = |image: &LinearImage, png: &Path, frame: u64| -> Result<()> {
        match sensor {
            // Temporal noise per image: the frame counter is unique within the job.
            Some(model) => {
                let raw = model.expose(&image.rgb, image.width, image.height, frame)?;
                write_png_raw(&raw, png)?;
            }
            None => write_png_srgb(image, args.exposure, png)?,
        }
        Ok(())
    };
    let (mut frame, mut done, mut laser_images) = (0u64, 0usize, 0usize);
    for shot in &job.shots {
        let sample = &baked.samples[shot.sample];
        // The lasers at this shot, each stopped by what its views see.
        let mut beams = Vec::with_capacity(lasers.len());
        for laser in &lasers {
            let world_se3_laser = sample.world_se3_frame[frame_of(&laser.id)?];
            let shadows = views
                .iter()
                .filter(|v| v.laser == laser.id)
                .map(|v| {
                    let passes = read_exr_passes(&out.join(v.path(&shot.capture)))?;
                    Ok(ShadowView::new(v, &world_se3_laser, &passes)?)
                })
                .collect::<Result<Vec<_>>>()?;
            beams.push((
                laser.id.as_str(),
                Beam::new(laser, &world_se3_laser, shadows)?,
            ));
        }
        for o in shot.outputs.iter().filter(images) {
            ctl.check()?;
            let (_, cam, lut) = canonical
                .iter()
                .find(|(id, _, _)| *id == o.camera)
                .expect("outputs name job cameras");
            let exr = out.join(&o.path);
            let render = read_exr_combined(&exr)?;
            // Box-filter the supersampled render over each pixel (validated by G4.1).
            let image = remap_image_box(&render, lut, taps);
            write(
                &image,
                &out.join(format!("images/{}/{}.png", o.camera, shot.capture)),
                frame,
            )?;
            frame += 1;
            let mine: Vec<&str> = imaged_by(scene, &o.camera)
                .iter()
                .map(|l| l.id.as_str())
                .collect();
            if !mine.is_empty() {
                let passes = read_exr_passes(&exr)?;
                let world_se3_camera = sample.world_se3_frame[frame_of(&o.camera)?];
                let lit: Vec<&Beam> = beams
                    .iter()
                    .filter(|(id, _)| mine.contains(id))
                    .map(|(_, b)| b)
                    .collect();
                let radiance = laser_radiance(cam, &world_se3_camera, &passes, &lit)?;
                let image = remap_image_box(
                    &laser_image(&render, &radiance, args.laser_ambient),
                    lut,
                    taps,
                );
                write(
                    &image,
                    &out.join(format!("images/{}/{}_laser.png", o.camera, shot.capture)),
                    frame,
                )?;
                frame += 1;
                laser_images += 1;
            }
            done += 1;
            ctl.step(Stage::Resample, done, n);
        }
    }
    ctl.log(format!(
        "wrote {n} image(s){} under {}",
        if laser_images > 0 {
            format!(" and {laser_images} laser image(s)")
        } else {
            String::new()
        },
        out.join("images").display()
    ));
    Ok(RenderSummary {
        images: n,
        laser_images,
        cameras: canonical.into_iter().map(|(id, _, _)| id).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blender(version: &str, pin: Option<&str>) -> Blender {
        Blender {
            exe: PathBuf::from("blender"),
            version: version.into(),
            pin: pin.map(|p| (p.into(), PathBuf::from("etendue.toml"))),
        }
    }

    #[test]
    fn a_series_pin_takes_its_patch_releases() {
        assert_eq!(blender("5.2.2", Some("5.2")).check(false).unwrap(), None);
        assert_eq!(blender("5.2.0", Some("5.2")).check(false).unwrap(), None);
        assert_eq!(blender("5.2", Some("5.2")).check(false).unwrap(), None);
        assert!(blender("5.20.1", Some("5.2")).check(false).is_err());
        assert!(blender("5.1.1", Some("5.2")).check(false).is_err());
    }

    #[test]
    fn a_full_pin_is_exact() {
        assert_eq!(blender("5.2.2", Some("5.2.2")).check(false).unwrap(), None);
        assert!(blender("5.2.1", Some("5.2.2")).check(false).is_err());
        assert!(blender("5.2.1", Some("5.2.2")).check(true).unwrap().is_some());
        assert!(blender("5.2.1", None).check(false).unwrap().is_some());
    }
}
