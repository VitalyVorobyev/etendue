//! Rays and ray–triangle intersection.
//!
//! [`Ray3`] is a parametric half-line `origin + t * direction`, `t >= 0`.
//! [`Ray3::intersect_triangle`] is a Möller–Trumbore intersection test —
//! a small, allocation-free kernel primitive.
//!
//! M1 only needs the primitive itself; later milestones use it for CPU
//! click-picking (`etendue-ui` ray-casts the camera ray against scene meshes).

use nalgebra::{Point3, Vector3};

use crate::geom::mesh::TriMesh;

/// Rays whose direction norm is below this are treated as degenerate.
const MIN_DIRECTION_NORM: f64 = 1e-12;

/// A parametric ray `origin + t * direction` for `t >= 0`.
///
/// The direction is **not** required to be unit length; [`Ray3::point_at`]
/// scales by `t` directly. [`Ray3::new`] only rejects a (near-)zero direction,
/// which would make the ray degenerate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray3 {
    /// The point the ray starts from (`t = 0`).
    pub origin: Point3<f64>,
    /// The ray's direction. Need not be normalized.
    pub direction: Vector3<f64>,
}

/// A successful ray–triangle hit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayHit {
    /// Ray parameter `t` at the intersection: `t >= 0`, and the hit point is
    /// `ray.origin + t * ray.direction`.
    pub t: f64,
    /// The intersection point in world space.
    pub point: Point3<f64>,
    /// Barycentric coordinates `(u, v)` of the hit within the triangle; the
    /// third coordinate is `1 - u - v`. Both lie in `[0, 1]` with `u + v <= 1`.
    pub bary: (f64, f64),
}

impl Ray3 {
    /// Build a ray, returning `None` if `direction` is (near-)zero.
    ///
    /// A zero direction has no meaningful intersection behaviour, so it is
    /// rejected at construction rather than silently producing NaNs later.
    #[must_use]
    pub fn new(origin: Point3<f64>, direction: Vector3<f64>) -> Option<Self> {
        if direction.norm() < MIN_DIRECTION_NORM {
            None
        } else {
            Some(Self { origin, direction })
        }
    }

    /// The point at ray parameter `t`: `origin + t * direction`.
    #[must_use]
    pub fn point_at(&self, t: f64) -> Point3<f64> {
        self.origin + self.direction * t
    }

    /// Möller–Trumbore intersection of this ray with a single triangle.
    ///
    /// Returns the hit nearest the origin with `t >= 0`, or `None` for a miss,
    /// a ray parallel to the triangle plane, or a hit strictly behind the
    /// origin. The triangle is treated as **double-sided** — a hit from either
    /// face counts.
    ///
    /// # Algorithm
    ///
    /// With edges `e1 = b - a`, `e2 = c - a`, the system
    /// `origin + t·d = a + u·e1 + v·e2` is solved by Cramer's rule. The
    /// determinant `det = (d × e2) · e1` vanishes when `d` is parallel to the
    /// triangle plane; the barycentric coordinates `u, v` and `1 - u - v` must
    /// all be non-negative for the point to lie inside the triangle.
    #[must_use]
    pub fn intersect_triangle(
        &self,
        a: Point3<f64>,
        b: Point3<f64>,
        c: Point3<f64>,
    ) -> Option<RayHit> {
        // A scale-relative epsilon keeps the parallel/inside tests stable for
        // both tiny and large triangles.
        let e1 = b - a;
        let e2 = c - a;
        let scale = e1.norm().max(e2.norm()).max(1.0);
        let eps = 1e-12 * scale;

        let pvec = self.direction.cross(&e2);
        let det = e1.dot(&pvec);
        // Parallel ray (or degenerate triangle): no single intersection.
        if det.abs() < eps {
            return None;
        }
        let inv_det = 1.0 / det;

        let tvec = self.origin - a;
        let u = tvec.dot(&pvec) * inv_det;
        if u < -eps || u > 1.0 + eps {
            return None;
        }

        let qvec = tvec.cross(&e1);
        let v = self.direction.dot(&qvec) * inv_det;
        if v < -eps || u + v > 1.0 + eps {
            return None;
        }

        let t = e2.dot(&qvec) * inv_det;
        // Reject intersections behind the ray origin.
        if t < -eps {
            return None;
        }

        Some(RayHit {
            t,
            point: self.point_at(t),
            bary: (u, v),
        })
    }

    /// Intersect this ray with every triangle of a mesh, returning the hit
    /// nearest the origin (smallest `t >= 0`), or `None` if the ray misses
    /// all triangles.
    ///
    /// Linear scan over triangles — adequate for the handful of scene meshes
    /// `etendue` renders; an acceleration structure is unnecessary at this
    /// scale.
    #[must_use]
    pub fn intersect_mesh(&self, mesh: &TriMesh) -> Option<RayHit> {
        mesh.triangles()
            .filter_map(|[a, b, c]| self.intersect_triangle(a, b, c))
            .min_by(|x, y| x.t.partial_cmp(&y.t).unwrap_or(std::cmp::Ordering::Equal))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    /// A unit right triangle in the `z = 0` plane: corners at the origin and
    /// the +X / +Y unit points.
    fn unit_triangle() -> (Point3<f64>, Point3<f64>, Point3<f64>) {
        (
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
        )
    }

    #[test]
    fn hit_through_the_centre() {
        let (a, b, c) = unit_triangle();
        // Fire from +Z straight down through the triangle's interior.
        let ray = Ray3::new(Point3::new(0.25, 0.25, 5.0), Vector3::new(0.0, 0.0, -1.0)).unwrap();
        let hit = ray.intersect_triangle(a, b, c).expect("ray should hit");
        assert_relative_eq!(hit.t, 5.0, epsilon = 1e-9);
        assert_relative_eq!(hit.point.x, 0.25, epsilon = 1e-9);
        assert_relative_eq!(hit.point.y, 0.25, epsilon = 1e-9);
        assert_relative_eq!(hit.point.z, 0.0, epsilon = 1e-9);
        // Barycentric coords of (0.25, 0.25): u = 0.25, v = 0.25.
        assert_relative_eq!(hit.bary.0, 0.25, epsilon = 1e-9);
        assert_relative_eq!(hit.bary.1, 0.25, epsilon = 1e-9);
    }

    #[test]
    fn miss_outside_the_triangle() {
        let (a, b, c) = unit_triangle();
        // The point (0.9, 0.9) is in the triangle's plane but outside it
        // (u + v = 1.8 > 1).
        let ray = Ray3::new(Point3::new(0.9, 0.9, 5.0), Vector3::new(0.0, 0.0, -1.0)).unwrap();
        assert!(ray.intersect_triangle(a, b, c).is_none());
    }

    #[test]
    fn miss_to_the_side() {
        let (a, b, c) = unit_triangle();
        // Clearly off the triangle entirely.
        let ray = Ray3::new(Point3::new(-3.0, -3.0, 5.0), Vector3::new(0.0, 0.0, -1.0)).unwrap();
        assert!(ray.intersect_triangle(a, b, c).is_none());
    }

    #[test]
    fn parallel_ray_does_not_intersect() {
        let (a, b, c) = unit_triangle();
        // A ray travelling in +X stays in the z = 0 plane: parallel to the
        // triangle, det == 0, no single intersection.
        let ray = Ray3::new(Point3::new(-1.0, 0.25, 0.0), Vector3::new(1.0, 0.0, 0.0)).unwrap();
        assert!(ray.intersect_triangle(a, b, c).is_none());
    }

    #[test]
    fn intersection_behind_the_origin_is_rejected() {
        let (a, b, c) = unit_triangle();
        // Origin below the triangle, pointing further down (-Z): the triangle
        // is behind the ray, t would be negative.
        let ray = Ray3::new(Point3::new(0.25, 0.25, -5.0), Vector3::new(0.0, 0.0, -1.0)).unwrap();
        assert!(ray.intersect_triangle(a, b, c).is_none());
    }

    #[test]
    fn hit_from_the_back_face_still_counts() {
        let (a, b, c) = unit_triangle();
        // Fire upward from -Z: hits the back face. Möller–Trumbore here is
        // double-sided, so this is a valid hit.
        let ray = Ray3::new(Point3::new(0.25, 0.25, -5.0), Vector3::new(0.0, 0.0, 1.0)).unwrap();
        let hit = ray.intersect_triangle(a, b, c).expect("back-face hit");
        assert_relative_eq!(hit.t, 5.0, epsilon = 1e-9);
    }

    #[test]
    fn non_unit_direction_scales_t_consistently() {
        let (a, b, c) = unit_triangle();
        // Direction of length 2: the hit at z = 0 is reached at t = 2.5.
        let ray = Ray3::new(Point3::new(0.25, 0.25, 5.0), Vector3::new(0.0, 0.0, -2.0)).unwrap();
        let hit = ray.intersect_triangle(a, b, c).expect("ray should hit");
        assert_relative_eq!(hit.t, 2.5, epsilon = 1e-9);
        // point_at(t) must still land on the triangle plane.
        assert_relative_eq!(hit.point.z, 0.0, epsilon = 1e-9);
    }

    #[test]
    fn zero_direction_ray_is_rejected() {
        assert!(Ray3::new(Point3::origin(), Vector3::zeros()).is_none());
    }

    #[test]
    fn intersect_mesh_returns_nearest_hit() {
        // A cube of edge 2 centred on the origin spans z in [-1, 1]. A ray
        // from +Z pointing down must hit the +Z face first (t smaller than
        // the -Z face).
        let cube = TriMesh::unit_cube(2.0);
        let ray = Ray3::new(Point3::new(0.0, 0.0, 10.0), Vector3::new(0.0, 0.0, -1.0)).unwrap();
        let hit = ray.intersect_mesh(&cube).expect("ray should hit the cube");
        // Nearest face is z = +1, so t = 10 - 1 = 9.
        assert_relative_eq!(hit.t, 9.0, epsilon = 1e-9);
        assert_relative_eq!(hit.point.z, 1.0, epsilon = 1e-9);
    }

    #[test]
    fn intersect_mesh_misses_cleanly() {
        let cube = TriMesh::unit_cube(2.0);
        let ray = Ray3::new(
            Point3::new(100.0, 100.0, 10.0),
            Vector3::new(0.0, 0.0, -1.0),
        )
        .unwrap();
        assert!(ray.intersect_mesh(&cube).is_none());
    }
}
