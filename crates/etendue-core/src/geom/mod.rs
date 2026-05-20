//! Pure geometry kernel: meshes and rays.
//!
//! Everything here is `f64`, allocation-conscious, and free of any rendering
//! or UI concerns. The UI crate (`etendue-ui`) converts these types to `f32`
//! GPU buffers at its own boundary; the kernel never sees `f32`.
//!
//! # Contents
//!
//! - [`mesh`] — [`TriMesh`], the triangle-mesh type and its primitive
//!   constructors ([`TriMesh::unit_cube`], [`TriMesh::quad`]).
//! - [`ray`] — [`Ray3`] and a Möller–Trumbore ray–triangle / ray–mesh
//!   intersection, used for CPU picking by later milestones.
//!
//! Later milestones add `plane`, `frustum`, and target primitives here.

pub mod mesh;
pub mod ray;

pub use mesh::TriMesh;
pub use ray::{Ray3, RayHit};
