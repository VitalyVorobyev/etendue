//! Workspace automation: `cargo xtask <command>`.
//!
//! - `emit-schemas [--check]`: write (or verify) the JSON Schemas of the
//!   `etendue-scene` document types into `schemas/`.
//! - `check-layering [--manifest-path <Cargo.toml>]`: enforce the crate
//!   dependency rules of ADR 0001 §2 and the single-nalgebra hard pin (with a
//!   manifest: the pin alone, for a crate outside the workspace).

mod check_layering;
mod emit_schemas;

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = workspace_root();
    match args.first().map(String::as_str) {
        Some("emit-schemas") => {
            let check = match args.get(1).map(String::as_str) {
                None => false,
                Some("--check") => true,
                Some(other) => bail!("unknown emit-schemas flag `{other}`"),
            };
            emit_schemas::run(&root, check)
        }
        Some("check-layering") => match (args.get(1).map(String::as_str), args.get(2)) {
            (None, _) => check_layering::run(&root),
            (Some("--manifest-path"), Some(path)) => {
                check_layering::run_manifest(&root, Path::new(path))
            }
            _ => bail!("usage: cargo xtask check-layering [--manifest-path <Cargo.toml>]"),
        },
        _ => bail!(
            "usage: cargo xtask <emit-schemas [--check] | check-layering [--manifest-path <Cargo.toml>]>"
        ),
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives one level below the workspace root")
        .to_path_buf()
}
