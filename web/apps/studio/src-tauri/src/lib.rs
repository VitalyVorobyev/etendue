//! Tauri 2 shell of the etendue studio (ADR 0007).
//!
//! The React studio keeps its kernel in `@etendue/wasm` (bake, projection,
//! remap: per-frame math, in process). This shell adds what a browser cannot
//! do, on the `etendue_cli` library:
//!
//! - files: [`pick`], [`read_text`], [`path_exists`], [`data_root`]. The
//!   webview reads only under the data root (the checkout in development, the
//!   bundled resources in an installed app), the datasets folder, and what
//!   the user picked ([`Allowed`]), and loads assets (meshes, images) from
//!   the same folders through the asset protocol;
//! - dataset generation: [`generate_dataset`] (ground truth → Blender →
//!   detection, with progress over a channel) and [`cancel_run`];
//! - [`blender_status`] and [`scenario_from_poses`].

mod pipeline;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use etendue_cli::progress::{Control, Progress};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use pipeline::{BlenderStatus, DatasetRequest, Outcome};

/// Cancellation flags of the runs in flight, by caller-chosen run id.
#[derive(Default)]
struct Runs(Mutex<HashMap<String, Arc<AtomicBool>>>);

impl Runs {
    fn start(&self, id: &str) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        self.0
            .lock()
            .expect("runs lock")
            .insert(id.to_owned(), flag.clone());
        flag
    }

    fn finish(&self, id: &str) {
        self.0.lock().expect("runs lock").remove(id);
    }

    fn cancel(&self, id: &str) -> bool {
        self.0.lock().expect("runs lock").get(id).is_some_and(|f| {
            f.store(true, Ordering::Relaxed);
            true
        })
    }
}

/// `path` made absolute and free of `.`/`..` and links: its longest existing
/// ancestor canonicalised (without Windows' `\\?\` prefix, which `/`-joined
/// paths from the webview would break), the rest appended.
fn resolve(path: &Path) -> Option<PathBuf> {
    let mut existing = path;
    let mut rest = Vec::new();
    loop {
        if let Ok(p) = dunce::canonicalize(existing) {
            let mut out = p;
            for part in rest.iter().rev() {
                out.push(part);
            }
            return Some(out);
        }
        let name = existing.file_name()?;
        if name == ".." || name == "." {
            return None;
        }
        rest.push(name.to_owned());
        existing = existing.parent()?;
    }
}

/// The folders the webview may read under and load through the asset
/// protocol: the data root, the folders of the files and folders the user
/// picked, and the robot manifests a picked scene names.
#[derive(Default)]
struct Allowed(Mutex<Vec<PathBuf>>);

impl Allowed {
    fn add(&self, app: &AppHandle, dir: &Path) {
        let Some(dir) = resolve(dir) else { return };
        if let Err(e) = app.asset_protocol_scope().allow_directory(&dir, true) {
            eprintln!("asset scope: {}: {e}", dir.display());
        }
        self.0.lock().expect("allowed lock").push(dir);
    }

    /// `path` resolved, if it is under an allowed folder.
    fn check(&self, path: &Path) -> Result<PathBuf, String> {
        let resolved = resolve(path).ok_or_else(|| format!("{}: no such path", path.display()))?;
        if self
            .0
            .lock()
            .expect("allowed lock")
            .iter()
            .any(|dir| resolved.starts_with(dir))
        {
            Ok(resolved)
        } else {
            Err(format!(
                "{}: outside the folders opened in the studio",
                path.display()
            ))
        }
    }
}

/// The robot manifest folders a scene file names (relative to it), so a
/// picked scene can load robots kept elsewhere.
fn manifest_dirs(scene: &Path) -> Vec<PathBuf> {
    #[derive(serde::Deserialize)]
    struct Robot {
        manifest: PathBuf,
    }
    #[derive(serde::Deserialize)]
    struct Scene {
        #[serde(default)]
        robots: Vec<Robot>,
    }
    let Some(dir) = scene.parent() else {
        return vec![];
    };
    std::fs::read_to_string(scene)
        .ok()
        .and_then(|t| serde_json::from_str::<Scene>(&t).ok())
        .map(|s| {
            s.robots
                .iter()
                .filter_map(|r| dir.join(&r.manifest).parent().map(Path::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Show a file dialog: JSON files (`multiple` or one), or a folder
/// (`directory`). What the user picks becomes readable.
#[tauri::command]
async fn pick(
    app: AppHandle,
    title: String,
    multiple: bool,
    directory: bool,
    allowed: State<'_, Allowed>,
) -> Result<Vec<PathBuf>, String> {
    use tauri_plugin_dialog::DialogExt;
    let dialog = app.dialog().file().set_title(title);
    let picked = if directory {
        dialog.blocking_pick_folder().into_iter().collect()
    } else if multiple {
        dialog
            .add_filter("JSON", &["json"])
            .blocking_pick_files()
            .unwrap_or_default()
    } else {
        dialog
            .add_filter("JSON", &["json"])
            .blocking_pick_file()
            .into_iter()
            .collect()
    };
    let paths: Vec<PathBuf> = picked
        .into_iter()
        .filter_map(|p| p.into_path().ok())
        .collect();
    for path in &paths {
        if directory {
            allowed.add(&app, path);
        } else {
            if let Some(dir) = path.parent() {
                allowed.add(&app, dir);
            }
            for dir in manifest_dirs(path) {
                allowed.add(&app, &dir);
            }
        }
    }
    Ok(paths)
}

/// Read a text file under an allowed folder.
#[tauri::command]
fn read_text(path: PathBuf, allowed: State<'_, Allowed>) -> Result<String, String> {
    let path = allowed.check(&path)?;
    std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Whether `path` is an existing file under an allowed folder.
#[tauri::command]
fn path_exists(path: PathBuf, allowed: State<'_, Allowed>) -> bool {
    allowed.check(&path).is_ok_and(|p| p.is_file())
}

/// The etendue checkout this shell was built from, if it is still there and
/// this is a development build: a release bundle uses its own resources, even
/// on the machine that built it.
fn checkout() -> Option<PathBuf> {
    if !cfg!(debug_assertions) {
        return None;
    }
    let root =
        dunce::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")).ok()?;
    root.join("examples").is_dir().then_some(root)
}

/// Where the examples, the robot library and the `etendue.toml` Blender pin
/// are (`root`), and where datasets go by default (`datasets`).
#[derive(Clone, Debug, serde::Serialize)]
struct DataRoot {
    root: PathBuf,
    datasets: PathBuf,
}

impl DataRoot {
    /// The checkout, writing under its `target/studio/`; else the app's
    /// bundled resources (read-only), writing under `Documents/etendue`.
    fn find(app: &AppHandle) -> Option<Self> {
        if let Some(root) = checkout() {
            let datasets = root.join("target/studio");
            return Some(Self { root, datasets });
        }
        let root = dunce::canonicalize(app.path().resource_dir().ok()?).ok()?;
        if !root.join("examples").is_dir() {
            return None;
        }
        let documents = app
            .path()
            .document_dir()
            .or_else(|_| app.path().app_data_dir())
            .ok()?;
        Some(Self {
            root,
            datasets: documents.join("etendue"),
        })
    }
}

/// The data root, if there is one.
#[tauri::command]
fn data_root(root: State<'_, Option<DataRoot>>) -> Option<DataRoot> {
    root.inner().clone()
}

/// Blender as it would render a scene in `scene_dir`.
#[tauri::command]
fn blender_status(
    exe: Option<PathBuf>,
    scene_dir: Option<PathBuf>,
    root: State<'_, Option<DataRoot>>,
) -> BlenderStatus {
    let pin_dir = root.inner().as_ref().map(|r| r.root.as_path());
    pipeline::blender_status(exe.as_deref(), scene_dir.as_deref(), pin_dir)
}

/// A stop-and-shoot scenario for `robot` from a poses file (see
/// `etendue_cli::poses`).
#[tauri::command]
fn scenario_from_poses(
    path: PathBuf,
    robot: String,
    speed_scale: f64,
    allowed: State<'_, Allowed>,
) -> Result<etendue_scene::ScenarioSpec, String> {
    let path = allowed.check(&path)?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    etendue_cli::poses::scenario(&text, &robot, speed_scale, 0.01)
        .map_err(|e| format!("{}: {e:#}", path.display()))
}

/// Generate a dataset (ground truth, then optionally render and detect) on a
/// worker thread, streaming progress. `run_id` names the run for
/// [`cancel_run`].
#[tauri::command]
async fn generate_dataset(
    app: AppHandle,
    run_id: String,
    mut request: DatasetRequest,
    on_progress: Channel<Progress>,
    runs: State<'_, Runs>,
    allowed: State<'_, Allowed>,
    root: State<'_, Option<DataRoot>>,
) -> Result<Outcome, String> {
    request.pin_dir = root.inner().as_ref().map(|r| r.root.clone());
    // Every path the run reads or writes is one the studio opened.
    request.scene = allowed.check(&request.scene)?;
    request.output = allowed.check(&request.output)?;
    if let Some(sensor) = &request.sensor {
        request.sensor = Some(allowed.check(sensor)?);
    }
    std::fs::create_dir_all(&request.output)
        .map_err(|e| format!("{}: {e}", request.output.display()))?;
    allowed.add(&app, &request.output);
    let cancel = runs.start(&run_id);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let report = move |p: Progress| {
            let _ = on_progress.send(p);
        };
        pipeline::generate(&request, &Control::new(&report, Some(&cancel)))
    })
    .await;
    runs.finish(&run_id);
    result.map_err(|e| format!("dataset run panicked: {e}"))
}

/// Ask a run to stop. `true` if it was still running.
#[tauri::command]
fn cancel_run(run_id: String, runs: State<'_, Runs>) -> bool {
    runs.cancel(&run_id)
}

/// Start the app.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Runs::default())
        .manage(Allowed::default())
        .setup(|app| {
            // The examples, the robot library (meshes) and the datasets folder.
            let root = DataRoot::find(app.handle());
            match &root {
                Some(r) => eprintln!(
                    "etendue studio: data root {}, datasets in {}",
                    r.root.display(),
                    r.datasets.display()
                ),
                None => eprintln!("etendue studio: no data root (no examples bundled)"),
            }
            if let Some(root) = &root {
                let allowed = app.state::<Allowed>();
                allowed.add(app.handle(), &root.root);
                allowed.add(app.handle(), &root.datasets);
            }
            app.manage(root);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            pick,
            read_text,
            path_exists,
            data_root,
            blender_status,
            scenario_from_poses,
            generate_dataset,
            cancel_run,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the etendue studio");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The checkout the tests run in (in any profile).
    fn repo() -> PathBuf {
        dunce::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")).unwrap()
    }

    #[test]
    fn runs_cancel_only_while_running() {
        let runs = Runs::default();
        let flag = runs.start("a");
        assert!(!runs.cancel("b"));
        assert!(runs.cancel("a"));
        assert!(flag.load(Ordering::Relaxed));
        runs.finish("a");
        assert!(!runs.cancel("a"));
    }

    #[test]
    fn resolve_handles_missing_tails_and_rejects_dot_dot() {
        let root = repo();
        let missing = root.join("target/studio/not-yet/created");
        assert_eq!(resolve(&missing), Some(missing.clone()));
        // `..` through an existing folder is folded by canonicalisation …
        assert_eq!(
            resolve(&root.join("examples/../examples")),
            Some(root.join("examples"))
        );
        // … but cannot climb out of a missing one.
        assert_eq!(resolve(&root.join("target/nope/../../..")), None);
    }

    #[test]
    fn manifest_dirs_follow_the_scene() {
        let root = repo();
        let dirs = manifest_dirs(&root.join("examples/closed_loop_ur5e/scene.json"));
        assert!(!dirs.is_empty());
        for d in dirs {
            assert!(
                resolve(&d).is_some_and(|d| d.join("robot.json").is_file()),
                "{}",
                d.display()
            );
        }
    }

    #[test]
    #[cfg(debug_assertions)]
    fn the_checkout_is_found() {
        let root = checkout().expect("tests run inside the checkout");
        assert!(root.join("examples/closed_loop_ur5e/scene.json").is_file());
    }
}
