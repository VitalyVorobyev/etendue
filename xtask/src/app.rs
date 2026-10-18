//! `cargo xtask app [--release] [tauri args…]`: the studio desktop app from
//! the checkout (aliases `cargo app` and `cargo app-release`).
//!
//! Prepares what the app needs, then hands over to the Tauri CLI:
//! 1. the robot meshes (`tools/robot-assets/build.py`, needs uv), only when a
//!    robot in `assets/robots/` has none;
//! 2. `@etendue/wasm` (`crates/etendue-wasm/scripts/build-npm.mjs`, needs node
//!    and wasm-pack), every time, so the kernel matches the checkout;
//! 3. `bun install --frozen-lockfile` in `web/`;
//! 4. without `--release`, `tauri dev`: a debug build that runs at once and
//!    reloads the frontend on edits; with `--release`, `tauri build`: the
//!    installers in `web/apps/studio/src-tauri/target/release/bundle/`
//!    (`--bundles app` for the macOS app alone).
//!
//! Every other argument goes to the Tauri CLI unchanged.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

pub fn run(root: &Path, args: &[String]) -> Result<()> {
    let release = args.iter().any(|a| a == "--release");
    let rest: Vec<&str> = args
        .iter()
        .map(String::as_str)
        .filter(|a| *a != "--release")
        .collect();

    if robots_without_meshes(root)? {
        run_tool(
            Command::new("uv")
                .current_dir(root)
                .args(["run", "--locked", "--project", "tools/robot-assets"])
                .arg("tools/robot-assets/build.py"),
            "the robot meshes (uv, https://docs.astral.sh/uv/)",
        )?;
    }
    run_tool(
        Command::new("node")
            .current_dir(root)
            .arg("crates/etendue-wasm/scripts/build-npm.mjs"),
        "@etendue/wasm (node and wasm-pack)",
    )?;
    run_tool(
        Command::new("bun")
            .current_dir(root.join("web"))
            .args(["install", "--frozen-lockfile"]),
        "the web packages (bun, https://bun.sh)",
    )?;
    let studio = root.join("web/apps/studio");
    run_tool(
        Command::new("bun")
            .current_dir(&studio)
            .args(["run", "tauri", if release { "build" } else { "dev" }])
            .args(&rest),
        "the desktop app (tauri)",
    )?;
    if release {
        let profile = if rest.contains(&"--debug") {
            "debug"
        } else {
            "release"
        };
        println!(
            "bundles: {}",
            studio
                .join(format!("src-tauri/target/{profile}/bundle"))
                .display()
        );
    }
    Ok(())
}

/// Whether a committed robot lacks its (git-ignored) meshes.
fn robots_without_meshes(root: &Path) -> Result<bool> {
    let robots = root.join("assets/robots");
    for entry in std::fs::read_dir(&robots).with_context(|| robots.display().to_string())? {
        let dir = entry?.path();
        if !dir.join("robot.json").is_file() {
            continue;
        }
        let meshes = dir.join("meshes");
        let empty = std::fs::read_dir(&meshes).map_or(true, |mut d| d.next().is_none());
        if empty {
            return Ok(true);
        }
    }
    Ok(false)
}

fn run_tool(command: &mut Command, what: &str) -> Result<()> {
    let program = command.get_program().to_string_lossy().into_owned();
    let status = command
        .status()
        .with_context(|| format!("building {what}: could not run `{program}`; is it installed?"))?;
    if !status.success() {
        bail!("building {what}: `{program}` failed ({status})");
    }
    Ok(())
}
