//! Gate G1.2 measurement: FK(IK(T)) round trips on the 10 000 reachable
//! poses of each FK fixture (`tools/fixtures/fk/<robot>.json`).
//!
//! ```bash
//! cargo run --release -p etendue-kinematics --example g1_2_ik [--features opw]
//! ```
//!
//! For DLS it reports, per robot and seed policy, the failure rate, the
//! iteration percentiles (p50 / p99 / max, summed over restarts), and the
//! worst residual of the successes. Seeds: `near` = true configuration +
//! U(−0.1, 0.1) rad per joint; `random` = uniform within the joint limits
//! (then deterministic restarts).

use std::path::{Path, PathBuf};

use etendue_kinematics::{IkOptions, RobotModel};
use etendue_scene::RobotManifest;
use nalgebra::Isometry3;
use serde_json::Value;

/// SplitMix64, as in the tests.
struct Rng(u64);
impl Rng {
    fn uniform(&mut self, lo: f64, hi: f64) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        lo + (hi - lo) * ((z >> 11) as f64 / (1u64 << 53) as f64)
    }
}

fn percentile(sorted: &[usize], p: f64) -> usize {
    if sorted.is_empty() {
        return 0;
    }
    sorted[((p * (sorted.len() - 1) as f64).round() as usize).min(sorted.len() - 1)]
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(root.join("tools/fixtures/fk"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    paths.sort();
    let opts = IkOptions::default();
    println!(
        "| robot | seeds | poses | failures | failure rate | iters p50 | iters p99 | iters max | worst residual (m / rad) |"
    );
    println!("|---|---|---|---|---|---|---|---|---|");
    for path in paths {
        let fixture: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let id = fixture["robot"].as_str().unwrap();
        let manifest_path = root.join(format!("assets/robots/{id}/robot.json"));
        let manifest: RobotManifest =
            serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
        let urdf =
            std::fs::read_to_string(manifest_path.parent().unwrap().join(&manifest.urdf)).unwrap();
        let model = RobotModel::from_urdf_str(&urdf, &manifest).unwrap();
        let samples = fixture["samples"].as_array().unwrap();
        for policy in ["near", "random"] {
            let mut rng = Rng(20260926);
            let (mut fails, mut iters, mut worst_t, mut worst_r) =
                (0usize, Vec::new(), 0.0_f64, 0.0_f64);
            for s in samples {
                let q_true: Vec<f64> = s["q"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_f64().unwrap())
                    .collect();
                let target: Isometry3<f64> =
                    serde_json::from_value(s["base_se3_tcp"].clone()).unwrap();
                let seed: Vec<f64> = if policy == "near" {
                    q_true.iter().map(|v| v + rng.uniform(-0.1, 0.1)).collect()
                } else {
                    model
                        .active_joints()
                        .iter()
                        .map(|j| rng.uniform(j.lower, j.upper))
                        .collect()
                };
                match model.ik(&target, &seed, &opts) {
                    Ok(sol) => {
                        let reached = model.tcp_pose(&sol.q);
                        worst_t = worst_t
                            .max((reached.translation.vector - target.translation.vector).norm());
                        worst_r = worst_r.max(reached.rotation.angle_to(&target.rotation));
                        iters.push(sol.iterations);
                    }
                    Err(_) => fails += 1,
                }
            }
            iters.sort_unstable();
            println!(
                "| {id} | {policy} | {} | {fails} | {:.2} % | {} | {} | {} | {worst_t:.2e} / {worst_r:.2e} |",
                samples.len(),
                100.0 * fails as f64 / samples.len() as f64,
                percentile(&iters, 0.5),
                percentile(&iters, 0.99),
                iters.last().copied().unwrap_or(0),
            );
        }
    }
}
