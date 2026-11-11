//! The JSON Schemas of the documents `etendue calibrate` writes and the studio reads: a run's
//! report (`calibration_run`) and a kept run as the studio lists it (`stored_run`). They sit with
//! the scene documents' schemas in `schemas/`, and the studio's types are generated from them
//! (`bun run generate:types`).
//!
//! With `ETENDUE_WRITE_SCHEMAS=1` this writes them; otherwise it checks the committed ones, so CI
//! catches a report type changed without its schema. (`cargo xtask emit-schemas` cannot emit
//! them: nothing may depend on `etendue-cli`'s library, ADR 0001.)

use std::path::PathBuf;

use etendue_cli::calibrate::{KeptRuns, Run};
use schemars::{JsonSchema, schema_for};

fn render<T: JsonSchema>() -> String {
    let mut text = serde_json::to_string_pretty(&schema_for!(T)).unwrap();
    text.push('\n');
    text
}

#[test]
fn the_report_schemas_are_current() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let write = std::env::var_os("ETENDUE_WRITE_SCHEMAS").is_some();
    for (name, text) in [
        ("calibration_run", render::<Run>()),
        ("kept_runs", render::<KeptRuns>()),
    ] {
        let path = dir.join(format!("{name}.schema.json"));
        if write {
            std::fs::write(&path, &text).unwrap();
        } else {
            // A Windows checkout may have CRLF line endings.
            let on_disk = std::fs::read_to_string(&path)
                .unwrap_or_default()
                .replace("\r\n", "\n");
            assert!(
                on_disk == text,
                "{} is stale: run `ETENDUE_WRITE_SCHEMAS=1 cargo test -p etendue-cli --test schemas`",
                path.display()
            );
        }
    }
}
