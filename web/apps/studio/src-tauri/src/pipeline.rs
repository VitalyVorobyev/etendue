//! Dataset generation: ground truth, Blender render, detection — the
//! `etendue gt | render | detect` steps in one run, on the `etendue_cli`
//! library. Plain functions, so the Tauri commands stay thin and the steps
//! are testable without a window.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use etendue_cli::detect::{self, CameraSummary};
use etendue_cli::gt::{self, GtSummary};
use etendue_cli::progress::{Cancelled, Control, Stage};
use etendue_cli::render::{self, Blender, RenderOptions, RenderSummary};
use etendue_cli::{load, read_json};
use etendue_scene::ScenarioSpec;
use etendue_synth::sensor::SensorModel;
use serde::{Deserialize, Serialize};

use crate::docs;

/// What to generate.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DatasetRequest {
    /// The scene, as edited in the studio. It is written to the output
    /// folder as `scene.json` (its robot manifests rebased), and the run
    /// reads it from there, so a dataset carries the scene it came from.
    pub scene: serde_json::Value,
    /// The scene file it was read from, if any: its manifests resolve
    /// relative to it, else in the robot library (the data root).
    pub origin: Option<PathBuf>,
    /// The scenario, as loaded in the studio (it may not exist as a file).
    pub scenario: ScenarioSpec,
    /// Output directory.
    pub output: PathBuf,
    /// Render with Blender (otherwise ground truth only).
    pub render: bool,
    /// Cycles samples per canonical pixel.
    pub samples: u32,
    /// Canonical supersampling.
    pub supersample: f64,
    /// Sensor model file (linear raw PNGs); none: sRGB PNGs.
    pub sensor: Option<PathBuf>,
    /// Cameras to render (all if empty).
    pub cameras: Vec<String>,
    /// Render on the CPU (bit-exact).
    pub cpu: bool,
    /// Blender executable (default: `$ETENDUE_BLENDER`, then the platform's).
    pub blender: Option<PathBuf>,
    /// Render even if Blender's version differs from the `etendue.toml` pin.
    pub allow_blender_version: bool,
    /// The data root (set by the shell, not the webview): where else to look
    /// for the Blender pin, after the scene's folder, and the robot library.
    #[serde(skip)]
    pub pin_dir: Option<PathBuf>,
}

/// What a run produced.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dataset {
    /// Output directory.
    pub output: PathBuf,
    /// The ground truth written.
    pub gt: GtSummary,
    /// The render, if one ran.
    pub render: Option<RenderSummary>,
    /// Detection per camera (after a render).
    pub detection: Vec<CameraSummary>,
}

/// The end of a run.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Outcome {
    /// Done.
    Ok(Dataset),
    /// Stopped by `cancel_run`.
    Cancelled,
    /// Failed; the error chain.
    Failed {
        /// Human-readable error.
        message: String,
    },
}

/// Run `request`, reporting through `ctl`.
pub fn generate(request: &DatasetRequest, ctl: &Control) -> Outcome {
    match run(request, ctl) {
        Ok(d) => Outcome::Ok(d),
        Err(e) if e.downcast_ref::<Cancelled>().is_some() => Outcome::Cancelled,
        Err(e) => Outcome::Failed {
            message: format!("{e:#}"),
        },
    }
}

fn run(request: &DatasetRequest, ctl: &Control) -> Result<Dataset> {
    // Everything that can fail cheaply first: a bad sensor file or Blender
    // should not cost the ground truth.
    let sensor: Option<SensorModel> = match &request.sensor {
        Some(p) => Some(read_json(p, "sensor model")?),
        None => None,
    };
    let out = &request.output;
    std::fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;
    let mut scene = request.scene.clone();
    docs::rebase_manifests(
        &mut scene,
        request.origin.as_deref().and_then(Path::parent),
        out,
        request.pin_dir.as_deref(),
    )?;
    let scene_path = out.join("scene.json");
    docs::write_json(&scene_path, &scene)?;
    let loaded = load(&scene_path)?;
    let baked = etendue_kinematics::bake(&loaded.scene, &request.scenario, &loaded.models)
        .map_err(|e| anyhow!("scenario: {e}"))?;
    let blender = if request.render {
        Some(Blender::checked(
            request.blender.as_deref(),
            request.allow_blender_version,
            &[
                request.origin.as_deref().and_then(Path::parent),
                Some(loaded.dir.as_path()),
                request.pin_dir.as_deref(),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>(),
            |w| ctl.log(w),
        )?)
    } else {
        None
    };
    ctl.check()?;

    // A previous run's images and features belong to its ground truth: a
    // camera this run does not render must not keep them, or detection
    // would check old images against the new truth.
    for stale in ["images", "exr"] {
        let dir = out.join(stale);
        if dir.is_dir() {
            std::fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))?;
        }
    }
    let features = out.join("features.json");
    if features.is_file() {
        std::fs::remove_file(&features)
            .with_context(|| format!("removing {}", features.display()))?;
    }

    ctl.step(Stage::GroundTruth, 0, 1);
    let gt = gt::write(&loaded, &baked, out)?;
    // The scenario the dataset came from, which the studio may hold only in
    // memory (imported poses).
    std::fs::write(
        out.join("scenario.json"),
        serde_json::to_string_pretty(&request.scenario)? + "\n",
    )
    .with_context(|| format!("writing {}", out.join("scenario.json").display()))?;
    ctl.step(Stage::GroundTruth, 1, 1);
    ctl.log(format!("{gt} → {}", out.display()));

    let Some(blender) = blender else {
        return Ok(Dataset {
            output: out.clone(),
            gt,
            render: None,
            detection: vec![],
        });
    };
    let render = render::run(
        &loaded,
        &baked,
        &blender,
        &RenderOptions {
            output: out.clone(),
            samples: request.samples,
            seed: render::DEFAULT_SEED,
            cpu: request.cpu,
            supersample: request.supersample,
            exposure: render::DEFAULT_EXPOSURE,
            ambient: render::DEFAULT_AMBIENT,
            cameras: request.cameras.clone(),
            sensor,
            laser_ambient: render::DEFAULT_LASER_AMBIENT,
        },
        ctl,
    )?;
    let features = detect::run(out, ctl)?;
    let detection = features.summary();
    for line in detect::summary_table(&detection) {
        ctl.log(line);
    }
    Ok(Dataset {
        output: out.clone(),
        gt,
        render: Some(render),
        detection,
    })
}

/// Blender as the studio shows it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlenderStatus {
    /// Found and runnable.
    pub found: bool,
    /// The executable tried.
    pub exe: PathBuf,
    /// Its version, if it ran.
    pub version: Option<String>,
    /// The pinned version, if a pin was found above the scene.
    pub pin: Option<String>,
    /// Why it cannot render, or a warning.
    pub message: Option<String>,
}

/// Blender's status for a scene in `scene_dir`, with the pin looked up there
/// and then in `pin_dir`.
pub fn blender_status(
    exe: Option<&Path>,
    scene_dir: Option<&Path>,
    pin_dir: Option<&Path>,
) -> BlenderStatus {
    let search: Vec<&Path> = scene_dir.into_iter().chain(pin_dir).collect();
    match Blender::find(exe, &search) {
        Ok(b) => {
            let check = b.check(false);
            BlenderStatus {
                found: true,
                exe: b.exe.clone(),
                version: Some(b.version.clone()),
                pin: b.pin.as_ref().map(|(v, _)| v.clone()),
                message: match check {
                    Ok(warning) => warning,
                    Err(e) => Some(format!("{e:#}")),
                },
            }
        }
        Err(e) => BlenderStatus {
            found: false,
            exe: render::blender_path(exe),
            version: None,
            pin: None,
            message: Some(format!("{e:#}")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicBool;

    fn repo() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
    }

    fn request(output: PathBuf) -> DatasetRequest {
        let example = repo().join("examples/closed_loop_ur5e");
        DatasetRequest {
            scene: read_json(&example.join("scene.json"), "scene").unwrap(),
            origin: Some(example.join("scene.json")),
            scenario: read_json(&example.join("scenario.json"), "scenario").unwrap(),
            output,
            render: false,
            samples: 16,
            supersample: 4.0,
            sensor: None,
            cameras: vec![],
            cpu: false,
            blender: None,
            allow_blender_version: false,
            pin_dir: None,
        }
    }

    #[test]
    fn ground_truth_only() {
        let dir = tempfile::tempdir().unwrap();
        // A previous run's output, which the new ground truth invalidates.
        let old = dir.path().join("images/cam_right/cap_000.png");
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        std::fs::write(&old, b"old").unwrap();
        std::fs::write(dir.path().join("features.json"), b"{}").unwrap();
        let events = Mutex::new(Vec::new());
        let report = |p| events.lock().unwrap().push(p);
        let outcome = generate(
            &request(dir.path().to_owned()),
            &Control::new(&report, None),
        );
        let Outcome::Ok(d) = outcome else {
            panic!("{outcome:?}")
        };
        assert_eq!((d.gt.captures, d.gt.views, d.gt.visible), (24, 48, 2592));
        assert!(d.render.is_none());
        for f in [
            "dataset.json",
            "gt.json",
            "robot_poses.json",
            "scene.json",
            "scenario.json",
        ] {
            assert!(dir.path().join(f).is_file(), "{f}");
        }
        assert!(!dir.path().join("images").exists());
        assert!(!dir.path().join("features.json").exists());
        let events = events.into_inner().unwrap();
        assert!(events.contains(&etendue_cli::progress::Progress::Step {
            stage: Stage::GroundTruth,
            done: 1,
            total: 1
        }));
    }

    #[test]
    fn cancelled_before_it_starts() {
        let dir = tempfile::tempdir().unwrap();
        let cancel = AtomicBool::new(true);
        let outcome = generate(
            &request(dir.path().to_owned()),
            &Control::new(&|_| {}, Some(&cancel)),
        );
        assert!(matches!(outcome, Outcome::Cancelled), "{outcome:?}");
    }

    /// Local only (needs Blender and the robot meshes):
    /// `cargo test --manifest-path … -- --ignored`.
    fn render_request(dir: &Path) -> DatasetRequest {
        let mut r = request(dir.to_owned());
        r.render = true;
        r.samples = 4;
        r.supersample = 1.0;
        r.cameras = vec!["cam_left".into()];
        r.sensor = Some(repo().join("examples/closed_loop_ur5e/sensor_linear.json"));
        r
    }

    #[test]
    #[ignore = "needs Blender and the robot meshes"]
    fn renders_and_detects_with_blender() {
        let dir = tempfile::tempdir().unwrap();
        let steps = Mutex::new(Vec::new());
        let report = |p| {
            if let etendue_cli::progress::Progress::Step { stage, done, .. } = p {
                steps.lock().unwrap().push((stage, done));
            }
        };
        let outcome = generate(&render_request(dir.path()), &Control::new(&report, None));
        let Outcome::Ok(d) = outcome else {
            panic!("{outcome:?}")
        };
        assert_eq!(d.render.as_ref().map(|r| r.images), Some(20));
        let left = &d.detection[0];
        assert_eq!(
            (left.camera.as_str(), left.ok, left.mislabelled),
            ("cam_left", 20, 0)
        );
        let steps = steps.into_inner().unwrap();
        assert!(
            steps.contains(&(Stage::Render, 20)),
            "every image is a render step"
        );
        assert!(steps.contains(&(Stage::Detect, 40)));
    }

    #[test]
    #[ignore = "needs Blender and the robot meshes"]
    fn cancelling_stops_blender() {
        let dir = tempfile::tempdir().unwrap();
        let cancel = AtomicBool::new(false);
        let report = |p| {
            // Cancel as soon as the first image is rendered.
            if let etendue_cli::progress::Progress::Step {
                stage: Stage::Render,
                done: 1,
                ..
            } = p
            {
                cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        };
        let t = std::time::Instant::now();
        let outcome = generate(
            &render_request(dir.path()),
            &Control::new(&report, Some(&cancel)),
        );
        assert!(matches!(outcome, Outcome::Cancelled), "{outcome:?}");
        let images = std::fs::read_dir(dir.path().join("exr/cam_left"))
            .map(|d| d.count())
            .unwrap_or(0);
        assert!(
            images < 20,
            "Blender stopped early ({images} images, {:?})",
            t.elapsed()
        );
    }

    #[test]
    fn a_bad_sensor_file_fails_first() {
        let dir = tempfile::tempdir().unwrap();
        let mut r = request(dir.path().join("out"));
        r.sensor = Some(dir.path().join("missing.json"));
        let Outcome::Failed { message } = generate(&r, &Control::new(&|_| {}, None)) else {
            panic!("expected a failure")
        };
        assert!(message.contains("sensor model"), "{message}");
        assert!(!dir.path().join("out").exists());
    }

    #[test]
    fn an_edited_scene_without_a_file_finds_its_robots_in_the_library() {
        let dir = tempfile::tempdir().unwrap();
        let mut r = request(dir.path().to_owned());
        r.origin = None;
        r.pin_dir = Some(repo());
        r.scene["cameras"][0]["resolution"] = serde_json::json!([640, 512]);
        let outcome = generate(&r, &Control::new(&|_| {}, None));
        assert!(matches!(outcome, Outcome::Ok(_)), "{outcome:?}");
        let written: serde_json::Value =
            read_json(&dir.path().join("scene.json"), "scene").unwrap();
        assert_eq!(written["cameras"][0]["resolution"][0], 640);
    }
}
