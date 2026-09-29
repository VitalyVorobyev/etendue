//! Tauri 2 shell of the etendue studio (ADR 0007).
//!
//! The React studio keeps its kernel in `@etendue/wasm` (bake, projection,
//! remap: per-frame math, in process). This shell adds what a browser cannot
//! do, on the `etendue_cli` library:
//!
//! - files: [`read_text`], [`path_exists`], [`repo_root`] — reading a file
//!   also lets the webview load assets from its directory (meshes, images)
//!   through the asset protocol;
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

/// Let the webview load files under `dir` through the asset protocol.
fn allow(app: &AppHandle, dir: &Path) {
    if let Err(e) = app.asset_protocol_scope().allow_directory(dir, true) {
        eprintln!("asset scope: {}: {e}", dir.display());
    }
}

/// Read a text file. Its directory (recursively) becomes loadable through
/// the asset protocol: a robot manifest's meshes sit below it.
#[tauri::command]
fn read_text(app: AppHandle, path: PathBuf) -> Result<String, String> {
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    if let Some(dir) = path.parent() {
        allow(&app, dir);
    }
    Ok(text)
}

/// Whether `path` is an existing file.
#[tauri::command]
fn path_exists(path: PathBuf) -> bool {
    path.is_file()
}

/// The etendue checkout this shell was built from, if it is still there (the
/// examples and the robot library live in it).
#[tauri::command]
fn repo_root() -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let root = root.canonicalize().ok()?;
    root.join("examples").is_dir().then_some(root)
}

/// Blender as it would render a scene in `scene_dir`.
#[tauri::command]
fn blender_status(exe: Option<PathBuf>, scene_dir: Option<PathBuf>) -> BlenderStatus {
    pipeline::blender_status(exe.as_deref(), scene_dir.as_deref())
}

/// A stop-and-shoot scenario for `robot` from a poses file (see
/// `etendue_cli::poses`).
#[tauri::command]
fn scenario_from_poses(
    path: PathBuf,
    robot: String,
    speed_scale: f64,
) -> Result<etendue_scene::ScenarioSpec, String> {
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
    request: DatasetRequest,
    on_progress: Channel<Progress>,
    runs: State<'_, Runs>,
) -> Result<Outcome, String> {
    let cancel = runs.start(&run_id);
    std::fs::create_dir_all(&request.output)
        .map_err(|e| format!("{}: {e}", request.output.display()))?;
    allow(&app, &request.output);
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
        .setup(|app| {
            // The examples and the robot library (meshes) of the checkout.
            if let Some(root) = repo_root() {
                for dir in ["examples", "assets/robots"] {
                    allow(app.handle(), &root.join(dir));
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            read_text,
            path_exists,
            repo_root,
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
    fn the_checkout_is_found() {
        let root = repo_root().expect("tests run inside the checkout");
        assert!(root.join("examples/closed_loop_ur5e/scene.json").is_file());
    }
}
