//! Which manifest a dataset argument names: a path, an id of `local/datasets.toml`, or a public
//! id of calibration-rs's data.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// The git-ignored file naming the datasets on this machine, under the root etendue runs from:
/// `id = "path/to/dataset.toml"` per line, a relative path read from the file's folder.
pub const LOCAL_DATASETS: &str = "local/datasets.toml";

/// The public datasets found in a sibling calibration-rs checkout,
/// `../calibration-rs/data/<id>/dataset.toml`, when `local/datasets.toml` does not name them.
pub const PUBLIC: &[&str] = &["kuka_1", "DS8"];

/// A real dataset: its manifest (calibration-rs's `dataset.toml`), and the id that named it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DatasetSource {
    /// The id it was named by, if any (a private dataset is known by it alone in anything
    /// committed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The manifest's path.
    pub manifest: String,
}

impl DatasetSource {
    /// The manifest's path.
    #[must_use]
    pub fn manifest(&self) -> &Path {
        Path::new(&self.manifest)
    }

    /// The folder the manifest's relative paths are read from.
    #[must_use]
    pub fn base_dir(&self) -> PathBuf {
        self.manifest()
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default()
    }

    /// The id, or the manifest's path.
    #[must_use]
    pub fn name(&self) -> String {
        self.id.clone().unwrap_or_else(|| self.manifest.clone())
    }
}

/// The dataset `arg` names, from `root` (the folder etendue runs from):
/// - an existing manifest file, or a folder holding `dataset.toml`;
/// - else an id: of [`LOCAL_DATASETS`] under `root`, or one of [`PUBLIC`] in the sibling
///   calibration-rs checkout.
///
/// # Errors
///
/// When `arg` names nothing, `local/datasets.toml` does not read, or the manifest it names does
/// not exist.
pub fn resolve(arg: &str, root: &Path) -> Result<DatasetSource> {
    let path = Path::new(arg);
    if path.is_file() {
        return Ok(source(None, path.to_path_buf()));
    }
    if path.is_dir() {
        let manifest = path.join("dataset.toml");
        if manifest.is_file() {
            return Ok(source(None, manifest));
        }
        bail!("{} holds no dataset.toml", path.display());
    }
    let local = root.join(LOCAL_DATASETS);
    if let Some(manifest) = local_entry(&local, arg)? {
        if !manifest.is_file() {
            bail!(
                "{} names `{arg}` as {}, which does not exist",
                local.display(),
                manifest.display()
            );
        }
        return Ok(source(Some(arg), manifest));
    }
    if PUBLIC.contains(&arg) {
        let manifest = root
            .join("../calibration-rs/data")
            .join(arg)
            .join("dataset.toml");
        if !manifest.is_file() {
            bail!(
                "the public dataset `{arg}` is not at {} (clone calibration-rs beside etendue, \
                 or name it in {LOCAL_DATASETS})",
                manifest.display()
            );
        }
        return Ok(source(Some(arg), manifest));
    }
    bail!(
        "`{arg}` is not a manifest, a folder with dataset.toml, an id of {} or a public dataset \
         ({})",
        local.display(),
        PUBLIC.join(", ")
    )
}

fn source(id: Option<&str>, manifest: PathBuf) -> DatasetSource {
    DatasetSource {
        id: id.map(str::to_owned),
        manifest: manifest.to_string_lossy().into_owned(),
    }
}

/// The manifest `local` gives `id`, read from `local`'s folder; `None` without the file or the id.
fn local_entry(local: &Path, id: &str) -> Result<Option<PathBuf>> {
    if !local.is_file() {
        return Ok(None);
    }
    let text =
        std::fs::read_to_string(local).with_context(|| format!("reading {}", local.display()))?;
    let ids: BTreeMap<String, String> =
        toml::from_str(&text).with_context(|| format!("reading {}", local.display()))?;
    Ok(ids.get(id).map(|p| {
        local
            .parent()
            .map_or_else(|| PathBuf::from(p), |dir| dir.join(p))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("etendue-real-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_path_an_id_and_a_public_id() {
        let root = scratch("resolve").join("etendue");
        std::fs::create_dir_all(root.join("local")).unwrap();
        let data = root.join("../data/mine");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("dataset.toml"), "").unwrap();
        // A manifest by path, and its folder.
        let manifest = data.join("dataset.toml");
        let by_path = resolve(manifest.to_str().unwrap(), &root).unwrap();
        assert_eq!(by_path.id, None);
        assert_eq!(by_path.manifest(), manifest);
        assert_eq!(by_path.base_dir(), data);
        let by_dir = resolve(data.to_str().unwrap(), &root).unwrap();
        assert_eq!(by_dir.manifest(), manifest);
        // An id, read from local/.
        std::fs::write(
            root.join(LOCAL_DATASETS),
            "mine = \"../../data/mine/dataset.toml\"\n",
        )
        .unwrap();
        let by_id = resolve("mine", &root).unwrap();
        assert_eq!(by_id.id.as_deref(), Some("mine"));
        assert_eq!(
            by_id.manifest(),
            root.join("local/../../data/mine/dataset.toml")
        );
        assert_eq!(by_id.name(), "mine");
        // A public id, from the sibling checkout.
        let kuka = root.join("../calibration-rs/data/kuka_1");
        std::fs::create_dir_all(&kuka).unwrap();
        std::fs::write(kuka.join("dataset.toml"), "").unwrap();
        assert_eq!(
            resolve("kuka_1", &root).unwrap().manifest(),
            kuka.join("dataset.toml")
        );
        // DS8 is public but not there; an unknown id is nothing.
        let e = resolve("DS8", &root).unwrap_err().to_string();
        assert!(e.contains("public dataset `DS8`"), "{e}");
        let e = resolve("theirs", &root).unwrap_err().to_string();
        assert!(e.contains("is not a manifest"), "{e}");
    }

    #[test]
    fn a_local_id_whose_manifest_is_missing_is_an_error() {
        let root = scratch("missing");
        std::fs::create_dir_all(root.join("local")).unwrap();
        std::fs::write(
            root.join(LOCAL_DATASETS),
            "gone = \"/no/such/dataset.toml\"\n",
        )
        .unwrap();
        let e = resolve("gone", &root).unwrap_err().to_string();
        assert!(e.contains("does not exist"), "{e}");
    }
}
