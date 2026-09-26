//! Placeable scene entities: rigs, cameras, lasers, targets, and parts.
//!
//! Every entity carries a unique `id`, a `parent` frame, and
//! `parent_se3_self` (maps entity-local coordinates into the parent frame).
//! Local-frame conventions match `etendue-core`:
//!
//! - **Camera**: calibration-rs/OpenCV — +Z forward, +X right, +Y down.
//! - **Laser**: the fan lies in the local `x = 0` plane and opens
//!   symmetrically about local +Z.
//! - **Target**: the surface is the local `z = 0` plane with its outward
//!   normal along local +Z.

use nalgebra::Isometry3;
use serde::{Deserialize, Serialize};
use vision_calibration_core::CameraParams;

use crate::frame::FrameRef;

/// A rigid mount that groups cameras (and optionally lasers). Cameras attach
/// to it with `parent_se3_self = rig_se3_cam`, the direction of calibration-rs
/// `DeviceSpec` `rig_se3_cam`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct RigSpec {
    /// Unique id.
    pub id: String,
    /// Parent frame (e.g. `"world"` or `"<robot>/tool0"` for eye-in-hand).
    pub parent: FrameRef,
    /// Pose of the rig in its parent (maps rig → parent).
    #[cfg_attr(
        feature = "schemars",
        schemars(with = "vision_calibration_core::Iso3Schema")
    )]
    pub parent_se3_self: Isometry3<f64>,
}

/// A camera. Its projection model is a calibration-rs [`CameraParams`] —
/// the same type calibration-rs calibrates and exports.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CameraSpec {
    /// Unique id.
    pub id: String,
    /// Parent frame (usually a rig).
    pub parent: FrameRef,
    /// Pose of the camera in its parent (maps camera → parent).
    #[cfg_attr(
        feature = "schemars",
        schemars(with = "vision_calibration_core::Iso3Schema")
    )]
    pub parent_se3_self: Isometry3<f64>,
    /// Projection / distortion / sensor / intrinsics.
    pub params: CameraParams,
    /// Image size in pixels, `[width, height]`.
    pub resolution: [u32; 2],
}

/// A line laser: a flat fan with a Gaussian-beam cross-section (the
/// `etendue-core` `LaserEntity` parameters).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct LaserSpec {
    /// Unique id.
    pub id: String,
    /// Parent frame.
    pub parent: FrameRef,
    /// Pose of the laser in its parent (maps laser → parent).
    #[cfg_attr(
        feature = "schemars",
        schemars(with = "vision_calibration_core::Iso3Schema")
    )]
    pub parent_se3_self: Isometry3<f64>,
    /// Fan half-angle in radians, in `(0, π/2)`.
    pub fan_half_angle: f64,
    /// Fan reach along its central ray, in metres.
    pub fan_length: f64,
    /// Emission wavelength in nanometres.
    pub wavelength_nm: f64,
    /// Beam-waist `1/e²` radius at the laser origin, in metres.
    pub beam_waist_m: f64,
}

/// A target surface.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct TargetSpec {
    /// Unique id.
    pub id: String,
    /// Parent frame (`"world"`, a fixture, or `"<robot>/tool0"` for
    /// eye-to-hand).
    pub parent: FrameRef,
    /// Pose of the target in its parent (maps target → parent).
    #[cfg_attr(
        feature = "schemars",
        schemars(with = "vision_calibration_core::Iso3Schema")
    )]
    pub parent_se3_self: Isometry3<f64>,
    /// What the target is.
    pub geometry: TargetGeometry,
}

/// Target geometry, tagged on `type`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TargetGeometry {
    /// A calibration board, described in the calibration-rs dataset
    /// vocabulary (the same `TargetSpec` a synthetic `DatasetSpec` emits).
    /// The board is centred on the local origin in the `z = 0` plane, facing
    /// +Z. The in-plane mapping from board (feature) coordinates to the
    /// target frame is defined once, by `etendue-synth` (ADR 0006).
    Board {
        /// Board layout (`kind`-tagged: chessboard, charuco, puzzleboard,
        /// ringgrid).
        board: vision_calibration_dataset::TargetSpec,
    },
    /// A plain rectangle centred on the local origin in the `z = 0` plane
    /// (the `etendue-core` `TargetEntity`).
    Rectangle {
        /// Extent along local X, in metres.
        width: f64,
        /// Extent along local Y, in metres.
        height: f64,
    },
}

/// A passive rigid part rendered from a mesh (a fixture, a workpiece).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PartSpec {
    /// Unique id.
    pub id: String,
    /// Parent frame.
    pub parent: FrameRef,
    /// Pose of the part in its parent (maps part → parent).
    #[cfg_attr(
        feature = "schemars",
        schemars(with = "vision_calibration_core::Iso3Schema")
    )]
    pub parent_se3_self: Isometry3<f64>,
    /// Mesh file (glTF binary), path relative to the scene file. Vertices
    /// are in the part frame, metres.
    pub mesh: String,
}
