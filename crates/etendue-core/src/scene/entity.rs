//! The posed laser and target entities the laser stripe model consumes
//! ([`crate::laser`], and `etendue-synth`'s stripes on targets).
//!
//! A [`LaserEntity`] is a line-projecting laser and a [`TargetEntity`] the
//! surface it strikes (a rectangle, or a mesh: [`MeshTarget`]). Cameras are
//! [`crate::optics::PhysicalCamera`] (ADR 0008), resolved from the scene
//! documents by [`crate::from_scene`].
//!
//! # Coordinate conventions
//!
//! - **World frame.** Right-handed, `+Z`-up, the ground in the `z = 0` plane
//!   (the scene documents' world frame). Entity poses place objects in this
//!   frame.
//! - **Entity-local frame.** Every entity owns a `pose: Isometry3<f64>` that
//!   maps a point expressed in the entity's *local* frame into the world
//!   frame (`p_world = pose * p_local`). The inverse, `pose.inverse()`, takes
//!   world points into the entity's local frame.
//! - **Laser-local frame.** A [`LaserEntity`] emits a fan in its local
//!   `x = 0` plane (the *fan plane*): the fan opens symmetrically about the
//!   local `+z` axis (the fan's central ray), spreading within the `y–z`
//!   plane. `LaserEntity::pose` is "world ← laser-local".
//! - **Target-local frame.** A [`TargetEntity`] is a rectangle in its local
//!   `z = 0` plane, centred on the local origin, with its outward normal along
//!   local `+z`. `TargetEntity::pose` is "world ← target-local".
//!
//! Units are SI: metres for lengths, radians for angles. Wavelength is the one
//! exception and is stored in nanometres (the unit lasers are specified in).

use nalgebra::Isometry3;
use serde::{Deserialize, Serialize};

use crate::geom::TriMesh;

/// A line-projecting laser placed in the world.
///
/// The laser is a flat fan with a half-angle and a finite extent, plus the
/// one physical-optics parameter the line model needs: the Gaussian-beam
/// waist. The geometric line model itself — a [`LaserPlane`] with a
/// Gaussian-beam cross-section width — lives in [`crate::laser`]; this entity
/// only carries the parameters it consumes.
///
/// [`LaserPlane`]: crate::laser::LaserPlane
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LaserEntity {
    /// Pose of the laser in the world: maps laser-local coordinates into world
    /// coordinates (`world ← laser-local`). The fan lies in the local `x = 0`
    /// plane and opens about the local `+z` axis.
    pub pose: Isometry3<f64>,
    /// Half-angle of the fan, in radians: the fan spans `±fan_half_angle`
    /// about the local `+z` axis within the local `y–z` plane.
    pub fan_half_angle: f64,
    /// Length of the fan along its central `+z` ray, in metres — how far the
    /// rendered fan reaches from the laser origin.
    pub fan_length: f64,
    /// Emission wavelength, in nanometres (e.g. `660.0` for a red diode).
    pub wavelength_nm: f64,
    /// Beam-waist radius `w0`, in **metres** — the `1/e²` radius of the laser
    /// beam's cross-section at its narrowest point. The
    /// [`GaussianBeamWidth`](crate::laser::GaussianBeamWidth) model places the
    /// waist at [`waist_distance_m`](Self::waist_distance_m) and grows the
    /// cross-section away from it via `w(d) = w0·sqrt(1 + ((d − d0)/zR)²)`,
    /// `zR = π·w0²/λ`. A typical machine-vision line laser focuses to a waist
    /// of a few hundred micrometres (e.g. `2.5e-4` m for a 0.25 mm waist).
    pub beam_waist_m: f64,
    /// Distance `d0` from the laser origin to the waist along each ray, in
    /// metres: where a focusable laser is focused. 0 (the default) puts the
    /// waist at the laser itself.
    #[serde(default)]
    pub waist_distance_m: f64,
}

impl LaserEntity {
    /// Build a laser entity, validating the fan geometry, wavelength, and
    /// beam waist.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidInput`](crate::Error::InvalidInput) if
    /// `fan_half_angle` is not finite or not in the open interval
    /// `(0, π/2)`, if `fan_length` is not finite and strictly positive, if
    /// `wavelength_nm` is not finite and strictly positive, or if
    /// `beam_waist_m` is not finite and strictly positive.
    pub fn new(
        pose: Isometry3<f64>,
        fan_half_angle: f64,
        fan_length: f64,
        wavelength_nm: f64,
        beam_waist_m: f64,
    ) -> crate::Result<Self> {
        if !(fan_half_angle.is_finite()
            && fan_half_angle > 0.0
            && fan_half_angle < std::f64::consts::FRAC_PI_2)
        {
            return Err(crate::Error::InvalidInput {
                reason: format!(
                    "laser fan half-angle must be finite and in (0, π/2), got {fan_half_angle}"
                ),
            });
        }
        if !(fan_length.is_finite() && fan_length > 0.0) {
            return Err(crate::Error::InvalidInput {
                reason: format!("laser fan length must be finite and positive, got {fan_length}"),
            });
        }
        if !(wavelength_nm.is_finite() && wavelength_nm > 0.0) {
            return Err(crate::Error::InvalidInput {
                reason: format!(
                    "laser wavelength must be finite and positive, got {wavelength_nm}"
                ),
            });
        }
        if !(beam_waist_m.is_finite() && beam_waist_m > 0.0) {
            return Err(crate::Error::InvalidInput {
                reason: format!(
                    "laser beam-waist radius must be finite and positive, got {beam_waist_m}"
                ),
            });
        }
        Ok(Self {
            pose,
            fan_half_angle,
            fan_length,
            wavelength_nm,
            beam_waist_m,
            waist_distance_m: 0.0,
        })
    }

    /// The same laser focused at `waist_distance_m` from its origin.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidInput`](crate::Error::InvalidInput) if the
    /// distance is not finite and non-negative.
    pub fn with_waist_distance(self, waist_distance_m: f64) -> crate::Result<Self> {
        if !(waist_distance_m.is_finite() && waist_distance_m >= 0.0) {
            return Err(crate::Error::InvalidInput {
                reason: format!(
                    "laser beam-waist distance must be finite and non-negative, got {waist_distance_m}"
                ),
            });
        }
        Ok(Self {
            waist_distance_m,
            ..self
        })
    }
}

/// A planar inspection target placed in the world.
///
/// The target is a finite rectangle. For triangle-mesh targets see
/// [`MeshTarget`], which supports fan-plane × triangle-mesh
/// laser-stripe intersection via `laser::stripe_segments_on_mesh`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TargetEntity {
    /// Pose of the target in the world: maps target-local coordinates into
    /// world coordinates (`world ← target-local`). The target rectangle lies
    /// in the local `z = 0` plane with its outward normal along local `+z`.
    pub pose: Isometry3<f64>,
    /// Width of the target rectangle (local `x` extent), in metres.
    pub width: f64,
    /// Height of the target rectangle (local `y` extent), in metres.
    pub height: f64,
}

impl TargetEntity {
    /// Build a target entity, validating the rectangle extent.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidInput`](crate::Error::InvalidInput) if `width`
    /// or `height` is not finite and strictly positive.
    pub fn new(pose: Isometry3<f64>, width: f64, height: f64) -> crate::Result<Self> {
        if !(width.is_finite() && width > 0.0) {
            return Err(crate::Error::InvalidInput {
                reason: format!("target width must be finite and positive, got {width}"),
            });
        }
        if !(height.is_finite() && height > 0.0) {
            return Err(crate::Error::InvalidInput {
                reason: format!("target height must be finite and positive, got {height}"),
            });
        }
        Ok(Self {
            pose,
            width,
            height,
        })
    }
}

/// A triangle-mesh inspection target placed in the world.
///
/// The mesh target complements [`TargetEntity`] (a simple planar rectangle)
/// with an arbitrary triangle mesh, enabling the laser-fan × mesh
/// intersection path ([`crate::laser::stripe_segments_on_mesh`]).
///
/// The mesh is stored in **mesh-local** coordinates; `pose` maps mesh-local
/// into the world frame (`world ← mesh-local`). This mirrors the local-frame
/// convention of [`TargetEntity`].
///
/// # Constructing from application code
///
/// Use [`MeshTarget::new`] for validated construction, or build the [`TriMesh`]
/// with one of the primitive constructors ([`TriMesh::unit_cube`],
/// [`TriMesh::quad`]) and pass it in. A unit-cube mesh target is the
/// recommended default mesh target.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MeshTarget {
    /// Pose of the mesh in the world: maps mesh-local coordinates into world
    /// coordinates (`world ← mesh-local`).
    pub pose: Isometry3<f64>,
    /// The triangle mesh in mesh-local coordinates.
    pub mesh: TriMesh,
}

impl MeshTarget {
    /// Build a mesh target from a pose and a triangle mesh.
    ///
    /// Validates that the mesh has at least one triangle.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidInput`](crate::Error::InvalidInput) if the mesh
    /// has no triangles.
    pub fn new(pose: Isometry3<f64>, mesh: TriMesh) -> crate::Result<Self> {
        if mesh.triangle_count() == 0 {
            return Err(crate::Error::InvalidInput {
                reason: "mesh target must have at least one triangle".to_string(),
            });
        }
        Ok(Self { pose, mesh })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use nalgebra::{Point3, Translation3, UnitQuaternion, Vector3};

    /// A plausible machine-vision line-laser beam waist: 0.25 mm radius.
    const TEST_WAIST_M: f64 = 0.25e-3;

    #[test]
    fn laser_entity_accepts_a_valid_fan() {
        let laser = LaserEntity::new(Isometry3::identity(), 0.3, 1.5, 660.0, TEST_WAIST_M)
            .expect("valid laser entity");
        assert_relative_eq!(laser.fan_half_angle, 0.3, epsilon = 1e-12);
        assert_relative_eq!(laser.fan_length, 1.5, epsilon = 1e-12);
        assert_relative_eq!(laser.beam_waist_m, TEST_WAIST_M, epsilon = 1e-15);
    }

    #[test]
    fn laser_entity_rejects_degenerate_fan_angle() {
        // Zero half-angle.
        assert!(LaserEntity::new(Isometry3::identity(), 0.0, 1.0, 660.0, TEST_WAIST_M).is_err());
        // Half-angle at/over π/2 would fold the fan back on itself.
        assert!(
            LaserEntity::new(
                Isometry3::identity(),
                std::f64::consts::FRAC_PI_2,
                1.0,
                660.0,
                TEST_WAIST_M,
            )
            .is_err()
        );
    }

    #[test]
    fn laser_entity_rejects_bad_wavelength() {
        assert!(LaserEntity::new(Isometry3::identity(), 0.3, 1.0, 0.0, TEST_WAIST_M).is_err());
        assert!(LaserEntity::new(Isometry3::identity(), 0.3, 1.0, -660.0, TEST_WAIST_M).is_err());
    }

    #[test]
    fn laser_entity_rejects_bad_beam_waist() {
        assert!(LaserEntity::new(Isometry3::identity(), 0.3, 1.0, 660.0, 0.0).is_err());
        assert!(LaserEntity::new(Isometry3::identity(), 0.3, 1.0, 660.0, -1e-4).is_err());
        assert!(LaserEntity::new(Isometry3::identity(), 0.3, 1.0, 660.0, f64::NAN).is_err());
    }

    #[test]
    fn target_entity_accepts_a_valid_rectangle() {
        let target =
            TargetEntity::new(Isometry3::identity(), 0.4, 0.3).expect("valid target entity");
        assert_relative_eq!(target.width, 0.4, epsilon = 1e-12);
        assert_relative_eq!(target.height, 0.3, epsilon = 1e-12);
    }

    #[test]
    fn target_entity_rejects_non_positive_extent() {
        assert!(TargetEntity::new(Isometry3::identity(), 0.0, 0.3).is_err());
        assert!(TargetEntity::new(Isometry3::identity(), 0.4, -0.3).is_err());
    }

    #[test]
    fn pose_maps_local_origin_to_its_translation() {
        // A pose's translation is, by construction, where the local origin
        // lands in the world — a sanity check on the "world ← local" reading.
        let translation = Translation3::new(1.0, -2.0, 0.5);
        let rotation = UnitQuaternion::from_axis_angle(&Vector3::z_axis(), 0.7);
        let pose = Isometry3::from_parts(translation, rotation);
        let world_origin = pose * Point3::origin();
        assert_relative_eq!(world_origin.x, 1.0, epsilon = 1e-12);
        assert_relative_eq!(world_origin.y, -2.0, epsilon = 1e-12);
        assert_relative_eq!(world_origin.z, 0.5, epsilon = 1e-12);
    }
}
