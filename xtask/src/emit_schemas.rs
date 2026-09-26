//! Emit the JSON Schemas of the `etendue-scene` document types.
//!
//! Output: `schemas/<name>.schema.json` (committed). The schemas are
//! schemars' default dialect (JSON Schema 2020-12), like calibration-rs's
//! `emit-schemas`. With `--check`, verify the committed files instead; CI
//! runs this to catch drift between source and schemas.

use std::path::Path;

use anyhow::{Context, Result, bail};
use etendue_scene::{BakedScenario, RobotManifest, ScenarioSpec, SceneSpec};
use schemars::{JsonSchema, schema_for};

fn render<T: JsonSchema>() -> Result<String> {
    let mut text = serde_json::to_string_pretty(&schema_for!(T))?;
    text.push('\n');
    Ok(text)
}

pub fn run(root: &Path, check: bool) -> Result<()> {
    let out_dir = root.join("schemas");
    let entries = [
        ("scene", render::<SceneSpec>()?),
        ("scenario", render::<ScenarioSpec>()?),
        ("baked_scenario", render::<BakedScenario>()?),
        ("robot_manifest", render::<RobotManifest>()?),
    ];

    if !check {
        std::fs::create_dir_all(&out_dir)
            .with_context(|| format!("creating {}", out_dir.display()))?;
    }
    let mut stale = Vec::new();
    for (name, text) in &entries {
        let path = out_dir.join(format!("{name}.schema.json"));
        if check {
            let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
            if on_disk.replace("\r\n", "\n") != *text {
                stale.push(path.display().to_string());
            }
        } else {
            std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
        }
    }
    if !stale.is_empty() {
        bail!(
            "stale or missing schemas (run `cargo xtask emit-schemas`):\n  {}",
            stale.join("\n  ")
        );
    }
    println!(
        "schemas {} ({} files)",
        if check { "up to date" } else { "written" },
        entries.len()
    );
    Ok(())
}
