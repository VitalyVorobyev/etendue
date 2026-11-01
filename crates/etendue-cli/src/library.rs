//! Component libraries (ADR 0009, ADR 0010): a project's `library/`, a user bank, the built-ins.
//! Checking one, listing several as layers, importing into one.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use etendue_core::from_scene::check_derived;
use etendue_kinematics::RobotModel;
use etendue_scene::{
    BANK_FILE, BankManifest, Issue, KINDS, LIBRARY_VERSION, LibraryItem, RobotManifest,
    ValidationError, Versioned,
};

use crate::load::read_json;

/// Parse a library file: its version first, so a newer file is refused as such.
///
/// # Errors
///
/// A [`ValidationError`] for a version this build does not read or JSON that does not parse.
pub fn read_item(text: &str) -> Result<LibraryItem, ValidationError> {
    let parse_error = |e: serde_json::Error| ValidationError {
        issues: vec![Issue {
            path: String::new(),
            message: e.to_string(),
        }],
    };
    let v: Versioned = serde_json::from_str(text).map_err(parse_error)?;
    LibraryItem::check_version(v.version)?;
    serde_json::from_str(text).map_err(parse_error)
}

/// What a checked library holds.
#[derive(Clone, Debug, Default)]
pub struct Contents {
    /// Every item that read, by reference (`cameras/imx174_lm25hc`).
    pub items: BTreeMap<String, LibraryItem>,
    /// The robots under `robots/<id>/`, by id.
    pub robots: BTreeMap<String, RobotManifest>,
}

impl Contents {
    /// The number of items per folder.
    #[must_use]
    pub fn counts(&self) -> BTreeMap<&'static str, usize> {
        let mut counts = BTreeMap::new();
        for item in self.items.values() {
            if let Some(folder) = item.folder() {
                *counts.entry(folder).or_default() += 1;
            }
        }
        counts
    }
}

/// The problems of one item read from `name` (`library/<folder>/<file>`) in `folder`.
fn item_problems(name: &str, file_stem: &str, folder: &str, item: &LibraryItem) -> Vec<String> {
    let mut problems = Vec::new();
    let mut issues = item.validate().err().map(|e| e.issues).unwrap_or_default();
    if file_stem != item.id {
        problems.push(format!("{name}: its id `{}` is not its file name", item.id));
    }
    if let Some(f) = item.folder()
        && f != folder
    {
        problems.push(format!(
            "{name}: a {} in the {folder} folder",
            item.kind().unwrap_or_default()
        ));
    }
    if let Some(c) = &item.camera
        && let Some(o) = &c.optics
    {
        issues.extend(check_derived(o, &c.params, "camera"));
    }
    if let Some(r) = &item.rig
        && let Some(o) = &r.layout.camera.optics
    {
        issues.extend(check_derived(
            o,
            &r.layout.camera.params,
            "rig.layout.camera",
        ));
    }
    problems.extend(
        issues
            .iter()
            .map(|i| format!("{name}: {}: {}", i.path, i.message)),
    );
    problems
}

/// Load a robot folder (`robot.json` and the URDF it names) as the kinematics would.
///
/// # Errors
///
/// When the manifest or URDF does not read, or the model does not build.
pub fn load_robot(dir: &Path) -> Result<RobotManifest> {
    let manifest_path = dir.join("robot.json");
    let manifest: RobotManifest = read_json(&manifest_path, "robot manifest")?;
    manifest
        .validate()
        .map_err(|e| anyhow::anyhow!("{}: {e}", manifest_path.display()))?;
    let urdf_path = dir.join(&manifest.urdf);
    let urdf = std::fs::read_to_string(&urdf_path)
        .with_context(|| format!("reading URDF {}", urdf_path.display()))?;
    RobotModel::from_urdf_str(&urdf, &manifest)
        .with_context(|| format!("robot {}", manifest_path.display()))?;
    Ok(manifest)
}

fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut paths: Vec<_> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    paths
}

/// Read and check every item under `dir/library/` and every robot under `dir/robots/` (a project
/// or a bank). Returns what read, and every problem found.
#[must_use]
pub fn scan(dir: &Path) -> (Contents, Vec<String>) {
    let mut contents = Contents::default();
    let mut problems = Vec::new();
    for (_, folder) in KINDS {
        for path in sorted_entries(&dir.join("library").join(folder))
            .into_iter()
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
        {
            let file = path.file_name().unwrap_or_default().to_string_lossy();
            let name = format!("library/{folder}/{file}");
            let text = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(e) => {
                    problems.push(format!("{name}: {e}"));
                    continue;
                }
            };
            match read_item(&text) {
                Ok(item) => {
                    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
                    problems.extend(item_problems(&name, &stem, folder, &item));
                    contents.items.insert(format!("{folder}/{stem}"), item);
                }
                Err(e) => problems.extend(
                    e.issues
                        .iter()
                        .map(|i| format!("{name}: {}: {}", i.path, i.message)),
                ),
            }
        }
    }
    for path in sorted_entries(&dir.join("robots"))
        .into_iter()
        .filter(|p| p.join("robot.json").is_file())
    {
        let id = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        match load_robot(&path) {
            Ok(m) => {
                contents.robots.insert(id, m);
            }
            Err(e) => problems.push(format!("robots/{id}: {e:#}")),
        }
    }
    (contents, problems)
}

/// Check a bank (or any folder shaped like one): its manifest, if it has one, and everything
/// [`scan`] reads.
///
/// # Errors
///
/// Every problem found, in one error.
pub fn check(dir: &Path) -> Result<Contents> {
    let mut problems = Vec::new();
    let manifest = dir.join(BANK_FILE);
    if manifest.is_file() {
        match read_json::<BankManifest>(&manifest, "bank manifest") {
            Ok(m) => {
                if let Err(e) = m.validate() {
                    problems.push(format!("{BANK_FILE}: {e}"));
                }
            }
            Err(e) => problems.push(format!("{e:#}")),
        }
    }
    let (contents, found) = scan(dir);
    problems.extend(found);
    if !problems.is_empty() {
        bail!(
            "library {}: {} problem(s)\n  {}",
            dir.display(),
            problems.len(),
            problems.join("\n  ")
        );
    }
    Ok(contents)
}

/// One layer of a listing.
#[derive(Clone, Debug)]
pub struct LayerDir {
    /// `project`, `bank` or `builtin`.
    pub name: &'static str,
    /// Its folder.
    pub dir: PathBuf,
}

/// An item or robot as the layers resolve it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    /// `<folder>/<id>`, or `robots/<id>`.
    pub reference: String,
    /// For people.
    pub name: String,
    /// The layer it resolves to.
    pub layer: &'static str,
    /// The lower layers that have the same reference (hidden by this one).
    pub shadows: Vec<&'static str>,
}

/// Every item and robot of `layers` (highest first), each once, from the layer it resolves to.
/// A layer that does not read is skipped with its problems; an item that does not check is
/// still listed.
#[must_use]
pub fn list(layers: &[LayerDir]) -> (Vec<Listed>, Vec<String>) {
    let mut out: BTreeMap<String, Listed> = BTreeMap::new();
    let mut problems = Vec::new();
    for layer in layers {
        let (contents, found) = scan(&layer.dir);
        problems.extend(found.into_iter().map(|p| format!("{}: {p}", layer.name)));
        let names = contents
            .items
            .iter()
            .map(|(r, i)| (r.clone(), i.name.clone()))
            .chain(
                contents
                    .robots
                    .iter()
                    .map(|(id, m)| (format!("robots/{id}"), m.name.clone())),
            );
        for (reference, name) in names {
            match out.get_mut(&reference) {
                Some(l) => l.shadows.push(layer.name),
                None => {
                    out.insert(
                        reference.clone(),
                        Listed {
                            reference,
                            name,
                            layer: layer.name,
                            shadows: vec![],
                        },
                    );
                }
            }
        }
    }
    (out.into_values().collect(), problems)
}

/// Make `dir` a bank named `name`: write its manifest (the folder may exist, but not a manifest).
///
/// # Errors
///
/// When it already has a manifest or the write fails.
pub fn init_bank(dir: &Path, name: &str) -> Result<()> {
    let path = dir.join(BANK_FILE);
    ensure!(!path.exists(), "{} already exists", path.display());
    let manifest = BankManifest::new(name);
    manifest
        .validate()
        .map_err(|e| anyhow::anyhow!("bank manifest: {e}"))?;
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let mut text = serde_json::to_string_pretty(&manifest)?;
    text.push('\n');
    std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))
}

/// What [`import`] wrote, relative to the destination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Imported {
    /// `library/<folder>/<id>.json`, or `robots/<id>`.
    pub path: String,
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to).with_context(|| format!("creating {}", to.display()))?;
    for entry in std::fs::read_dir(from).with_context(|| format!("reading {}", from.display()))? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)
                .with_context(|| format!("copying {}", entry.path().display()))?;
        }
    }
    Ok(())
}

/// Import `source` into the library at `into`: a library item file (checked, then written at
/// version [`LIBRARY_VERSION`] to `library/<folder>/<id>.json`), or a robot folder (`robot.json`
/// and its URDF, loaded as the kinematics would, then copied to `robots/<id>/` with its meshes).
/// An existing item or robot is replaced only with `force`.
///
/// # Errors
///
/// When the source does not check, the destination exists without `force`, or a write fails.
pub fn import(source: &Path, into: &Path, force: bool) -> Result<Imported> {
    if source.is_dir() {
        load_robot(source)?;
        let id = source
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .filter(|id| etendue_scene::is_valid_id(id))
            .with_context(|| format!("{}: a robot folder is named by its id", source.display()))?;
        let to = into.join("robots").join(&id);
        ensure!(
            force || !to.exists(),
            "{} exists (use --force to replace it)",
            to.display()
        );
        if to.exists() {
            std::fs::remove_dir_all(&to).with_context(|| format!("replacing {}", to.display()))?;
        }
        copy_dir(source, &to)?;
        return Ok(Imported {
            path: format!("robots/{id}"),
        });
    }
    let text =
        std::fs::read_to_string(source).with_context(|| format!("reading {}", source.display()))?;
    let mut item = read_item(&text).map_err(|e| anyhow::anyhow!("{}: {e}", source.display()))?;
    item.version = LIBRARY_VERSION;
    let folder = item
        .folder()
        .with_context(|| format!("{}: holds no single part", source.display()))?;
    let problems = item_problems(&source.display().to_string(), &item.id, folder, &item);
    ensure!(problems.is_empty(), "{}", problems.join("\n  "));
    let path = format!("library/{folder}/{}.json", item.id);
    let to = into.join(&path);
    ensure!(
        force || !to.exists(),
        "{} exists (use --force to replace it)",
        to.display()
    );
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let mut out = serde_json::to_string_pretty(&item)?;
    out.push('\n');
    std::fs::write(&to, out).with_context(|| format!("writing {}", to.display()))?;
    Ok(Imported { path })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("etendue-library-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn repo() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn layers_resolve_highest_first_and_mark_what_they_hide() {
        let bank = temp("bank");
        init_bank(&bank, "test bank").unwrap();
        assert!(init_bank(&bank, "again").is_err());
        let examples = repo().join("examples");
        let lens = examples.join("library/lenses");
        let first = sorted_entries(&lens).into_iter().next().unwrap();
        import(&first, &bank, false).unwrap();
        assert!(import(&first, &bank, false).is_err(), "no silent replace");
        import(&first, &bank, true).unwrap();
        check(&bank).unwrap();
        let (listed, problems) = list(&[
            LayerDir {
                name: "bank",
                dir: bank.clone(),
            },
            LayerDir {
                name: "builtin",
                dir: examples,
            },
        ]);
        assert!(problems.is_empty(), "{problems:?}");
        let stem = first.file_stem().unwrap().to_string_lossy();
        let hit = listed
            .iter()
            .find(|l| l.reference == format!("lenses/{stem}"))
            .unwrap();
        assert_eq!(
            (hit.layer, hit.shadows.as_slice()),
            ("bank", &["builtin"][..])
        );
        assert!(
            listed
                .iter()
                .any(|l| l.layer == "builtin" && l.shadows.is_empty())
        );
        // The import is written at the current version.
        let text =
            std::fs::read_to_string(bank.join(format!("library/lenses/{stem}.json"))).unwrap();
        assert_eq!(read_item(&text).unwrap().version, LIBRARY_VERSION);
        let _ = std::fs::remove_dir_all(&bank);
    }

    #[test]
    fn a_robot_folder_imports_when_its_model_loads() {
        let bank = temp("robots");
        let ur5e = repo().join("assets/robots/ur5e");
        let imported = import(&ur5e, &bank, false).unwrap();
        assert_eq!(imported.path, "robots/ur5e");
        let contents = check(&bank).unwrap();
        assert!(contents.robots.contains_key("ur5e"));
        // A folder whose URDF is missing is refused.
        let broken = temp("broken");
        std::fs::create_dir_all(broken.join("bad")).unwrap();
        std::fs::copy(ur5e.join("robot.json"), broken.join("bad/robot.json")).unwrap();
        assert!(import(&broken.join("bad"), &bank, false).is_err());
        let _ = std::fs::remove_dir_all(&bank);
        let _ = std::fs::remove_dir_all(&broken);
    }
}
