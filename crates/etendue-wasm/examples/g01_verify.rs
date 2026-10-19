//! Gate G0.1, native side: recompute the Node probe natively and compare
//! bit-for-bit.
//!
//! ```bash
//! node crates/etendue-wasm/scripts/build-npm.mjs
//! node crates/etendue-wasm/tests/node/g01_parity.mjs > target/g01.json
//! cargo run -p etendue-wasm --example g01_verify -- target/g01.json
//! ```
//!
//! The probe file holds the camera pose and world points the Node script fed
//! to the wasm build, and the pixels it got back per camera, all as IEEE-754
//! bit patterns (16 hex digits). This binary loads the same scene
//! (`examples/eye_in_hand_ur5e`) into a native [`etendue_wasm::Session`],
//! reruns [`etendue_wasm::Session::project_points`], and exits non-zero on
//! any differing bit.

use std::path::Path;
use std::process::ExitCode;

use etendue_wasm::{Session, iso3_from_wire};
use serde_json::{Value, json};

fn decode(v: &Value) -> Vec<f64> {
    v.as_array()
        .expect("an array of bit patterns")
        .iter()
        .map(|h| {
            let h = h.as_str().expect("bit patterns are hex strings");
            f64::from_bits(u64::from_str_radix(h, 16).expect("valid 64-bit hex"))
        })
        .collect()
}

fn read(root: &Path, path: &str) -> String {
    std::fs::read_to_string(root.join(path)).unwrap_or_else(|e| panic!("reading {path}: {e}"))
}

fn main() -> ExitCode {
    let path = std::env::args()
        .nth(1)
        .expect("usage: g01_verify <probe.json>");
    let text = std::fs::read_to_string(&path).expect("read probe file");
    let probe: Value = serde_json::from_str(&text).expect("probe file is JSON");

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest: Value =
        serde_json::from_str(&read(&root, "assets/robots/ur5e/robot.json")).expect("manifest");
    let robots = json!([{
        "id": "ur5e",
        "manifest": manifest,
        "urdf": read(&root, "assets/robots/ur5e/robot.urdf"),
    }]);
    let session = Session::load(
        &read(&root, "examples/eye_in_hand_ur5e/scene.json"),
        &robots.to_string(),
    )
    .expect("example scene loads");

    let pose = iso3_from_wire(&decode(&probe["world_se3_camera"])).expect("pose");
    let xyz = decode(&probe["xyz"]);
    let cameras = probe["cameras"].as_array().expect("cameras");
    let wasm_uv = probe["uv"].as_array().expect("uv");

    println!("G0.1 wasm/native projection parity");
    println!("  points per camera: {}", xyz.len() / 3);
    let mut mismatches = 0usize;
    let mut max_abs_diff = 0.0_f64;
    let mut total = 0usize;
    for (camera, wasm_uv) in cameras.iter().zip(wasm_uv) {
        let id = camera.as_str().expect("camera id");
        let wasm_uv = decode(wasm_uv);
        let native_uv = session
            .project_points(id, &pose, &xyz)
            .expect("native projection");
        assert_eq!(
            native_uv.len(),
            wasm_uv.len(),
            "native and wasm output lengths differ"
        );
        let imaged = native_uv
            .chunks_exact(2)
            .filter(|uv| uv[0].is_finite())
            .count();
        for (i, (a, b)) in native_uv.iter().zip(&wasm_uv).enumerate() {
            if a.to_bits() != b.to_bits() {
                if mismatches < 10 {
                    eprintln!(
                        "{id}: mismatch at output {i} (point {}): native {a:e} ({:016x}) vs wasm {b:e} ({:016x})",
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
        total += native_uv.len();
        println!(
            "  {id:<17}  imaged {imaged}, not imaged (NaN) {}",
            xyz.len() / 3 - imaged
        );
    }
    // ADR 0008: the physical camera, its derived params and its projection frame.
    let optics = &probe["optics"];
    let text = |key: &str| optics[key].as_str().expect("optics probe text");
    let derived = etendue_wasm::derive_optics(text("spec"), text("params")).expect("native derive");
    let native_derived = serde_json::to_string(&derived).expect("serialise");
    if native_derived != text("derived") {
        eprintln!(
            "deriveOptics differs:\n  native {native_derived}\n  wasm   {}",
            text("derived")
        );
        mismatches += 1;
    }
    let physical = Session::load(text("scene"), &robots.to_string()).expect("optics scene loads");
    let native_uv = physical
        .project_points("cam_left", &pose, &xyz)
        .expect("native projection");
    let wasm_uv = decode(&optics["uv"]);
    for (i, (a, b)) in native_uv.iter().zip(&wasm_uv).enumerate() {
        if a.to_bits() != b.to_bits() {
            if mismatches < 10 {
                eprintln!("cam_left (optics): mismatch at output {i}: native {a:e} vs wasm {b:e}");
            }
            mismatches += 1;
        }
    }
    total += native_uv.len();
    let mech = physical
        .mech_se3_projection("cam_left")
        .expect("known camera")
        .expect("a camera with optics");
    let (q, t) = (mech.rotation.coords, mech.translation.vector);
    for (a, b) in [q.x, q.y, q.z, q.w, t.x, t.y, t.z]
        .iter()
        .zip(decode(&optics["mech_se3_projection"]))
    {
        if a.to_bits() != b.to_bits() {
            mismatches += 1;
        }
    }
    println!(
        "  cam_left (optics)  imaged {}, plus deriveOptics and its projection frame",
        native_uv
            .chunks_exact(2)
            .filter(|uv| uv[0].is_finite())
            .count()
    );
    println!("  output values:     {total}");
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
