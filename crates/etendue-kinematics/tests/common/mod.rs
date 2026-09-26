//! Shared test helpers.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use etendue_kinematics::RobotModel;
use etendue_scene::RobotManifest;
use nalgebra::Isometry3;

/// Directory of this crate's test data.
pub fn data_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data")
}

/// Load a manifest and its URDF (path relative to the manifest).
pub fn load(manifest_path: &Path) -> (RobotManifest, RobotModel) {
    let manifest: RobotManifest =
        serde_json::from_str(&std::fs::read_to_string(manifest_path).expect("read manifest"))
            .expect("parse manifest");
    let urdf = std::fs::read_to_string(manifest_path.parent().unwrap().join(&manifest.urdf))
        .expect("read urdf");
    let model = RobotModel::from_urdf_str(&urdf, &manifest).expect("robot model");
    (manifest, model)
}

/// The hand-written 6R test arm.
pub fn test_arm() -> RobotModel {
    load(&data_dir().join("test_arm.json")).1
}

/// SplitMix64: a tiny deterministic generator for test inputs.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[lo, hi)`.
    pub fn uniform(&mut self, lo: f64, hi: f64) -> f64 {
        let u = (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
        lo + (hi - lo) * u
    }

    /// A joint vector uniform within the model's limits, shrunk by `margin`.
    pub fn q(&mut self, model: &RobotModel, margin: f64) -> Vec<f64> {
        model
            .active_joints()
            .iter()
            .map(|j| self.uniform(j.lower + margin, j.upper - margin))
            .collect()
    }
}

/// Translation and rotation-angle distance between two poses.
pub fn pose_distance(a: &Isometry3<f64>, b: &Isometry3<f64>) -> (f64, f64) {
    (
        (a.translation.vector - b.translation.vector).norm(),
        a.rotation.angle_to(&b.rotation),
    )
}
