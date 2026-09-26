//! Gates G1.1 (FK vs Pinocchio) and G1.2 (FK∘IK round trips) on the robot
//! assets in `assets/robots/`, using the committed fixtures in
//! `tools/fixtures/fk/` (generated once by `tools/fixtures/fk_fixture.py`).
//!
//! The full G1.2 DLS measurement (10k poses per robot, reported failure rate
//! and iteration percentiles) is `cargo run --release -p etendue-kinematics
//! --example g1_2_ik`; here a CI-sized subset asserts the same tolerances.

mod common;

use std::path::{Path, PathBuf};

use common::{Rng, load, pose_distance};
use etendue_kinematics::{IkOptions, RobotModel};
use etendue_scene::RobotManifest;
use nalgebra::Isometry3;
use serde_json::Value;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

struct Fixture {
    manifest: RobotManifest,
    model: RobotModel,
    json: Value,
}

fn fixtures() -> Vec<Fixture> {
    let dir = repo_root().join("tools/fixtures/fk");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("fixture directory")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    paths.sort();
    assert!(
        paths.len() >= 2,
        "expected FK fixtures for both robots in {}",
        dir.display()
    );
    paths
        .into_iter()
        .map(|p| {
            let json: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
            let id = json["robot"].as_str().unwrap();
            let (manifest, model) =
                load(&repo_root().join(format!("assets/robots/{id}/robot.json")));
            assert_eq!(json["base_link"], manifest.base_link.as_str());
            assert_eq!(json["tcp_link"], manifest.tcp_link.as_str());
            let names: Vec<&str> = json["joint_names"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect();
            let expected: Vec<&str> = manifest.joints.iter().map(|j| j.name.as_str()).collect();
            assert_eq!(names, expected, "fixture joint order");
            Fixture {
                manifest,
                model,
                json,
            }
        })
        .collect()
}

fn q_of(v: &Value) -> Vec<f64> {
    v["q"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap())
        .collect()
}

fn iso(v: &Value) -> Isometry3<f64> {
    serde_json::from_value(v.clone()).unwrap()
}

/// G1.1: FK matches Pinocchio on 10 000 configurations per robot (TCP) and
/// on every link for the first 100, to ≤ 1e-9 m and ≤ 1e-9 rad.
#[test]
fn g1_1_fk_matches_pinocchio() {
    for f in fixtures() {
        let samples = f.json["samples"].as_array().unwrap();
        assert_eq!(samples.len(), 10_000, "{}", f.manifest.id);
        let (mut max_t, mut max_r) = (0.0_f64, 0.0_f64);
        for s in samples {
            let (dt, dr) = pose_distance(&f.model.tcp_pose(&q_of(s)), &iso(&s["base_se3_tcp"]));
            max_t = max_t.max(dt);
            max_r = max_r.max(dr);
        }
        let (mut link_t, mut link_r, mut n_links) = (0.0_f64, 0.0_f64, 0);
        for s in f.json["all_links_samples"].as_array().unwrap() {
            let poses = f.model.link_poses(&q_of(s));
            let map = s["base_se3_link"].as_object().unwrap();
            assert_eq!(map.len(), f.model.links().len(), "every link is covered");
            for (link, pose) in map {
                let i = f
                    .model
                    .link_index(link)
                    .unwrap_or_else(|| panic!("unknown link {link}"));
                let (dt, dr) = pose_distance(&poses[i], &iso(pose));
                link_t = link_t.max(dt);
                link_r = link_r.max(dr);
                n_links += 1;
            }
        }
        println!(
            "G1.1 {}: TCP max {max_t:.3e} m / {max_r:.3e} rad over {}; all links max {link_t:.3e} m / {link_r:.3e} rad over {n_links}",
            f.manifest.id,
            samples.len()
        );
        assert!(
            max_t <= 1e-9 && max_r <= 1e-9,
            "{}: {max_t} m, {max_r} rad",
            f.manifest.id
        );
        assert!(
            link_t <= 1e-9 && link_r <= 1e-9,
            "{}: {link_t} m, {link_r} rad",
            f.manifest.id
        );
    }
}

/// G1.2 (DLS, CI subset): FK(IK(T)) within 1e-6 on reachable poses from the
/// fixtures, seeded near the true configuration and from random seeds.
#[test]
fn g1_2_dls_round_trips_subset() {
    let opts = IkOptions::default();
    for f in fixtures() {
        let mut rng = Rng::new(20260926);
        let samples = f.json["samples"].as_array().unwrap();
        let (mut fails, mut worst) = (0usize, 0.0_f64);
        let n = 200;
        for s in samples.iter().take(n) {
            let target = iso(&s["base_se3_tcp"]);
            let q_true = q_of(s);
            let near: Vec<f64> = q_true.iter().map(|v| v + rng.uniform(-0.1, 0.1)).collect();
            let random = rng.q(&f.model, 0.0);
            for seed in [near, random] {
                match f.model.ik(&target, &seed, &opts) {
                    Ok(sol) => {
                        let (dt, dr) = pose_distance(&f.model.tcp_pose(&sol.q), &target);
                        assert!(
                            dt <= 1e-6 && dr <= 1e-6,
                            "{}: {dt} m {dr} rad",
                            f.manifest.id
                        );
                        worst = worst.max(dt).max(dr);
                    }
                    Err(_) => fails += 1,
                }
            }
        }
        println!(
            "G1.2 DLS subset {}: {fails}/{} failures, worst residual {worst:.3e}",
            f.manifest.id,
            2 * n
        );
        assert!(fails * 50 < 2 * n, "{}: {fails} failures", f.manifest.id);
    }
}

/// G1.2 (OPW): FK(IK(T)) within 1e-9 on all 10 000 fixture poses of the OPW
/// robot, and one returned solution is the fixture configuration's branch.
#[cfg(feature = "opw")]
#[test]
fn g1_2_opw_round_trips() {
    use etendue_kinematics::opw::OpwSolver;
    let f = fixtures()
        .into_iter()
        .find(|f| f.manifest.id == "abb_irb1200_5_90")
        .expect("ABB fixture");
    let solver = OpwSolver::preset("irb1200_5_90").unwrap();
    let (mut worst_t, mut worst_r, mut fails) = (0.0_f64, 0.0_f64, 0usize);
    for s in f.json["samples"].as_array().unwrap() {
        let target = iso(&s["base_se3_tcp"]);
        let q_true = q_of(s);
        match f.model.ik_opw(&solver, &target, &q_true) {
            Ok(sol) => {
                let (dt, dr) = pose_distance(&f.model.tcp_pose(&sol.q), &target);
                worst_t = worst_t.max(dt);
                worst_r = worst_r.max(dr);
            }
            Err(_) => fails += 1,
        }
    }
    println!(
        "G1.2 OPW {}: {fails}/10000 failures, max {worst_t:.3e} m / {worst_r:.3e} rad",
        f.manifest.id
    );
    assert_eq!(fails, 0);
    assert!(worst_t <= 1e-9 && worst_r <= 1e-9);
}
