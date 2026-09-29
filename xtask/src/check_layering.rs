//! Enforce the crate dependency rules of ADR 0001 §2 (`docs/adrs/0001-pivot.md`).
//!
//! Rules, checked on normal and build dependencies (dev-dependencies are
//! exempt):
//!
//! - Each workspace crate may depend only on the workspace crates listed for
//!   it in [`RULES`]. A workspace crate missing from [`RULES`] is an error —
//!   new crates must declare their layer.
//! - `etendue-scene` may depend only on the external crates in
//!   [`SCENE_EXTERNAL`].
//! - No crate may depend on a binary-only workspace crate, or on a crate in
//!   [`TOP`] (the top of the stack, even though it has a library).
//! - The resolved graph contains exactly one `nalgebra` (the 0.34 hard pin)
//!   and never the `k` crate (it requires nalgebra ^0.30).
//!
//! `--manifest-path <Cargo.toml>` checks only the last rule, on a crate
//! outside the workspace (the studio's Tauri shell), whose graph the
//! workspace's `cargo metadata` does not see.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde_json::Value;

/// Allowed workspace dependencies per workspace crate. `"*lib"` means any
/// library crate of the workspace.
const RULES: &[(&str, &[&str])] = &[
    ("etendue-scene", &[]),
    ("etendue-core", &["etendue-scene"]),
    ("etendue-kinematics", &["etendue-scene"]),
    ("etendue-synth", &["etendue-scene", "etendue-kinematics"]),
    ("etendue-wasm", &["*lib"]),
    ("etendue-cli", &["*lib"]),
    ("etendue-ui", &["etendue-core"]),
    ("xtask", &["*lib"]),
];

/// Crates with a library that nothing in the workspace may depend on: the
/// pipeline library behind the CLI is for the CLI and the studio shell only.
const TOP: &[&str] = &["etendue-cli"];

/// The only external crates `etendue-scene` may depend on.
const SCENE_EXTERNAL: &[&str] = &[
    "nalgebra",
    "serde",
    "thiserror",
    "schemars",
    "vision-calibration-core",
    "vision-calibration-dataset",
];

fn metadata(root: &Path, manifest: Option<&Path>) -> Result<Value> {
    let mut command = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    command
        .args(["metadata", "--format-version", "1"])
        .current_dir(root);
    if let Some(m) = manifest {
        command.arg("--manifest-path").arg(m);
    }
    let output = command.output().context("running cargo metadata")?;
    if !output.status.success() {
        bail!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

/// The single-nalgebra and no-`k` rules on `packages`.
fn check_graph<'a>(packages: &'a [Value], errors: &mut Vec<String>) -> Vec<&'a str> {
    let versions = |crate_name: &str| -> Vec<&str> {
        packages
            .iter()
            .filter(|p| p["name"] == crate_name)
            .filter_map(|p| p["version"].as_str())
            .collect()
    };
    let nalgebra = versions("nalgebra");
    if nalgebra.len() != 1 {
        errors.push(format!(
            "expected exactly one nalgebra in the graph (hard pin), found {nalgebra:?}"
        ));
    }
    if !versions("k").is_empty() {
        errors.push("the `k` crate is banned (requires nalgebra ^0.30)".into());
    }
    nalgebra
}

/// Check the graph of a crate outside the workspace.
pub fn run_manifest(root: &Path, manifest: &Path) -> Result<()> {
    let meta = metadata(root, Some(manifest))?;
    let packages = meta["packages"].as_array().context("packages")?;
    let mut errors = Vec::new();
    let nalgebra = check_graph(packages, &mut errors);
    if errors.is_empty() {
        println!("{}: nalgebra {}", manifest.display(), nalgebra[0]);
        Ok(())
    } else {
        bail!("{}:\n  {}", manifest.display(), errors.join("\n  "))
    }
}

pub fn run(root: &Path) -> Result<()> {
    let meta = metadata(root, None)?;
    let packages = meta["packages"].as_array().context("packages")?;
    let members: BTreeSet<&str> = meta["workspace_members"]
        .as_array()
        .context("workspace_members")?
        .iter()
        .filter_map(Value::as_str)
        .collect();

    let workspace: Vec<&Value> = packages
        .iter()
        .filter(|p| p["id"].as_str().is_some_and(|id| members.contains(id)))
        .collect();
    let is_lib: BTreeMap<&str, bool> = workspace
        .iter()
        .map(|p| {
            let has_lib = p["targets"].as_array().is_some_and(|ts| {
                ts.iter().any(|t| {
                    t["kind"].as_array().is_some_and(|k| {
                        k.iter()
                            .any(|k| k != "bin" && k != "example" && k != "test" && k != "bench")
                    })
                })
            });
            (p["name"].as_str().unwrap_or_default(), has_lib)
        })
        .collect();
    let rules: BTreeMap<&str, &[&str]> = RULES.iter().copied().collect();

    let mut errors = Vec::new();
    for p in &workspace {
        let name = p["name"].as_str().unwrap_or_default();
        let Some(allowed) = rules.get(name) else {
            errors.push(format!(
                "workspace crate `{name}` has no layering rule; add it to xtask/src/check_layering.rs"
            ));
            continue;
        };
        for dep in p["dependencies"].as_array().into_iter().flatten() {
            if dep["kind"].as_str() == Some("dev") {
                continue;
            }
            let dep_name = dep["name"].as_str().unwrap_or_default();
            if let Some(&dep_is_lib) = is_lib.get(dep_name) {
                if TOP.contains(&dep_name) {
                    errors.push(format!(
                        "`{name}` may not depend on `{dep_name}` (the top of the stack)"
                    ));
                } else if !dep_is_lib {
                    errors.push(format!("`{name}` depends on binary crate `{dep_name}`"));
                } else if !(allowed.contains(&dep_name) || allowed.contains(&"*lib")) {
                    errors.push(format!(
                        "`{name}` may not depend on `{dep_name}` (allowed: {allowed:?})"
                    ));
                }
            } else if name == "etendue-scene" && !SCENE_EXTERNAL.contains(&dep_name) {
                errors.push(format!(
                    "`etendue-scene` may not depend on `{dep_name}` (allowed: {SCENE_EXTERNAL:?})"
                ));
            }
        }
    }

    let nalgebra = check_graph(packages, &mut errors);

    if errors.is_empty() {
        println!(
            "layering ok ({} workspace crates, nalgebra {})",
            workspace.len(),
            nalgebra[0]
        );
        Ok(())
    } else {
        bail!("layering violations:\n  {}", errors.join("\n  "))
    }
}
