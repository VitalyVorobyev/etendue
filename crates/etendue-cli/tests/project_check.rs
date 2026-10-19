//! `etendue project check` (ADR 0009): the reference project passes, and each kind of problem
//! is caught and named.

use std::path::{Path, PathBuf};

use etendue_cli::project::check;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn the_examples_are_a_valid_project() {
    let s = check(&root().join("examples")).unwrap();
    assert_eq!(s.cells, 6);
    assert_eq!(s.library["cameras"], 2);
    assert_eq!(s.library["sensors"], 3);
}

/// A copy of the examples' manifest and library in `target/project_check/<name>`, with `edit`
/// applied, checked.
fn broken(name: &str, edit: impl FnOnce(&Path)) -> String {
    let dir = root().join("target/project_check").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    let copy = |from: &Path, to: &Path| {
        for entry in walk(from) {
            let rel = entry.strip_prefix(from).unwrap();
            std::fs::create_dir_all(to.join(rel).parent().unwrap()).unwrap();
            std::fs::copy(&entry, to.join(rel)).unwrap();
        }
    };
    copy(&root().join("examples/library"), &dir.join("library"));
    // No cells: the library and the manifest are under test here.
    let manifest = std::fs::read_to_string(root().join("examples/etendue-project.json")).unwrap();
    let mut doc: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    doc["cells"] = serde_json::json!([]);
    std::fs::write(dir.join("etendue-project.json"), doc.to_string()).unwrap();
    edit(&dir);
    format!("{:#}", check(&dir).unwrap_err())
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}

fn edit_json(path: &Path, f: impl FnOnce(&mut serde_json::Value)) {
    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    f(&mut v);
    std::fs::write(path, v.to_string()).unwrap();
}

#[test]
fn a_stale_camera_design_is_caught() {
    let err = broken("stale", |dir| {
        edit_json(&dir.join("library/cameras/imx174_lm25hc.json"), |v| {
            v["camera"]["optics"]["focus_distance_m"] = serde_json::json!(0.8);
        });
    });
    assert!(
        err.contains("library/cameras/imx174_lm25hc.json: camera.params.intrinsics.fx"),
        "{err}"
    );
}

#[test]
fn a_newer_item_is_refused_for_its_version() {
    let err = broken("version", |dir| {
        edit_json(&dir.join("library/lenses/lm25hc.json"), |v| {
            v["version"] = serde_json::json!(2);
            v["lens"]["future_field"] = serde_json::json!(true);
        });
    });
    assert!(
        err.contains("library/lenses/lm25hc.json: version: library item version 2"),
        "{err}"
    );
    assert!(!err.contains("future_field"), "{err}");
}

#[test]
fn ids_kinds_and_defaults_are_checked() {
    let err = broken("names", |dir| {
        std::fs::rename(
            dir.join("library/sensors/imx174.json"),
            dir.join("library/sensors/imx174_copy.json"),
        )
        .unwrap();
        std::fs::rename(
            dir.join("library/lasers/zm18_532.json"),
            dir.join("library/sensors/zm18_532.json"),
        )
        .unwrap();
        std::fs::remove_file(dir.join("library/targets/chess_9x6_20mm.json")).unwrap();
    });
    assert!(
        err.contains("library/sensors/imx174_copy.json: its id `imx174` is not its file name"),
        "{err}"
    );
    assert!(
        err.contains("library/sensors/zm18_532.json: a laser in the sensors folder"),
        "{err}"
    );
    assert!(
        err.contains("defaults.target: no library/targets/chess_9x6_20mm.json"),
        "{err}"
    );
}
