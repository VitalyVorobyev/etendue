//! Tauri 2 shell of the etendue studio (ADR 0007).
//!
//! The React studio keeps its kernel in `@etendue/wasm` (bake, projection,
//! remap: per-frame math, in process). This shell adds what a browser cannot
//! do, on the `etendue_cli` library:
//!
//! - files: [`pick`], [`read_text`], [`path_exists`], [`data_root`]. The
//!   webview reads only under the data root (the checkout in development, the
//!   bundled resources in an installed app), the datasets folder, and what
//!   the user picked ([`Access`]), and loads assets (meshes, images) from
//!   the same folders through the asset protocol;
//! - dataset generation: [`generate_dataset`] (ground truth → Blender →
//!   detection, with progress over a channel) and [`cancel_run`];
//! - documents: [`save_dialog`], [`write_text`] and [`save_scene`]. The
//!   webview writes only under the folders the user chose in a dialog (to
//!   save or to pick) and the datasets folder ([`Access`]) — never into the
//!   data root, so an example is saved elsewhere;
//! - [`blender_status`] and [`scenario_from_poses`].

mod docs;
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

/// A list of folders, and whether a path is under one of them.
#[derive(Default)]
struct Folders(Mutex<Vec<PathBuf>>);

impl Folders {
    /// Add `dir` (resolved); its resolved form, if it resolves.
    fn add(&self, dir: &Path) -> Option<PathBuf> {
        let dir = resolve(dir)?;
        self.0.lock().expect("folders lock").push(dir.clone());
        Some(dir)
    }

    /// `path` resolved, if it is under one of the folders.
    fn check(&self, path: &Path, outside: &str) -> Result<PathBuf, String> {
        let resolved = resolve(path).ok_or_else(|| format!("{}: no such path", path.display()))?;
        if self
            .0
            .lock()
            .expect("folders lock")
            .iter()
            .any(|dir| resolved.starts_with(dir))
        {
            Ok(resolved)
        } else {
            Err(format!("{}: {outside}", path.display()))
        }
    }
}

/// The folders the webview may use. It reads under, and loads through the
/// asset protocol from, the data root, the folders of the files and folders
/// the user picked or saved to, and the robot manifests a picked scene names.
/// It writes only under the folders the user chose in a dialog and the
/// datasets folder — never into the data root.
#[derive(Default)]
struct Access {
    read: Folders,
    write: Folders,
}

impl Access {
    /// Let the webview read under `dir`.
    fn allow_read(&self, app: &AppHandle, dir: &Path) {
        let Some(dir) = self.read.add(dir) else {
            return;
        };
        if let Err(e) = app.asset_protocol_scope().allow_directory(&dir, true) {
            eprintln!("asset scope: {}: {e}", dir.display());
        }
    }

    /// Let the webview read and write under `dir`.
    fn allow_write(&self, app: &AppHandle, dir: &Path) {
        self.allow_read(app, dir);
        self.write.add(dir);
    }

    /// `path` resolved, if the webview may read it.
    fn readable(&self, path: &Path) -> Result<PathBuf, String> {
        self.read
            .check(path, "outside the folders opened in the studio")
    }

    /// `path` resolved, if the webview may write it.
    fn writable(&self, path: &Path) -> Result<PathBuf, String> {
        self.write.check(
            path,
            "not in a folder chosen in the studio (use Save As… to choose one)",
        )
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
    access: State<'_, Access>,
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
            access.allow_write(&app, path);
        } else {
            if let Some(dir) = path.parent() {
                access.allow_write(&app, dir);
            }
            for dir in manifest_dirs(path) {
                access.allow_read(&app, &dir);
            }
        }
    }
    Ok(paths)
}

/// Show a save dialog for a file named `name` by default, filtered to
/// `extensions` (without dots) and starting in `dir` if given. The chosen
/// file's folder becomes readable and writable. `None` if cancelled.
#[tauri::command]
async fn save_dialog(
    app: AppHandle,
    title: String,
    name: String,
    extensions: Vec<String>,
    dir: Option<PathBuf>,
    access: State<'_, Access>,
) -> Result<Option<PathBuf>, String> {
    use tauri_plugin_dialog::DialogExt;
    let mut dialog = app.dialog().file().set_title(title).set_file_name(name);
    if !extensions.is_empty() {
        let ext: Vec<&str> = extensions.iter().map(String::as_str).collect();
        dialog = dialog.add_filter(ext.join(", "), &ext);
    }
    if let Some(dir) = dir.filter(|d| d.is_dir()) {
        dialog = dialog.set_directory(dir);
    }
    let Some(path) = dialog.blocking_save_file().and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    if let Some(dir) = path.parent() {
        access.allow_write(&app, dir);
    }
    Ok(Some(path))
}

/// Write a text file in a writable folder, creating the folders it needs there (a project's
/// `library/cameras/`, say).
#[tauri::command]
fn write_text(path: PathBuf, text: String, access: State<'_, Access>) -> Result<(), String> {
    let path = access.writable(&path)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Open the project folder `dir` (ADR 0009): it must hold an `etendue-project.json` that
/// parses. The folder becomes readable and writable (it is the user's), and so does nothing
/// else. Returns the manifest.
#[tauri::command]
fn open_project(
    app: AppHandle,
    dir: PathBuf,
    access: State<'_, Access>,
) -> Result<etendue_scene::ProjectManifest, String> {
    let dir = resolve(&dir)
        .filter(|d| d.is_dir())
        .ok_or_else(|| format!("{}: not a folder", dir.display()))?;
    let path = dir.join(etendue_scene::PROJECT_FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let manifest: etendue_scene::ProjectManifest =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    manifest
        .validate()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    access.allow_write(&app, &dir);
    Ok(manifest)
}

/// Save a scene (and its scenario) to `scene_path` (and `scenario_path`),
/// both in writable folders. The scene's robot manifest paths are rewritten
/// relative to its new folder: they are found relative to `from` (the folder
/// it was read from), else in the robot library. Returns the scene as written.
#[tauri::command]
fn save_scene(
    scene_path: PathBuf,
    mut scene: serde_json::Value,
    scenario_path: Option<PathBuf>,
    scenario: Option<serde_json::Value>,
    from: Option<PathBuf>,
    access: State<'_, Access>,
    root: State<'_, Option<DataRoot>>,
) -> Result<serde_json::Value, String> {
    let scene_path = access.writable(&scene_path)?;
    let scenario = match (scenario_path, scenario) {
        (Some(path), Some(doc)) => Some((access.writable(&path)?, doc)),
        (None, None) => None,
        _ => return Err("a scenario needs both a path and a document".into()),
    };
    let dir = scene_path
        .parent()
        .ok_or_else(|| format!("{}: no folder", scene_path.display()))?;
    let library = root.inner().as_ref().map(|r| r.root.as_path());
    docs::rebase_manifests(&mut scene, from.as_deref(), dir, library)
        .map_err(|e| format!("{e:#}"))?;
    docs::write_json(&scene_path, &scene).map_err(|e| format!("{e:#}"))?;
    if let Some((path, doc)) = scenario {
        docs::write_json(&path, &doc).map_err(|e| format!("{e:#}"))?;
    }
    Ok(scene)
}

/// Read a text file under an allowed folder.
#[tauri::command]
fn read_text(path: PathBuf, access: State<'_, Access>) -> Result<String, String> {
    let path = access.readable(&path)?;
    std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Whether `path` is an existing file under an allowed folder.
#[tauri::command]
fn path_exists(path: PathBuf, access: State<'_, Access>) -> bool {
    access.readable(&path).is_ok_and(|p| p.is_file())
}

/// The names of the files in folder `path`, sorted, if it is under an allowed folder (a
/// project's library folder, say). Subfolders are left out.
#[tauri::command]
fn list_dir(path: PathBuf, access: State<'_, Access>) -> Result<Vec<String>, String> {
    let dir = access.readable(&path)?;
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            entry
                .file_type()
                .ok()?
                .is_file()
                .then(|| entry.file_name().to_string_lossy().into_owned())
        })
        .collect();
    names.sort();
    Ok(names)
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
    /// bundled resources (read-only).
    fn find(app: &AppHandle) -> Option<Self> {
        if let Some(root) = checkout() {
            let datasets = root.join("target/studio");
            return Some(Self { root, datasets });
        }
        let documents = app
            .path()
            .document_dir()
            .or_else(|_| app.path().app_data_dir())
            .ok();
        Self::bundled(app.path().resource_dir().ok()?, documents)
    }

    /// The bundled resources at `resources` (if they hold the examples),
    /// writing under `<documents>/etendue`, or the temporary folder when
    /// there is no documents folder: a missing datasets folder does not
    /// discard a valid root.
    fn bundled(resources: PathBuf, documents: Option<PathBuf>) -> Option<Self> {
        let root = dunce::canonicalize(resources).ok()?;
        if !root.join("examples").is_dir() {
            return None;
        }
        let datasets = documents.unwrap_or_else(std::env::temp_dir).join("etendue");
        Some(Self { root, datasets })
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
    access: State<'_, Access>,
) -> Result<etendue_scene::ScenarioSpec, String> {
    let path = access.readable(&path)?;
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
    access: State<'_, Access>,
    root: State<'_, Option<DataRoot>>,
) -> Result<Outcome, String> {
    request.pin_dir = root.inner().as_ref().map(|r| r.root.clone());
    // Every path the run reads or writes is one the studio opened.
    if let Some(origin) = &request.origin {
        request.origin = Some(access.readable(origin)?);
    }
    request.output = access.writable(&request.output)?;
    if let Some(sensor) = &request.sensor {
        request.sensor = Some(access.readable(sensor)?);
    }
    std::fs::create_dir_all(&request.output)
        .map_err(|e| format!("{}: {e}", request.output.display()))?;
    access.allow_write(&app, &request.output);
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

/// How a calibration run ended.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CalibrationOutcome {
    Ok {
        #[serde(flatten)]
        run: etendue_cli::calibrate::Run,
    },
    Cancelled,
    Failed {
        message: String,
    },
}

/// Calibrate the dataset in `dir` natively (`etendue calibrate`) on a worker thread, one progress
/// step per trial; `run_id` names the run for [`cancel_run`]. The run is also written to
/// `dir/calibration.json`.
#[tauri::command]
async fn calibrate_dataset(
    run_id: String,
    dir: PathBuf,
    options: etendue_cli::calibrate::RunOptions,
    on_progress: Channel<Progress>,
    runs: State<'_, Runs>,
    access: State<'_, Access>,
) -> Result<CalibrationOutcome, String> {
    let dir = access.writable(&dir)?;
    let cancel = runs.start(&run_id);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let report = move |p: Progress| {
            let _ = on_progress.send(p);
        };
        match etendue_cli::calibrate::run_dir(&dir, &options, &Control::new(&report, Some(&cancel))) {
            Ok(run) => CalibrationOutcome::Ok { run },
            Err(e) if e.downcast_ref::<etendue_cli::progress::Cancelled>().is_some() => {
                CalibrationOutcome::Cancelled
            }
            Err(e) => CalibrationOutcome::Failed {
                message: format!("{e:#}"),
            },
        }
    })
    .await;
    runs.finish(&run_id);
    result.map_err(|e| format!("calibration run panicked: {e}"))
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
        .manage(Access::default())
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
                let access = app.state::<Access>();
                access.allow_read(app.handle(), &root.root);
                // The datasets folder may not exist yet: create it, so it resolves.
                let _ = std::fs::create_dir_all(&root.datasets);
                access.allow_write(app.handle(), &root.datasets);
            }
            app.manage(root);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            pick,
            save_dialog,
            write_text,
            open_project,
            save_scene,
            read_text,
            path_exists,
            list_dir,
            data_root,
            blender_status,
            scenario_from_poses,
            generate_dataset,
            calibrate_dataset,
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
    fn bundled_resources_are_a_data_root() {
        let dir = tempfile::tempdir().unwrap();
        assert!(DataRoot::bundled(dir.path().to_owned(), None).is_none());
        std::fs::create_dir(dir.path().join("examples")).unwrap();
        let docs = dir.path().join("Documents");
        let found = DataRoot::bundled(dir.path().to_owned(), Some(docs.clone())).unwrap();
        assert_eq!(found.root, dunce::canonicalize(dir.path()).unwrap());
        assert_eq!(found.datasets, docs.join("etendue"));
        let fallback = DataRoot::bundled(dir.path().to_owned(), None).unwrap();
        assert_eq!(fallback.datasets, std::env::temp_dir().join("etendue"));
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
