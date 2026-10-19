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

/// JSON text of anything serialisable, for comparing with the wasm side's.
mod erased {
    pub trait Json {
        fn text(&self) -> String;
    }
    impl<T: serde::Serialize> Json for T {
        fn text(&self) -> String {
            serde_json::to_string(self).expect("serialise")
        }
    }
}
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
    // The triangulation head (P10).
    let head = &probe["head"];
    let text = |key: &str| head[key].as_str().expect("head probe text");
    let native_map =
        etendue_wasm::head_map(text("design"), None, [16, 12]).expect("native head map");
    let wasm_map = decode(&head["map"]);
    for (i, (a, b)) in native_map.iter().zip(&wasm_map).enumerate() {
        if a.to_bits() != b.to_bits() {
            if mismatches < 10 {
                eprintln!("head map: mismatch at {i}: native {a:e} vs wasm {b:e}");
            }
            mismatches += 1;
        }
    }
    total += native_map.len();
    let json = |v: &dyn erased::Json| v.text();
    let section = etendue_wasm::head_section(
        text("design"),
        r#"{"probes":[0.25,0.32,0.4],"blur_limit_px":2}"#,
    )
    .expect("native section");
    let scheimpflug = etendue_wasm::head_scheimpflug(text("design")).expect("native scheimpflug");
    let optimum =
        etendue_wasm::head_optimise(text("design"), text("search")).expect("native optimum");
    for (name, native) in [
        ("section", json(&section)),
        ("scheimpflug", json(&scheimpflug)),
        ("optimum", json(&optimum)),
    ] {
        if native != text(name) {
            eprintln!(
                "head {name} differs:\n  native {native}\n  wasm   {}",
                text(name)
            );
            mismatches += 1;
        }
    }
    println!(
        "  head               map {} values, section, Scheimpflug, optimum",
        native_map.len()
    );
    // G12.1: the UR5e's forward kinematics, a TCP and an IK answer (P12).
    let robot = &probe["robot"];
    let q = decode(&robot["q"]);
    let wasm_poses = decode(&robot["link_poses"]);
    let mut native_poses = Vec::with_capacity(wasm_poses.len());
    for joints in q.chunks_exact(6) {
        native_poses.extend(session.robot_link_poses("ur5e", joints).expect("native FK"));
    }
    let fk_differ = native_poses
        .iter()
        .zip(&wasm_poses)
        .filter(|(a, b)| a.to_bits() != b.to_bits())
        .count()
        + native_poses.len().abs_diff(wasm_poses.len());
    if fk_differ > 0 {
        eprintln!("robot FK: {fk_differ} values differ");
    }
    mismatches += fk_differ;
    total += native_poses.len();
    let tcp = session
        .robot_tcp("ur5e", &[0.3, -1.0, 1.2, -1.6, -1.4, 0.5])
        .expect("native TCP");
    let tcp_text = serde_json::to_string(&tcp).expect("TCP JSON");
    let ik = session
        .robot_ik(
            "ur5e",
            &serde_json::to_string(&tcp.base_se3_tcp).expect("pose JSON"),
            &decode(&robot["ik_seed"]),
            r#"{"restarts":0}"#,
        )
        .expect("native IK");
    let ik_text = serde_json::to_string(&ik).expect("IK JSON");
    for (name, native) in [("tcp", tcp_text), ("ik", ik_text)] {
        if native != robot[name].as_str().expect("robot probe text") {
            eprintln!(
                "robot {name} differs:\n  native {native}\n  wasm   {}",
                robot[name]
            );
            mismatches += 1;
        }
    }
    println!(
        "  robot              FK {} values at {} joint vectors, TCP, IK",
        native_poses.len(),
        q.len() / 6
    );
    // A ring of heads' coverage (P11).
    let rig = &probe["rig"];
    let text = |key: &str| rig[key].as_str().expect("rig probe text");
    let native = etendue_wasm::rig_coverage(text("layout"), text("grid"), text("criteria"))
        .expect("native rig coverage");
    let wasm: Vec<u8> = rig["coverage"]
        .as_array()
        .expect("rig coverage")
        .iter()
        .map(|v| u8::try_from(v.as_u64().expect("a count")).expect("a count"))
        .collect();
    let differ = native.iter().zip(&wasm).filter(|(a, b)| a != b).count()
        + native.len().abs_diff(wasm.len());
    if differ > 0 {
        eprintln!("rig coverage: {differ} counts differ");
    }
    mismatches += differ;
    total += native.len();
    println!(
        "  rig                coverage {} counts ({} seen)",
        native.len(),
        native[..native.len() / 2]
            .iter()
            .filter(|&&v| v > 0)
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
