//! Pure geometry kernel: triangle meshes.
//!
//! Everything here is `f64`, allocation-conscious, and free of any rendering
//! or UI concerns. The UI crate (`etendue-ui`) converts these types to `f32`
//! GPU buffers at its own boundary; the kernel never sees `f32`.
//!
//! # Contents
//!
//! - [`mesh`] — [`TriMesh`], the triangle-mesh type and its primitive
//!   constructors ([`TriMesh::unit_cube`], [`TriMesh::quad`]).

pub mod mesh;

pub use mesh::TriMesh;
