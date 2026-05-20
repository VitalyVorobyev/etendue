//! [`LaserPlane`] — the geometric model of a line laser's fan.
//!
//! A line laser emits a planar *fan* of light: rays leaving a common origin,
//! all lying in one plane, spreading over an angular extent. [`LaserPlane`]
//! captures that geometry, built from a [`LaserEntity`], in **world**
//! coordinates so the intersection and projection layers ([`super::intersect`],
//! [`super::project`]) can work directly against the scene's other entities.
//!
//! # Laser-local convention (from M2)
//!
//! A [`LaserEntity`]'s fan lies in its local `x = 0` plane and opens
//! symmetrically about the local `+z` axis (the *central ray*), spreading
//! within the local `y–z` plane. A ray of the fan at fan-angle `phi`
//! (`phi = 0` is the central ray, `phi > 0` toward local `+y`) has the
//! local direction `(0, sin phi, cos phi)`. The fan spans
//! `phi ∈ [-fan_half_angle, +fan_half_angle]`, and the rendered/illuminated
//! extent reaches `fan_length` along each edge ray.
//!
//! # What `LaserPlane` exposes
//!
//! Everything below is in **world** coordinates (the [`LaserEntity::pose`]
//! has been applied):
//!
//! - the **origin** — the fan's apex, where every ray starts;
//! - the **plane** the fan lies in — a point (`origin`) and a unit `normal`
//!   (the laser-local `x` axis mapped into the world);
//! - the **central direction** — the local `+z` axis in the world;
//! - the **in-plane axis** — the local `+y` axis in the world, the direction
//!   `phi` opens toward;
//! - the angular extent `±half_angle` and the radial `length`.
//!
//! A point in the fan plane is parameterised as
//! `origin + r·(cos phi·central + sin phi·in_plane)` for a radius `r ≥ 0` and
//! a fan-angle `phi`; [`LaserPlane::fan_angle_of`] and
//! [`LaserPlane::contains_in_extent`] invert and bound that parameterisation.

use nalgebra::{Point3, Vector3};

use crate::scene::LaserEntity;

/// Directions shorter than this (after transform) are treated as degenerate.
/// A laser pose is a rigid isometry, so its mapped axes are always unit
/// length — this guard only protects against a pathological hand-built pose.
const MIN_AXIS_NORM: f64 = 1e-12;

/// The geometric model of a line laser's planar fan, in world coordinates.
///
/// Built from a [`LaserEntity`] by [`LaserPlane::from_entity`]. The fan is a
/// sector of the plane through [`LaserPlane::origin`] with unit normal
/// [`LaserPlane::normal`]: rays leave the origin, span the fan-angle interval
/// `±half_angle` about [`LaserPlane::central`], and reach [`LaserPlane::length`]
/// radially.
///
/// All vectors are unit length and mutually consistent: `central`, `in_plane`,
/// and `normal` are the world images of the laser-local `+z`, `+y`, `+x`
/// axes — a right-handed orthonormal frame, so `normal = in_plane × central`
/// (the standard `x = y × z` relation for the local axes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LaserPlane {
    /// The fan apex — where every ray of the fan originates (world frame).
    origin: Point3<f64>,
    /// Unit normal of the fan plane (world frame): the laser-local `x` axis.
    normal: Vector3<f64>,
    /// Unit central-ray direction (world frame): the laser-local `+z` axis.
    /// The fan-angle `phi` is measured from this direction.
    central: Vector3<f64>,
    /// Unit in-plane direction (world frame): the laser-local `+y` axis. The
    /// fan opens toward `+in_plane` for `phi > 0`.
    in_plane: Vector3<f64>,
    /// Half-angle of the fan, radians: rays span `phi ∈ [-half_angle, +half]`.
    half_angle: f64,
    /// Radial extent of the fan, metres — how far a ray reaches from `origin`.
    length: f64,
}

impl LaserPlane {
    /// Build the world-frame fan model from a [`LaserEntity`].
    ///
    /// Applies the entity's `pose` to the laser-local axes: the apex is the
    /// pose translation, and `central` / `in_plane` / `normal` are the local
    /// `+z` / `+y` / `+x` axes rotated into the world. The angular and radial
    /// extents are copied straight from the entity.
    ///
    /// A [`LaserEntity`] built through [`LaserEntity::new`] always yields a
    /// valid plane: the pose is a rigid isometry (orthonormal axes) and the
    /// fan parameters are pre-validated.
    #[must_use]
    pub fn from_entity(laser: &LaserEntity) -> Self {
        let pose = &laser.pose;
        let origin = pose * Point3::origin();
        // The laser-local axes mapped into the world. A pose rotation is
        // orthonormal, so these come out unit length and mutually orthogonal.
        let central = pose * Vector3::z();
        let in_plane = pose * Vector3::y();
        let normal = pose * Vector3::x();
        Self {
            origin,
            normal,
            central,
            in_plane,
            half_angle: laser.fan_half_angle,
            length: laser.fan_length,
        }
    }

    /// The fan apex, in world coordinates — the common origin of every ray.
    #[must_use]
    pub fn origin(&self) -> Point3<f64> {
        self.origin
    }

    /// The unit normal of the fan plane, in world coordinates.
    #[must_use]
    pub fn normal(&self) -> Vector3<f64> {
        self.normal
    }

    /// The unit central-ray direction, in world coordinates.
    ///
    /// The fan-angle `phi` of a ray is measured from this direction.
    #[must_use]
    pub fn central(&self) -> Vector3<f64> {
        self.central
    }

    /// The unit in-plane direction, in world coordinates — the direction the
    /// fan opens toward for a positive fan-angle.
    #[must_use]
    pub fn in_plane(&self) -> Vector3<f64> {
        self.in_plane
    }

    /// The fan half-angle, in radians. Rays span `±half_angle` about the
    /// central direction.
    #[must_use]
    pub fn half_angle(&self) -> f64 {
        self.half_angle
    }

    /// The fan's radial extent, in metres.
    #[must_use]
    pub fn length(&self) -> f64 {
        self.length
    }

    /// The unit direction of the fan ray at fan-angle `phi` (radians), in
    /// world coordinates.
    ///
    /// `phi = 0` is the central ray; `phi` opens toward [`LaserPlane::in_plane`].
    /// The direction is `cos phi · central + sin phi · in_plane` — a unit
    /// vector, since `central` and `in_plane` are orthonormal.
    #[must_use]
    pub fn ray_direction(&self, phi: f64) -> Vector3<f64> {
        let (s, c) = phi.sin_cos();
        self.central * c + self.in_plane * s
    }

    /// Distance of a world point from the fan plane (signed by the normal).
    ///
    /// Positive on the `+normal` side. A point lies *in* the fan plane when
    /// this is zero.
    #[must_use]
    pub fn signed_distance(&self, point: &Point3<f64>) -> f64 {
        (point - self.origin).dot(&self.normal)
    }

    /// The fan-angle `phi` of a world point, in radians, measured from the
    /// central ray within the fan plane.
    ///
    /// The point is projected onto the in-plane `(central, in_plane)` basis;
    /// `phi = atan2(in_plane component, central component)`. Points behind the
    /// laser (negative central component) therefore map outside `±π/2`. Any
    /// out-of-plane component is ignored — call [`LaserPlane::signed_distance`]
    /// to test planarity.
    ///
    /// Returns `0.0` for the apex itself (a zero offset has no defined angle),
    /// which keeps the result finite.
    #[must_use]
    pub fn fan_angle_of(&self, point: &Point3<f64>) -> f64 {
        let v = point - self.origin;
        let along = v.dot(&self.central);
        let across = v.dot(&self.in_plane);
        if along.abs() < MIN_AXIS_NORM && across.abs() < MIN_AXIS_NORM {
            return 0.0;
        }
        across.atan2(along)
    }

    /// The radius of a world point — its distance from the apex measured
    /// **within the fan plane** (the out-of-plane component is dropped).
    #[must_use]
    pub fn radius_of(&self, point: &Point3<f64>) -> f64 {
        let v = point - self.origin;
        let along = v.dot(&self.central);
        let across = v.dot(&self.in_plane);
        (along * along + across * across).sqrt()
    }

    /// Whether a world point lies inside the fan's angular *and* radial extent.
    ///
    /// True when the point's [`fan_angle_of`](LaserPlane::fan_angle_of) is
    /// within `±half_angle` (with a small tolerance) and its
    /// [`radius_of`](LaserPlane::radius_of) is within `length`. The
    /// out-of-plane distance is **not** checked here — clip to the plane
    /// first, or test [`signed_distance`](LaserPlane::signed_distance)
    /// separately.
    #[must_use]
    pub fn contains_in_extent(&self, point: &Point3<f64>) -> bool {
        // A relative angular tolerance keeps an exactly-on-edge sample in.
        let angle_eps = 1e-9;
        let radius_eps = 1e-9 * self.length.max(1.0);
        let phi = self.fan_angle_of(point);
        let r = self.radius_of(point);
        phi.abs() <= self.half_angle + angle_eps && r <= self.length + radius_eps
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use nalgebra::{Isometry3, Translation3, UnitQuaternion};
    use std::f64::consts::FRAC_PI_4;

    /// A laser at the origin, identity pose: central ray along world `+z`,
    /// fan in the world `x = 0` plane.
    fn identity_laser(half: f64, length: f64) -> LaserEntity {
        LaserEntity::new(Isometry3::identity(), half, length, 660.0, 0.25e-3).expect("valid laser")
    }

    #[test]
    fn identity_pose_maps_axes_to_world_axes() {
        let plane = LaserPlane::from_entity(&identity_laser(0.3, 1.5));
        assert_relative_eq!(plane.origin(), Point3::origin(), epsilon = 1e-15);
        assert_relative_eq!(plane.central(), Vector3::z(), epsilon = 1e-15);
        assert_relative_eq!(plane.in_plane(), Vector3::y(), epsilon = 1e-15);
        assert_relative_eq!(plane.normal(), Vector3::x(), epsilon = 1e-15);
        assert_relative_eq!(plane.half_angle(), 0.3, epsilon = 1e-15);
        assert_relative_eq!(plane.length(), 1.5, epsilon = 1e-15);
    }

    #[test]
    fn frame_is_right_handed_orthonormal() {
        // A non-trivial pose: rotate and translate, then check the world axes
        // still form a right-handed orthonormal triad.
        let rot = UnitQuaternion::from_euler_angles(0.3, -0.6, 1.1);
        let pose = Isometry3::from_parts(Translation3::new(0.4, -1.0, 0.2), rot);
        let laser = LaserEntity::new(pose, 0.4, 2.0, 660.0, 0.25e-3).unwrap();
        let plane = LaserPlane::from_entity(&laser);
        // Unit length.
        assert_relative_eq!(plane.central().norm(), 1.0, epsilon = 1e-12);
        assert_relative_eq!(plane.in_plane().norm(), 1.0, epsilon = 1e-12);
        assert_relative_eq!(plane.normal().norm(), 1.0, epsilon = 1e-12);
        // Mutually orthogonal.
        assert_relative_eq!(plane.central().dot(&plane.in_plane()), 0.0, epsilon = 1e-12);
        assert_relative_eq!(plane.central().dot(&plane.normal()), 0.0, epsilon = 1e-12);
        assert_relative_eq!(plane.in_plane().dot(&plane.normal()), 0.0, epsilon = 1e-12);
        // Right-handed: the local axes satisfy x = y × z, so the world
        // normal (local +x) equals in_plane (local +y) × central (local +z).
        let cross = plane.in_plane().cross(&plane.central());
        assert_relative_eq!(cross, plane.normal(), epsilon = 1e-12);
    }

    #[test]
    fn ray_direction_spans_the_fan() {
        let plane = LaserPlane::from_entity(&identity_laser(FRAC_PI_4, 1.0));
        // Central ray: phi = 0 -> +z.
        assert_relative_eq!(plane.ray_direction(0.0), Vector3::z(), epsilon = 1e-12);
        // +half_angle (45 deg): direction is (0, sin45, cos45).
        let edge = plane.ray_direction(FRAC_PI_4);
        let s = FRAC_PI_4.sin();
        assert_relative_eq!(edge, Vector3::new(0.0, s, s), epsilon = 1e-12);
        // Every fan direction is a unit vector lying in the fan plane.
        for phi in [-0.5, -0.1, 0.0, 0.2, 0.7] {
            let d = plane.ray_direction(phi);
            assert_relative_eq!(d.norm(), 1.0, epsilon = 1e-12);
            assert_relative_eq!(d.dot(&plane.normal()), 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn signed_distance_is_zero_in_the_plane() {
        let plane = LaserPlane::from_entity(&identity_laser(0.3, 2.0));
        // A point in the y-z plane (x = 0) is in the fan plane.
        let in_plane = Point3::new(0.0, 0.5, 1.0);
        assert_relative_eq!(plane.signed_distance(&in_plane), 0.0, epsilon = 1e-12);
        // A point offset along +x is off the plane by exactly that offset.
        let off = Point3::new(0.7, 0.5, 1.0);
        assert_relative_eq!(plane.signed_distance(&off), 0.7, epsilon = 1e-12);
    }

    #[test]
    fn fan_angle_and_radius_invert_the_parameterisation() {
        let plane = LaserPlane::from_entity(&identity_laser(0.6, 3.0));
        for (phi, r) in [(0.0, 1.0), (0.3, 2.0), (-0.4, 0.5), (0.55, 2.9)] {
            let p = plane.origin() + plane.ray_direction(phi) * r;
            assert_relative_eq!(plane.fan_angle_of(&p), phi, epsilon = 1e-9);
            assert_relative_eq!(plane.radius_of(&p), r, epsilon = 1e-9);
        }
    }

    #[test]
    fn fan_angle_of_the_apex_is_finite() {
        let plane = LaserPlane::from_entity(&identity_laser(0.3, 1.0));
        let phi = plane.fan_angle_of(&plane.origin());
        assert!(phi.is_finite());
        assert_relative_eq!(phi, 0.0, epsilon = 1e-15);
    }

    #[test]
    fn contains_in_extent_bounds_angle_and_radius() {
        let plane = LaserPlane::from_entity(&identity_laser(0.5, 2.0));
        // Inside both bounds.
        let inside = plane.origin() + plane.ray_direction(0.2) * 1.0;
        assert!(plane.contains_in_extent(&inside));
        // Outside the angular extent.
        let wide = plane.origin() + plane.ray_direction(0.9) * 1.0;
        assert!(!plane.contains_in_extent(&wide));
        // Outside the radial extent.
        let far = plane.origin() + plane.ray_direction(0.2) * 3.0;
        assert!(!plane.contains_in_extent(&far));
        // Exactly on each edge counts as inside (tolerance).
        let edge_angle = plane.origin() + plane.ray_direction(0.5) * 1.0;
        assert!(plane.contains_in_extent(&edge_angle));
        let edge_radius = plane.origin() + plane.ray_direction(0.0) * 2.0;
        assert!(plane.contains_in_extent(&edge_radius));
    }
}
