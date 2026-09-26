//! Gate G0.1, native side: recompute the Node probe natively and compare
//! bit-for-bit.
//!
//! ```bash
//! wasm-pack build crates/etendue-wasm --target nodejs --release
//! node crates/etendue-wasm/tests/node/g01_parity.mjs > target/g01.json
//! cargo run -p etendue-wasm --example g01_verify -- target/g01.json
//! ```
//!
//! The probe file holds the world points the Node script fed to the wasm
//! build and the pixels it got back, both as IEEE-754 bit patterns (16 hex
//! digits). This binary decodes the inputs, runs the same
//! [`etendue_wasm::project_points_world`] natively on `Scene::default_mvp()`,
//! and exits non-zero on any differing bit.

use std::process::ExitCode;

use etendue_core::Scene;
use serde_json::Value;

fn decode(v: &Value, key: &str) -> Vec<f64> {
    v[key]
        .as_array()
        .unwrap_or_else(|| panic!("probe file has no `{key}` array"))
        .iter()
        .map(|h| {
            let h = h.as_str().expect("bit patterns are hex strings");
            f64::from_bits(u64::from_str_radix(h, 16).expect("valid 64-bit hex"))
        })
        .collect()
}

fn main() -> ExitCode {
    let path = std::env::args()
        .nth(1)
        .expect("usage: g01_verify <probe.json>");
    let text = std::fs::read_to_string(&path).expect("read probe file");
    let probe: Value = serde_json::from_str(&text).expect("probe file is JSON");
    let camera_index = probe["camera_index"].as_u64().expect("camera_index") as usize;
    let xyz = decode(&probe, "xyz");
    let wasm_uv = decode(&probe, "uv");

    let native_uv = etendue_wasm::project_points_world(&Scene::default_mvp(), camera_index, &xyz)
        .expect("native projection");
    assert_eq!(
        native_uv.len(),
        wasm_uv.len(),
        "native and wasm output lengths differ"
    );

    let n_points = xyz.len() / 3;
    let imaged = native_uv
        .chunks_exact(2)
        .filter(|uv| uv[0].is_finite())
        .count();
    let mut mismatches = 0usize;
    let mut max_abs_diff = 0.0_f64;
    for (i, (a, b)) in native_uv.iter().zip(&wasm_uv).enumerate() {
        if a.to_bits() != b.to_bits() {
            if mismatches < 10 {
                eprintln!(
                    "mismatch at output {i} (point {}): native {a:e} ({:016x}) vs wasm {b:e} ({:016x})",
                    i / 2,
                    a.to_bits(),
                    b.to_bits()
                );
            }
            mismatches += 1;
            if a.is_finite() && b.is_finite() {
                max_abs_diff = max_abs_diff.max((a - b).abs());
            }
        }
    }

    println!("G0.1 wasm/native projection parity");
    println!("  points:            {n_points}");
    println!("  imaged:            {imaged}");
    println!("  not imaged (NaN):  {}", n_points - imaged);
    println!("  output values:     {}", native_uv.len());
    println!("  bit mismatches:    {mismatches}");
    println!("  max |Δ| (px):      {max_abs_diff:e}");
    if mismatches == 0 {
        println!("  result:            PASS (bit-for-bit)");
        ExitCode::SUCCESS
    } else {
        println!("  result:            FAIL");
        ExitCode::FAILURE
    }
}
