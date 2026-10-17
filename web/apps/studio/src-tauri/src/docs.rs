//! Writing scene documents somewhere else than where they were read.
//!
//! A scene names its robot manifests by paths relative to the scene file.
//! Saved to another folder, those paths must be rewritten, or the scene no
//! longer finds its robots. [`rebase_manifests`] does that: it resolves each
//! manifest against the folder the scene came from (or the robot library) and
//! rewrites it relative to the new folder.

use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use serde_json::Value;

/// `target` relative to the directory `base`, with `/` separators; `None`
/// when no relative path exists (different Windows drives). Both must be
/// absolute and free of `.`/`..`.
pub fn relative(target: &Path, base: &Path) -> Option<String> {
    let t: Vec<Component> = target.components().collect();
    let b: Vec<Component> = base.components().collect();
    if t.first() != b.first() {
        return None;
    }
    let common = t.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let mut parts: Vec<String> = vec!["..".to_owned(); b.len() - common];
    parts.extend(
        t[common..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    );
    Some(if parts.is_empty() {
        ".".to_owned()
    } else {
        parts.join("/")
    })
}

/// Where a scene robot's manifest is: `manifest` relative to `from` (the
/// folder the scene came from) if that file exists, else the robot library's
/// copy (`<library>/assets/robots/<folder>/<file>`, or the local-only
/// `assets/robots-local/`, by the manifest path's last two components).
fn find_manifest(manifest: &str, from: Option<&Path>, library: Option<&Path>) -> Option<PathBuf> {
    if let Some(found) = from.map(|d| d.join(manifest)).filter(|p| p.is_file()) {
        return dunce::canonicalize(found).ok();
    }
    let path = Path::new(manifest);
    let file = path.file_name()?;
    let folder = path.parent()?.file_name()?;
    let library = library?;
    ["assets/robots", "assets/robots-local"]
        .iter()
        .map(|dir| library.join(dir).join(folder).join(file))
        .find(|candidate| candidate.is_file())
        .and_then(|candidate| dunce::canonicalize(candidate).ok())
}

/// Rewrite `scene.robots[].manifest` (a scene document as JSON) for a scene
/// file saved in the directory `to`. Manifests are found relative to `from`
/// (where the scene was read, if anywhere), then in `library`.
///
/// # Errors
///
/// If a manifest is found nowhere, or `to` shares no root with it.
pub fn rebase_manifests(
    scene: &mut Value,
    from: Option<&Path>,
    to: &Path,
    library: Option<&Path>,
) -> Result<()> {
    let Some(robots) = scene.get_mut("robots").and_then(Value::as_array_mut) else {
        return Ok(());
    };
    let to = dunce::canonicalize(to).with_context(|| format!("{}", to.display()))?;
    for robot in robots {
        let id = robot
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_owned();
        let Some(manifest) = robot.get("manifest").and_then(Value::as_str) else {
            continue;
        };
        let found = find_manifest(manifest, from, library)
            .ok_or_else(|| anyhow!("robot `{id}`: manifest {manifest} not found"))?;
        let rebased = relative(&found, &to).ok_or_else(|| {
            anyhow!(
                "robot `{id}`: {} has no path relative to {}",
                found.display(),
                to.display()
            )
        })?;
        robot["manifest"] = Value::String(rebased);
    }
    Ok(())
}

/// Write `doc` as pretty JSON (with a final newline) to `path`.
///
/// # Errors
///
/// If the file cannot be written.
pub fn write_json(path: &Path, doc: &Value) -> Result<()> {
    std::fs::write(path, serde_json::to_string_pretty(doc)? + "\n")
        .with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> PathBuf {
        dunce::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")).unwrap()
    }

    #[test]
    fn relative_paths_climb_and_descend() {
        let r = |a: &str, b: &str| relative(Path::new(a), Path::new(b));
        assert_eq!(r("/a/b/c.json", "/a/d").as_deref(), Some("../b/c.json"));
        assert_eq!(r("/a/b/c.json", "/a/b").as_deref(), Some("c.json"));
        assert_eq!(r("/a", "/a").as_deref(), Some("."));
        assert_eq!(r("/x/y", "/a/b/c").as_deref(), Some("../../../x/y"));
    }

    #[test]
    fn manifests_follow_the_scene_to_its_new_folder() {
        let root = repo();
        let example = root.join("examples/eye_in_hand_ur5e");
        let mut scene: Value =
            serde_json::from_str(&std::fs::read_to_string(example.join("scene.json")).unwrap())
                .unwrap();
        let out = tempfile::tempdir_in(root.join("target")).unwrap();
        rebase_manifests(&mut scene, Some(&example), out.path(), Some(&root)).unwrap();
        let manifest = scene["robots"][0]["manifest"].as_str().unwrap();
        let manifest = out.path().join(manifest);
        assert!(
            manifest.parent().unwrap().join("robot.urdf").is_file(),
            "{}",
            manifest.display()
        );

        // A scene from nowhere (dropped files) finds its robots in the library.
        let mut dropped: Value = serde_json::json!({
            "robots": [{"id": "ur5e", "manifest": "whatever/ur5e/robot.json"}]
        });
        rebase_manifests(&mut dropped, None, out.path(), Some(&root)).unwrap();
        let m = dropped["robots"][0]["manifest"].as_str().unwrap();
        assert!(out.path().join(m).is_file(), "{m}");

        let mut missing: Value =
            serde_json::json!({"robots": [{"id": "x", "manifest": "nope/robot.json"}]});
        assert!(rebase_manifests(&mut missing, None, out.path(), Some(&root)).is_err());
    }
}
