//! Robots: their placement in a scene ([`RobotSpec`]) and the asset manifest
//! that describes a robot model ([`RobotManifest`], the `robot.json` written
//! by `tools/robot-assets`).

use std::collections::BTreeMap;

use nalgebra::Isometry3;
use serde::{Deserialize, Serialize};

use crate::frame::FrameRef;
use crate::validate::{Issues, ValidationError};

/// A robot placed in a scene.
///
/// The robot contributes one frame per URDF link, named
/// `"<id>/<link_name>"`. `parent_se3_self` places the manifest's
/// [`base_link`](RobotManifest::base_link) in the parent frame.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct RobotSpec {
    /// Unique id (shared namespace with every entity).
    pub id: String,
    /// Parent frame of the robot base.
    pub parent: FrameRef,
    /// Pose of the robot base link in its parent (`parent_se3_base`).
    #[cfg_attr(
        feature = "schemars",
        schemars(with = "vision_calibration_core::Iso3Schema")
    )]
    pub parent_se3_self: Isometry3<f64>,
    /// Path to the robot's `robot.json` manifest, relative to the scene file.
    pub manifest: String,
    /// Joint positions at `t = 0`, in manifest joint order (rad or m).
    /// Defaults to all zeros.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_q: Option<Vec<f64>>,
}

/// A robot asset manifest (`robot.json`), version 1.
///
/// Written by `tools/robot-assets/build.py`; consumed by
/// `etendue-kinematics` (joint limits, base and TCP links) and by the web
/// viewer (per-link meshes).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct RobotManifest {
    /// Manifest format version; must be `1`.
    pub version: u32,
    /// Robot model id (e.g. `"ur5e"`).
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Expanded URDF, path relative to this manifest. Used for kinematics.
    pub urdf: String,
    /// The robot's base link (the frame a [`RobotSpec`] places).
    pub base_link: String,
    /// The link whose pose the controller reports (the calibration-rs
    /// "gripper" frame; `robot_poses` are exported as `base_se3_<tcp_link>`).
    pub tcp_link: String,
    /// Movable joints on the path `base_link → tcp_link`, in that order. This
    /// order defines the joint vector `q` everywhere.
    pub joints: Vec<ManifestJoint>,
    /// Per-link visual meshes (links without visual geometry are absent).
    pub visuals: Vec<ManifestVisual>,
    /// Where the description came from.
    pub source: ManifestSource,
    /// Licences of the description and meshes.
    pub license: ManifestLicense,
}

/// Motion limits of one joint.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ManifestJoint {
    /// URDF joint name.
    pub name: String,
    /// Lower position limit (rad or m).
    pub lower: f64,
    /// Upper position limit (rad or m).
    pub upper: f64,
    /// Maximum speed (rad/s or m/s), `> 0`.
    pub max_velocity: f64,
    /// Maximum acceleration (rad/s² or m/s²), `> 0`.
    pub max_acceleration: f64,
    /// Provenance of the limits (file and key, datasheet, or documented
    /// default).
    pub limit_source: String,
}

/// A link's visual mesh.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ManifestVisual {
    /// URDF link name.
    pub link: String,
    /// glTF binary in the **link frame** (URDF visual origin and scale
    /// applied), path relative to the manifest.
    pub mesh: String,
}

/// Upstream source of a robot description, pinned by revision.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ManifestSource {
    /// Repository URL.
    pub repository: String,
    /// Pinned git commit SHA.
    pub revision: String,
    /// xacro / URDF entry file inside the repository.
    pub entry: String,
    /// xacro arguments used for expansion.
    pub xacro_args: BTreeMap<String, String>,
}

/// Licences of a robot description.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ManifestLicense {
    /// Licence of the URDF / xacro (SPDX id).
    pub urdf: String,
    /// Licence of the meshes (SPDX id or `LicenseRef-…` with a description).
    pub meshes: String,
    /// Whether the meshes may be redistributed (committed / published).
    pub meshes_redistributable: bool,
    /// Evidence and remarks.
    pub notes: String,
}

impl RobotManifest {
    /// Structural checks: version, non-empty names, finite ordered limits,
    /// positive velocity and acceleration limits, unique joint names.
    ///
    /// Whether the joints actually lie on the URDF path `base_link →
    /// tcp_link` is checked by `etendue-kinematics` when it loads the URDF.
    ///
    /// # Errors
    ///
    /// Every problem found.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut issues = Issues::default();
        if self.version != 1 {
            issues.push(
                "version",
                format!("unsupported manifest version {}", self.version),
            );
        }
        for (field, value) in [
            ("id", &self.id),
            ("urdf", &self.urdf),
            ("base_link", &self.base_link),
            ("tcp_link", &self.tcp_link),
        ] {
            if value.is_empty() {
                issues.push(field, "must not be empty");
            }
        }
        if self.joints.is_empty() {
            issues.push("joints", "a robot needs at least one movable joint");
        }
        for (i, j) in self.joints.iter().enumerate() {
            let path = format!("joints[{i}]");
            if self.joints[..i].iter().any(|o| o.name == j.name) {
                issues.push(&path, format!("duplicate joint `{}`", j.name));
            }
            if !(j.lower.is_finite() && j.upper.is_finite() && j.lower <= j.upper) {
                issues.push(
                    &path,
                    format!(
                        "position limits must be finite with lower ≤ upper, got [{}, {}]",
                        j.lower, j.upper
                    ),
                );
            }
            issues.check_positive(&format!("{path}.max_velocity"), j.max_velocity);
            issues.check_positive(&format!("{path}.max_acceleration"), j.max_acceleration);
        }
        issues.finish()
    }
}
