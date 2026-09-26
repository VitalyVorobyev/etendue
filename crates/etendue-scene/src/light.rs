//! Light sources.
//!
//! Directional lights (spot, area) emit along their local **+Z** axis — the
//! same forward axis as cameras and lasers. Renderer backends convert at
//! their edge (Blender lights emit along local −Z).

use nalgebra::Isometry3;
use serde::{Deserialize, Serialize};

use crate::frame::FrameRef;

/// A light source.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct LightSpec {
    /// Unique id.
    pub id: String,
    /// Parent frame.
    pub parent: FrameRef,
    /// Pose of the light in its parent (maps light → parent).
    #[cfg_attr(
        feature = "schemars",
        schemars(with = "vision_calibration_core::Iso3Schema")
    )]
    pub parent_se3_self: Isometry3<f64>,
    /// Radiant power in watts.
    pub power_w: f64,
    /// Linear RGB colour, each channel in `[0, 1]`.
    pub color: [f64; 3],
    /// Emitter shape.
    pub shape: LightShape,
}

/// Emitter shape, tagged on `type`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum LightShape {
    /// Isotropic point (or small sphere) light.
    Point {
        /// Emitter radius in metres (`0` = ideal point).
        radius_m: f64,
    },
    /// Cone light along local +Z.
    Spot {
        /// Full cone angle in radians, in `(0, π)`.
        cone_angle: f64,
        /// Soft-edge fraction of the cone, in `[0, 1]`.
        blend: f64,
    },
    /// Rectangular area light in the local `z = 0` plane, emitting along +Z.
    Area {
        /// Extent `[x, y]` in metres.
        size_m: [f64; 2],
    },
}
