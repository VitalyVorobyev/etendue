//! Triangle-mesh kernel type.
//!
//! [`TriMesh`] is the pure-geometry mesh representation used throughout
//! `etendue-core`: `f64` vertices, `[u32; 3]` triangle indices, and a
//! per-vertex normal for every vertex. It carries no rendering state — the UI
//! crate packs it down to `f32` GPU buffers at its own boundary.
//!
//! Normals are stored **per vertex**. Primitive constructors that need hard
//! edges (a cube) duplicate the shared corner positions so each face gets its
//! own flat normal; smooth surfaces (added in later milestones) can share
//! vertices and average normals. Keeping a single per-vertex normal field
//! makes the GPU vertex layout uniform regardless of which constructor built
//! the mesh.

use nalgebra::{Point3, Vector3};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// A triangle mesh in `f64` precision.
///
/// Invariants (enforced by [`TriMesh::new`]):
/// - `normals.len() == vertices.len()` — one normal per vertex.
/// - every index in `indices` is `< vertices.len()`.
///
/// The primitive constructors ([`TriMesh::unit_cube`], [`TriMesh::quad`])
/// always satisfy these; [`TriMesh::new`] re-checks them for caller-built
/// meshes.
///
/// # Serde note
///
/// The derived `Deserialize` bypasses [`TriMesh::new`]'s index/normal-count
/// validation. For v0.1.0 this is acceptable — scene JSON is app-produced and
/// is expected to be well-formed. If mesh data arrives from untrusted sources,
/// call [`TriMesh::new`] explicitly after deserialisation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriMesh {
    /// Vertex positions, indexed by `indices`.
    vertices: Vec<Point3<f64>>,
    /// Per-vertex outward normals; `normals[i]` belongs to `vertices[i]`.
    normals: Vec<Vector3<f64>>,
    /// Triangle list: each `[a, b, c]` indexes three entries of `vertices`,
    /// wound counter-clockwise when viewed from outside.
    indices: Vec<[u32; 3]>,
}

impl TriMesh {
    /// Build a mesh from explicit buffers, validating the invariants.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidInput`] if `normals` and `vertices` differ in
    /// length, or if any triangle index is out of bounds.
    pub fn new(
        vertices: Vec<Point3<f64>>,
        normals: Vec<Vector3<f64>>,
        indices: Vec<[u32; 3]>,
    ) -> Result<Self> {
        if normals.len() != vertices.len() {
            return Err(Error::InvalidInput {
                reason: format!(
                    "normal count ({}) must equal vertex count ({})",
                    normals.len(),
                    vertices.len()
                ),
            });
        }
        let vertex_count = vertices.len() as u32;
        for (tri_index, tri) in indices.iter().enumerate() {
            for &vertex_index in tri {
                if vertex_index >= vertex_count {
                    return Err(Error::InvalidInput {
                        reason: format!(
                            "triangle {tri_index} references vertex {vertex_index}, \
                             but the mesh has only {vertex_count} vertices"
                        ),
                    });
                }
            }
        }
        Ok(Self {
            vertices,
            normals,
            indices,
        })
    }

    /// Vertex positions.
    #[must_use]
    pub fn vertices(&self) -> &[Point3<f64>] {
        &self.vertices
    }

    /// Mutable access to vertex positions.
    ///
    /// Allows in-place translation or transformation of vertices without
    /// rebuilding the mesh. The caller is responsible for keeping the vertex
    /// positions geometrically consistent with the stored normals.
    pub fn vertices_mut(&mut self) -> &mut [Point3<f64>] {
        &mut self.vertices
    }

    /// Per-vertex normals, parallel to [`TriMesh::vertices`].
    #[must_use]
    pub fn normals(&self) -> &[Vector3<f64>] {
        &self.normals
    }

    /// Triangle index list.
    #[must_use]
    pub fn indices(&self) -> &[[u32; 3]] {
        &self.indices
    }

    /// Number of triangles in the mesh.
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.indices.len()
    }

    /// Iterate the three corner positions of every triangle.
    ///
    /// Useful for CPU ray-casting against the mesh.
    pub fn triangles(&self) -> impl Iterator<Item = [Point3<f64>; 3]> + '_ {
        self.indices.iter().map(|&[a, b, c]| {
            [
                self.vertices[a as usize],
                self.vertices[b as usize],
                self.vertices[c as usize],
            ]
        })
    }

    /// An axis-aligned cube of the given `edge` length, centred on the origin.
    ///
    /// Each of the six faces has its own four vertices so the face normal is
    /// flat (hard edges). Winding is counter-clockwise seen from outside, so
    /// the outward normal follows the right-hand rule.
    ///
    /// # Panics
    ///
    /// Panics if `edge` is not finite and strictly positive — primitive
    /// constructors take trusted, hard-coded inputs, so a bad value is a
    /// programming error rather than a recoverable one.
    #[must_use]
    pub fn unit_cube(edge: f64) -> Self {
        assert!(
            edge.is_finite() && edge > 0.0,
            "cube edge must be finite and positive, got {edge}"
        );
        let h = edge * 0.5;

        // Six faces, each: outward normal + four corners wound CCW from
        // outside. Corners are listed so [0,1,2] and [0,2,3] tessellate the
        // quad with consistent winding.
        let faces: [(Vector3<f64>, [Point3<f64>; 4]); 6] = [
            // +X
            (
                Vector3::new(1.0, 0.0, 0.0),
                [
                    Point3::new(h, -h, -h),
                    Point3::new(h, h, -h),
                    Point3::new(h, h, h),
                    Point3::new(h, -h, h),
                ],
            ),
            // -X
            (
                Vector3::new(-1.0, 0.0, 0.0),
                [
                    Point3::new(-h, -h, h),
                    Point3::new(-h, h, h),
                    Point3::new(-h, h, -h),
                    Point3::new(-h, -h, -h),
                ],
            ),
            // +Y
            (
                Vector3::new(0.0, 1.0, 0.0),
                [
                    Point3::new(-h, h, -h),
                    Point3::new(-h, h, h),
                    Point3::new(h, h, h),
                    Point3::new(h, h, -h),
                ],
            ),
            // -Y
            (
                Vector3::new(0.0, -1.0, 0.0),
                [
                    Point3::new(-h, -h, h),
                    Point3::new(-h, -h, -h),
                    Point3::new(h, -h, -h),
                    Point3::new(h, -h, h),
                ],
            ),
            // +Z
            (
                Vector3::new(0.0, 0.0, 1.0),
                [
                    Point3::new(-h, -h, h),
                    Point3::new(h, -h, h),
                    Point3::new(h, h, h),
                    Point3::new(-h, h, h),
                ],
            ),
            // -Z
            (
                Vector3::new(0.0, 0.0, -1.0),
                [
                    Point3::new(h, -h, -h),
                    Point3::new(-h, -h, -h),
                    Point3::new(-h, h, -h),
                    Point3::new(h, h, -h),
                ],
            ),
        ];

        let mut vertices = Vec::with_capacity(24);
        let mut normals = Vec::with_capacity(24);
        let mut indices = Vec::with_capacity(12);
        for (normal, corners) in faces {
            let base = vertices.len() as u32;
            vertices.extend_from_slice(&corners);
            normals.extend(std::iter::repeat_n(normal, 4));
            indices.push([base, base + 1, base + 2]);
            indices.push([base, base + 2, base + 3]);
        }

        // Hard-coded primitive: the invariants hold by construction, so the
        // `new` validation cannot fail here.
        Self {
            vertices,
            normals,
            indices,
        }
    }

    /// Append the vertices, normals, and triangles of `other` into `self`.
    ///
    /// The triangle indices in `other` are offset by the current vertex count
    /// of `self` before appending, so every triangle in `other` still
    /// references the correct (re-based) vertices. After the call `self`
    /// contains all geometry from both meshes.
    ///
    /// This is the building block for multi-cube voxel visualisations:
    /// construct one cube per voxel via [`TriMesh::unit_cube`], translate its
    /// vertices to the voxel centre, and merge into an accumulator with
    /// `extend_with`.
    pub fn extend_with(&mut self, other: &TriMesh) {
        let offset = self.vertices.len() as u32;
        self.vertices.extend_from_slice(&other.vertices);
        self.normals.extend_from_slice(&other.normals);
        self.indices.extend(
            other
                .indices
                .iter()
                .map(|&[a, b, c]| [a + offset, b + offset, c + offset]),
        );
    }

    /// A flat rectangular quad of size `width` x `height`, centred on the
    /// origin and lying in the `z = 0` plane, normal pointing toward `+Z`.
    ///
    /// Two triangles, four shared vertices. Used by the M1 demo scene as the
    /// translucent test object and, in later milestones, as the target plane.
    ///
    /// # Panics
    ///
    /// Panics if `width` or `height` is not finite and strictly positive.
    #[must_use]
    pub fn quad(width: f64, height: f64) -> Self {
        assert!(
            width.is_finite() && width > 0.0,
            "quad width must be finite and positive, got {width}"
        );
        assert!(
            height.is_finite() && height > 0.0,
            "quad height must be finite and positive, got {height}"
        );
        let (hw, hh) = (width * 0.5, height * 0.5);
        let vertices = vec![
            Point3::new(-hw, -hh, 0.0),
            Point3::new(hw, -hh, 0.0),
            Point3::new(hw, hh, 0.0),
            Point3::new(-hw, hh, 0.0),
        ];
        let normal = Vector3::new(0.0, 0.0, 1.0);
        let normals = vec![normal; 4];
        let indices = vec![[0, 1, 2], [0, 2, 3]];
        Self {
            vertices,
            normals,
            indices,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn unit_cube_has_expected_topology() {
        let cube = TriMesh::unit_cube(2.0);
        // 6 faces x 4 corners, 6 faces x 2 triangles.
        assert_eq!(cube.vertices().len(), 24);
        assert_eq!(cube.normals().len(), 24);
        assert_eq!(cube.triangle_count(), 12);
    }

    #[test]
    fn unit_cube_vertices_lie_on_the_box() {
        let edge = 3.0;
        let cube = TriMesh::unit_cube(edge);
        let h = edge * 0.5;
        for v in cube.vertices() {
            assert_relative_eq!(v.x.abs(), h, epsilon = 1e-12);
            assert_relative_eq!(v.y.abs(), h, epsilon = 1e-12);
            assert_relative_eq!(v.z.abs(), h, epsilon = 1e-12);
        }
    }

    #[test]
    fn unit_cube_normals_are_unit_and_face_outward() {
        let cube = TriMesh::unit_cube(2.0);
        for tri in 0..cube.triangle_count() {
            let [a, b, c] = cube.indices()[tri];
            let pa = cube.vertices()[a as usize];
            let pb = cube.vertices()[b as usize];
            let pc = cube.vertices()[c as usize];
            // Geometric normal from CCW winding.
            let geo = (pb - pa).cross(&(pc - pa)).normalize();
            let stored = cube.normals()[a as usize];
            assert_relative_eq!(stored.norm(), 1.0, epsilon = 1e-12);
            // Stored per-vertex normal must agree with the winding normal.
            assert_relative_eq!(geo.dot(&stored), 1.0, epsilon = 1e-9);
            // And it must point away from the cube centre (the origin).
            let centroid = Point3::from((pa.coords + pb.coords + pc.coords) / 3.0);
            assert!(
                centroid.coords.dot(&stored) > 0.0,
                "triangle {tri} normal points inward"
            );
        }
    }

    #[test]
    fn quad_is_two_triangles_in_the_z_plane() {
        let q = TriMesh::quad(4.0, 2.0);
        assert_eq!(q.vertices().len(), 4);
        assert_eq!(q.triangle_count(), 2);
        for v in q.vertices() {
            assert_relative_eq!(v.z, 0.0, epsilon = 1e-12);
        }
        for n in q.normals() {
            assert_relative_eq!(n.z, 1.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn new_rejects_normal_count_mismatch() {
        let vertices = vec![Point3::origin(); 3];
        let normals = vec![Vector3::z(); 2];
        let indices = vec![[0, 1, 2]];
        let err = TriMesh::new(vertices, normals, indices).unwrap_err();
        assert!(matches!(err, Error::InvalidInput { .. }));
    }

    #[test]
    fn new_rejects_out_of_bounds_index() {
        let vertices = vec![Point3::origin(); 3];
        let normals = vec![Vector3::z(); 3];
        let indices = vec![[0, 1, 9]];
        let err = TriMesh::new(vertices, normals, indices).unwrap_err();
        assert!(matches!(err, Error::InvalidInput { .. }));
    }

    #[test]
    fn new_accepts_a_valid_mesh() {
        let vertices = vec![
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
        ];
        let normals = vec![Vector3::z(); 3];
        let indices = vec![[0, 1, 2]];
        let mesh = TriMesh::new(vertices, normals, indices).unwrap();
        assert_eq!(mesh.triangle_count(), 1);
        let tri: Vec<_> = mesh.triangles().collect();
        assert_eq!(tri.len(), 1);
    }

    #[test]
    fn extend_with_doubles_the_mesh_size() {
        // Two quads merged: vertex, triangle, and normal counts must double.
        let q = TriMesh::quad(1.0, 1.0);
        let mut combined = q.clone();
        combined.extend_with(&q);
        assert_eq!(combined.vertices().len(), q.vertices().len() * 2);
        assert_eq!(combined.normals().len(), q.normals().len() * 2);
        assert_eq!(combined.triangle_count(), q.triangle_count() * 2);
    }

    #[test]
    fn extend_with_rebases_indices_correctly() {
        // Two quads merged: the second quad's indices must reference the
        // second block of vertices, not overlap with the first.
        let q = TriMesh::quad(1.0, 1.0);
        let n = q.vertices().len() as u32;
        let mut combined = q.clone();
        combined.extend_with(&q);
        for &[a, b, c] in &combined.indices()[q.triangle_count()..] {
            assert!(
                a >= n && b >= n && c >= n,
                "second block indices must be offset"
            );
        }
    }

    #[test]
    fn vertices_mut_allows_in_place_translation() {
        let mut mesh = TriMesh::quad(1.0, 1.0);
        for v in mesh.vertices_mut() {
            v.z += 5.0;
        }
        for v in mesh.vertices() {
            assert_relative_eq!(v.z, 5.0, epsilon = 1e-12);
        }
    }
}
