//! The hand-written 3D viewport that renders *behind* the egui UI.
//!
//! The viewport is `etendue-ui`'s own wgpu render stack — deliberately not
//! `eframe` and not egui's painter — because the optical-design scene needs a
//! depth buffer, an opaque pass, and a separate alpha-blended translucent
//! pass, all interleaved with the egui frame.
//!
//! # Frame structure
//!
//! Each frame the application records, into one command encoder and the
//! surface texture:
//!
//! 1. the 3D pass ([`Renderer::render`]) — clears color + depth, draws the
//!    opaque geometry (grid, axes, flat-shaded meshes, wireframe edges,
//!    points), then the translucent geometry (alpha-blended, depth-write off);
//! 2. the egui pass — *loads* (does not clear) the surface and paints the UI
//!    on top.
//!
//! # Modules
//!
//! - [`camera`] — [`OrbitCamera`], the user's orbit/pan/zoom viewpoint. This
//!   is the viewer's camera, not a simulated optical sensor.
//! - [`mesh`] — the `f64` -> `f32` GPU boundary: `Pod`/`Zeroable` vertex
//!   structs and the buffer builders that pack kernel [`TriMesh`]es into them.
//! - [`scene`] — the bridge from `etendue-core` scene entities (camera, laser,
//!   target) to renderable geometry: frustum edges, the laser fan mesh, the
//!   target quad.
//! - [`heatmap`] — the M4 defocus heatmap: the depth-of-field colormap and the
//!   tessellated per-vertex-colored grid that paints a `DefocusMap` onto the
//!   target quad.
//! - [`renderer`] — [`Renderer`], the wgpu pipelines, depth texture, and the
//!   per-pass draw recording.
//!
//! [`TriMesh`]: etendue_core::geom::TriMesh

pub mod camera;
pub mod heatmap;
pub mod mesh;
pub mod renderer;
pub mod scene;

pub use camera::OrbitCamera;
pub use renderer::Renderer;
