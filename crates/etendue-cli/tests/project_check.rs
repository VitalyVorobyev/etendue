//! `etendue project check` (ADR 0009): the reference project passes, and each kind of problem
//! is caught and named.

use std::path::{Path, PathBuf};

use etendue_cli::project::check;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn the_examples_are_a_valid_project() {
    let s = check(&root().join("examples"), None).unwrap();
    assert_eq!(s.cells, 6);
    assert_eq!(s.library["cameras"], 3);
    assert_eq!(s.library["sensors"], 3);
    assert_eq!(s.library["mounts"], 2);
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
    format!("{:#}", check(&dir, None).unwrap_err())
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
            v["version"] = serde_json::json!(3);
            v["lens"]["future_field"] = serde_json::json!(true);
        });
    });
    assert!(
        err.contains("library/lenses/lm25hc.json: version: library item version 3"),
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

/// Gate G14.1, the CLI's side: `etendue library check` passes on every built-in item. (The
/// studio's side, the byte-for-byte round trip, is `web/apps/studio/src/io/g14.test.ts`.)
#[test]
fn g14_1_every_built_in_item_passes_library_check() {
    let contents = etendue_cli::library::check(&root().join("examples")).unwrap();
    let counts = contents.counts();
    assert!(counts.values().sum::<usize>() > 20, "{counts:?}");
}

/// Gate G14.2, the CLI's side: a project whose items are copies of bank items (as the studio's
/// `copyOf` writes them: the item, version 2, and its origin with the source's content hash)
/// passes `project check` once the bank is gone, and `--bank` notes what the bank no longer has.
#[test]
fn g14_2_a_project_of_bank_copies_passes_without_the_bank() {
    use etendue_cli::library::{import, init_bank, read_item};

    let dir = root().join("target/project_check/g14_2");
    let _ = std::fs::remove_dir_all(&dir);
    let (bank, project) = (dir.join("bank"), dir.join("project"));
    init_bank(&bank, "G14.2 bank").unwrap();
    let refs = [
        "cameras/imx174_lm25hc",
        "lasers/stingray_660",
        "targets/chess_9x6_20mm",
    ];
    for r in refs {
        import(
            &root().join(format!("examples/library/{r}.json")),
            &bank,
            false,
        )
        .unwrap();
    }
    for r in refs {
        let text = std::fs::read_to_string(bank.join(format!("library/{r}.json"))).unwrap();
        let source = read_item(&text).unwrap();
        let mut copy: serde_json::Value = serde_json::from_str(&text).unwrap();
        copy["version"] = serde_json::json!(2);
        copy["origin"] = serde_json::json!({
            "layer": "bank",
            "reference": r,
            "sha256": etendue_core::library::content_hash(&source),
        });
        let path = project.join(format!("library/{r}.json"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, serde_json::to_string_pretty(&copy).unwrap()).unwrap();
    }
    let manifest = serde_json::json!({
        "version": 1, "name": "G14.2", "cells": [],
        "defaults": {"camera": refs[0], "laser": refs[1], "target": refs[2]},
    });
    std::fs::write(project.join("etendue-project.json"), manifest.to_string()).unwrap();

    // With the bank: nothing has changed.
    let s = check(&project, Some(&bank)).unwrap();
    assert!(s.notes.is_empty(), "{:?}", s.notes);

    // Without it: still a valid project, and the bank's absence is a note, not a problem.
    std::fs::remove_dir_all(&bank).unwrap();
    let s = check(&project, None).unwrap();
    assert_eq!(s.library.values().sum::<usize>(), 3);
    let s = check(&project, Some(&bank)).unwrap();
    assert_eq!(s.notes.len(), 3, "{:?}", s.notes);
    assert!(
        s.notes.iter().all(|n| n.contains("no longer has")),
        "{:?}",
        s.notes
    );
}
