//! The `f64` -> `f32` GPU boundary.
//!
//! `etendue-core` geometry is `f64`; GPUs want `f32`. This module is the one
//! place that conversion happens. It defines:
//!
//! - the `Pod`/`Zeroable` GPU vertex structs ([`MeshVertex`], [`LineVertex`])
//!   and their [`wgpu::VertexBufferLayout`]s,
//! - [`GpuMesh`] / [`GpuLines`] / [`GpuPoints`] — uploaded vertex (+ index)
//!   buffers ready to draw,
//! - builders that pack a kernel [`TriMesh`] or a list of `f64` segments into
//!   those GPU buffers.
//!
//! Nothing here pushes `f32` back into the kernel — the conversion is strictly
//! one-way, at upload time.

use bytemuck::{Pod, Zeroable};
use etendue_core::geom::TriMesh;
use nalgebra::Point3;
use wgpu::util::DeviceExt;

/// A lit-mesh GPU vertex: position, normal, color — all `f32`.
///
/// `#[repr(C)]` + `Pod`/`Zeroable` make it safe to reinterpret a `&[MeshVertex]`
/// as bytes for [`wgpu::util::DeviceExt::create_buffer_init`].
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct MeshVertex {
    /// Model-space position.
    pub position: [f32; 3],
    /// Model-space normal (per-vertex).
    pub normal: [f32; 3],
    /// Per-vertex RGB color, multiplied by the per-draw tint in the shader.
    pub color: [f32; 3],
}

impl MeshVertex {
    /// `wgpu` vertex attributes: `@location(0..=2)` = position, normal, color.
    const ATTRS: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3];

    /// The vertex buffer layout for the mesh pipelines.
    #[must_use]
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRS,
        }
    }
}

/// An unlit colored GPU vertex for line and point primitives: position +
/// color, `f32`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct LineVertex {
    /// Model-space position.
    pub position: [f32; 3],
    /// Per-vertex RGB color.
    pub color: [f32; 3],
}

impl LineVertex {
    /// `wgpu` vertex attributes: `@location(0..=1)` = position, color.
    const ATTRS: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];

    /// The vertex buffer layout for the line/point pipelines.
    #[must_use]
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRS,
        }
    }

    /// Construct a vertex, narrowing an `f64` kernel point to `f32`.
    #[must_use]
    pub fn from_point(p: Point3<f64>, color: [f32; 3]) -> Self {
        Self {
            position: [p.x as f32, p.y as f32, p.z as f32],
            color,
        }
    }
}

/// An indexed triangle mesh resident on the GPU, ready for the mesh pipelines.
#[derive(Debug)]
pub struct GpuMesh {
    /// `MeshVertex` vertex buffer.
    pub vertices: wgpu::Buffer,
    /// `u32` index buffer.
    pub indices: wgpu::Buffer,
    /// Number of indices to draw (`3 * triangle_count`).
    pub index_count: u32,
}

impl GpuMesh {
    /// Pack a kernel [`TriMesh`] into GPU buffers, narrowing `f64` -> `f32`.
    ///
    /// Every vertex gets the same flat `color`; the per-draw tint (uploaded
    /// separately) modulates it. The kernel mesh is untouched and stays `f64`.
    #[must_use]
    pub fn from_trimesh(device: &wgpu::Device, mesh: &TriMesh, color: [f32; 3]) -> Self {
        let vertices: Vec<MeshVertex> = mesh
            .vertices()
            .iter()
            .zip(mesh.normals())
            .map(|(p, n)| MeshVertex {
                position: [p.x as f32, p.y as f32, p.z as f32],
                normal: [n.x as f32, n.y as f32, n.z as f32],
                color,
            })
            .collect();

        // Flatten [[u32; 3]] triangle list to a flat u32 index buffer.
        let indices: Vec<u32> = mesh.indices().iter().flatten().copied().collect();

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("etendue-mesh-vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("etendue-mesh-indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            vertices: vertex_buffer,
            indices: index_buffer,
            index_count: indices.len() as u32,
        }
    }
}

/// An indexed **per-vertex-colored** triangle mesh resident on the GPU.
///
/// Where [`GpuMesh`] carries lit [`MeshVertex`]es with a single flat color,
/// this carries unlit [`LineVertex`]es — position + a per-vertex RGB color —
/// drawn as a `TriangleList`. It is the M4 defocus-heatmap geometry: a
/// tessellated grid over the target quad whose vertex colors encode the
/// circle-of-confusion field through a colormap. Using the unlit
/// `LineVertex`/`line.wgsl` path keeps the colormap colors faithful (no
/// Lambert shading darkening the ramp) and adds no new shader.
#[derive(Debug)]
pub struct GpuColoredMesh {
    /// `LineVertex` vertex buffer (position + per-vertex color).
    pub vertices: wgpu::Buffer,
    /// `u32` index buffer.
    pub indices: wgpu::Buffer,
    /// Number of indices to draw (`3 * triangle_count`).
    pub index_count: u32,
}

impl GpuColoredMesh {
    /// Upload a per-vertex-colored indexed triangle mesh.
    ///
    /// `vertices` are already-built [`LineVertex`]es (position narrowed to
    /// `f32`, RGB color); `indices` is a flat `u32` triangle list (length a
    /// multiple of three). An empty mesh yields zero-length buffers that draw
    /// nothing.
    #[must_use]
    pub fn from_colored_vertices(
        device: &wgpu::Device,
        vertices: &[LineVertex],
        indices: &[u32],
    ) -> Self {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("etendue-colored-mesh-vertices"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("etendue-colored-mesh-indices"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        Self {
            vertices: vertex_buffer,
            indices: index_buffer,
            index_count: indices.len() as u32,
        }
    }
}

/// A line-list primitive resident on the GPU.
///
/// Generic carrier for axes, grids, wireframe edges, and later curve overlays:
/// vertices are interpreted pairwise as independent segments.
#[derive(Debug)]
pub struct GpuLines {
    /// `LineVertex` vertex buffer.
    pub vertices: wgpu::Buffer,
    /// Number of vertices (`2 * segment_count`).
    pub vertex_count: u32,
}

impl GpuLines {
    /// Upload an already-built `LineVertex` slice as a line-list buffer.
    ///
    /// The slice length should be even (pairs of endpoints); an empty slice
    /// yields a zero-vertex buffer that draws nothing.
    #[must_use]
    pub fn from_vertices(device: &wgpu::Device, verts: &[LineVertex]) -> Self {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("etendue-line-vertices"),
            contents: bytemuck::cast_slice(verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        Self {
            vertices: buffer,
            vertex_count: verts.len() as u32,
        }
    }

    /// Build a line-list from `f64` endpoint pairs, narrowing to `f32`.
    ///
    /// Each `(start, end)` becomes one segment with the given per-segment
    /// color.
    #[must_use]
    pub fn from_segments(
        device: &wgpu::Device,
        segments: &[(Point3<f64>, Point3<f64>, [f32; 3])],
    ) -> Self {
        let mut verts = Vec::with_capacity(segments.len() * 2);
        for &(start, end, color) in segments {
            verts.push(LineVertex::from_point(start, color));
            verts.push(LineVertex::from_point(end, color));
        }
        Self::from_vertices(device, &verts)
    }
}

/// A point-list primitive resident on the GPU.
#[derive(Debug)]
pub struct GpuPoints {
    /// `LineVertex` vertex buffer (point primitives reuse the unlit layout).
    pub vertices: wgpu::Buffer,
    /// Number of point vertices.
    pub vertex_count: u32,
}

impl GpuPoints {
    /// Build a point-list from `f64` positions, narrowing to `f32`.
    #[must_use]
    pub fn from_points(device: &wgpu::Device, points: &[(Point3<f64>, [f32; 3])]) -> Self {
        let verts: Vec<LineVertex> = points
            .iter()
            .map(|&(p, color)| LineVertex::from_point(p, color))
            .collect();
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("etendue-point-vertices"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        Self {
            vertices: buffer,
            vertex_count: verts.len() as u32,
        }
    }
}
