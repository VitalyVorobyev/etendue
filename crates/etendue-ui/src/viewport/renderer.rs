//! The hand-written wgpu 3D renderer that draws *behind* the egui UI.
//!
//! # Frame structure
//!
//! Each frame the application calls [`Renderer::render`], which records, into
//! a caller-supplied encoder and color view:
//!
//! 1. one render pass that **clears** color + depth, then runs the **opaque**
//!    pass — flat-shaded meshes, the ground grid, world axes, wireframe edges,
//!    and points, all with depth-test **and** depth-write enabled;
//! 2. the **translucent** pass, in the same render pass, *after* the opaque
//!    geometry: alpha-blended meshes with depth-test on but depth-write
//!    **off**, so translucent surfaces are occluded by opaque geometry yet do
//!    not occlude each other or block the depth buffer.
//!
//! The egui pass runs afterwards (the application owns that), painting the UI
//! on top of the finished 3D image. The 3D renderer and egui share the
//! surface texture but use separate render passes and separate encoders'
//! command streams within the same submission.
//!
//! # Translucency ordering
//!
//! The translucent pass is depth-tested but does not write depth, so
//! alpha-blended surfaces are correctly occluded by opaque geometry yet do not
//! occlude one another. With more than one translucent drawable the blend
//! result still depends on draw order, so the translucent drawables are sorted
//! **back-to-front** by the distance from their world-space centroid to the
//! view camera before they are recorded. With M8 camera-anatomy quads,
//! the M6 working-volume patch, and the M10 voxel-overlap cloud, there are
//! now several translucent drawables; the sort ensures correct blending.
//!
//! # Pipelines
//!
//! Five [`wgpu::RenderPipeline`]s, two shaders:
//!
//! | pipeline            | shader      | topology   | blend          | depth write |
//! |---------------------|-------------|------------|----------------|-------------|
//! | `mesh_opaque`       | `mesh.wgsl` | triangles  | replace        | yes         |
//! | `mesh_translucent`  | `mesh.wgsl` | triangles  | alpha blend    | no          |
//! | `mesh_vertex_color` | `line.wgsl` | triangles  | replace        | yes         |
//! | `lines`             | `line.wgsl` | line-list  | replace        | yes         |
//! | `points`            | `line.wgsl` | point-list | replace        | yes         |
//!
//! Wireframes use the `lines` pipeline with explicit edge geometry — wgpu's
//! Metal backend has no `PolygonMode::Line`. The `mesh_vertex_color` pipeline
//! is the M4 defocus-heatmap path: unlit per-vertex-colored triangles (the
//! `line.wgsl` shader applied to a `TriangleList`), so the heatmap colormap is
//! shown faithfully without Lambert shading darkening the ramp.

use bytemuck::{Pod, Zeroable};
use nalgebra::{Matrix4, Point3};
use wgpu::util::DeviceExt;

use etendue_core::analysis::{DefocusMap, VoxelOverlap, WorkingVolume};

use crate::viewport::camera::OrbitCamera;
use crate::viewport::mesh::{GpuColoredMesh, GpuLines, GpuMesh, GpuPoints, LineVertex, MeshVertex};

/// Depth texture format. `Depth32Float` is universally supported and avoids
/// stencil bookkeeping the viewport does not need.
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Background color the 3D pass clears the surface to.
const CLEAR_COLOR: wgpu::Color = wgpu::Color {
    r: 0.043,
    g: 0.051,
    b: 0.067,
    a: 1.0,
};

/// Per-frame camera + lighting uniform — bind group 0.
///
/// Mirrors `Globals` in both WGSL shaders. `#[repr(C)]` + `Pod` make it
/// byte-uploadable; every field is 16-byte aligned (a `mat4x4` and two
/// `vec4`s), satisfying `std140`-style uniform layout rules without padding.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct GlobalsUniform {
    /// Combined view-projection matrix, column-major.
    view_proj: [[f32; 4]; 4],
    /// World-space direction toward the light; `w` unused.
    light_dir: [f32; 4],
    /// World-space eye position; `w` unused.
    eye: [f32; 4],
}

impl GlobalsUniform {
    /// Build the uniform from the orbit camera and the fixed scene light.
    fn from_camera(camera: &OrbitCamera, aspect: f32) -> Self {
        let eye = camera.eye();
        // A fixed directional light coming from above/front-right; the value
        // is a direction, normalized in the shader.
        let light_dir = [0.4, 0.5, 0.75, 0.0];
        Self {
            view_proj: matrix_to_cols(camera.view_proj(aspect)),
            light_dir,
            eye: [eye.x, eye.y, eye.z, 1.0],
        }
    }
}

/// Per-draw model uniform — bind group 1, one per [`Drawable`].
///
/// Mirrors `Model` in both shaders. The `normal` matrix is the inverse-
/// transpose of the model's upper-left 3x3, stored in a `mat4x4` slot for
/// alignment; line/point shaders ignore it.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct ModelUniform {
    /// Model -> world transform, column-major.
    model: [[f32; 4]; 4],
    /// Inverse-transpose of the model, column-major.
    normal: [[f32; 4]; 4],
    /// Flat RGBA tint; `a < 1` routes a mesh through the translucent pass.
    tint: [f32; 4],
}

impl ModelUniform {
    /// Build a model uniform from a transform and an RGBA tint.
    ///
    /// The normal matrix is the inverse-transpose so non-uniform scale (none
    /// in M1, but cheap insurance for M2 entities) still yields correct
    /// normals; if the model is not invertible it falls back to the identity.
    fn new(model: Matrix4<f32>, tint: [f32; 4]) -> Self {
        let normal = model
            .try_inverse()
            .map_or_else(Matrix4::identity, |inv| inv.transpose());
        Self {
            model: matrix_to_cols(model),
            normal: matrix_to_cols(normal),
            tint,
        }
    }
}

/// Convert an `nalgebra` column-major `Matrix4` to a `[[f32; 4]; 4]` whose
/// outer index is the column — the layout WGSL `mat4x4` expects.
fn matrix_to_cols(m: Matrix4<f32>) -> [[f32; 4]; 4] {
    let mut cols = [[0.0_f32; 4]; 4];
    for (c, col) in cols.iter_mut().enumerate() {
        for (r, slot) in col.iter_mut().enumerate() {
            *slot = m[(r, c)];
        }
    }
    cols
}

/// Which translucency class a drawable belongs to.
///
/// Determines the pass it is recorded in and whether its depth writes are
/// retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pass {
    /// Drawn in the opaque pass; writes depth.
    Opaque,
    /// Drawn in the translucent pass after all opaque geometry; depth-tested
    /// but does not write depth.
    Translucent,
}

/// The kind of GPU geometry a [`Drawable`] holds.
enum Geometry {
    /// Indexed flat-colored, lit triangle mesh.
    Mesh(GpuMesh),
    /// Indexed per-vertex-colored, unlit triangle mesh (the M4 heatmap grid).
    ColoredMesh(GpuColoredMesh),
    /// Line-list segments (axes, grid, wireframe edges).
    Lines(GpuLines),
    /// Point-list markers.
    Points(GpuPoints),
}

/// One renderable scene item: GPU geometry plus its own model bind group.
///
/// The model uniform is uploaded once at construction via `Drawable::new`.
/// All scene mutations go through the full [`Renderer::rebuild_scene`] path
/// which replaces the entire drawable list.
pub struct Drawable {
    /// The geometry buffers.
    geometry: Geometry,
    /// Bind group 1 wrapping the model uniform buffer.
    model_bind_group: wgpu::BindGroup,
    /// Opaque vs. translucent — selects the pass.
    pass: Pass,
    /// World-space centroid, used to sort translucent drawables back-to-front.
    /// Opaque drawables do not need it; it is the model matrix applied to the
    /// geometry's local centroid at construction time.
    centroid: Point3<f32>,
}

impl Drawable {
    /// Internal constructor: upload the model uniform and build its bind
    /// group, then pair it with `geometry`.
    ///
    /// `local_centroid` is the geometry's centroid in its own local frame; the
    /// `model` matrix is applied to it to obtain the world-space centroid used
    /// for translucency depth sorting.
    fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        geometry: Geometry,
        model: Matrix4<f32>,
        tint: [f32; 4],
        pass: Pass,
        local_centroid: Point3<f32>,
    ) -> Self {
        let uniform = ModelUniform::new(model, tint);
        let model_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("etendue-model-uniform"),
            contents: bytemuck::bytes_of(&uniform),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let model_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("etendue-model-bind-group"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: model_buffer.as_entire_binding(),
            }],
        });
        let centroid = model.transform_point(&local_centroid);
        Self {
            geometry,
            model_bind_group,
            pass,
            centroid,
        }
    }
}

/// The 3D viewport renderer: pipelines, the camera uniform, the depth texture,
/// and the list of drawables.
pub struct Renderer {
    /// Bind group 1 layout (a per-draw `ModelUniform`).
    model_layout: wgpu::BindGroupLayout,
    /// GPU buffer for the per-frame `GlobalsUniform`.
    globals_buffer: wgpu::Buffer,
    /// Bind group 0 wrapping `globals_buffer`.
    globals_bind_group: wgpu::BindGroup,
    /// Triangle pipeline, opaque variant (depth write on, blend replace).
    mesh_opaque: wgpu::RenderPipeline,
    /// Triangle pipeline, translucent variant (depth write off, alpha blend).
    mesh_translucent: wgpu::RenderPipeline,
    /// Triangle pipeline, unlit per-vertex-color variant — the M4 defocus
    /// heatmap grid (depth write on, blend replace).
    mesh_vertex_color: wgpu::RenderPipeline,
    /// Line-list pipeline (axes, grid, wireframes).
    lines: wgpu::RenderPipeline,
    /// Point-list pipeline.
    points: wgpu::RenderPipeline,
    /// Depth texture view; recreated by [`Renderer::resize`].
    depth_view: wgpu::TextureView,
    /// Every scene drawable; iterated per pass each frame.
    drawables: Vec<Drawable>,
}

impl Renderer {
    /// Build the renderer for a surface of `surface_format`, sized
    /// `width` x `height` physical pixels, populated from `scene`.
    ///
    /// Compiles both shaders, builds the four pipelines, allocates the camera
    /// uniform + depth texture, and turns every entity in `scene` into a
    /// drawable (plus the ground grid and world axes).
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        scene: &etendue_core::Scene,
    ) -> Self {
        // --- Bind group layouts ------------------------------------------
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("etendue-globals-layout"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let model_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("etendue-model-layout"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });

        // --- Camera/lighting uniform (bind group 0) ----------------------
        // Initialized zeroed; the real values are written every frame before
        // the pass, so the starting contents do not matter.
        let globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("etendue-globals-uniform"),
            contents: bytemuck::bytes_of(&GlobalsUniform::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("etendue-globals-bind-group"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });

        // --- Shaders ------------------------------------------------------
        let mesh_shader = device.create_shader_module(wgpu::include_wgsl!("shaders/mesh.wgsl"));
        let line_shader = device.create_shader_module(wgpu::include_wgsl!("shaders/line.wgsl"));

        // Both pipeline families share this layout: group 0 = globals,
        // group 1 = per-draw model.
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("etendue-pipeline-layout"),
            bind_group_layouts: &[Some(&globals_layout), Some(&model_layout)],
            // No `var<immediate>` push-constant data in any viewport shader.
            immediate_size: 0,
        });

        let mesh_opaque = build_pipeline(
            device,
            &pipeline_layout,
            &mesh_shader,
            "etendue-mesh-opaque",
            &[MeshVertex::layout()],
            surface_format,
            wgpu::PrimitiveTopology::TriangleList,
            Blend::Replace,
            DepthWrite::On,
        );
        let mesh_translucent = build_pipeline(
            device,
            &pipeline_layout,
            &mesh_shader,
            "etendue-mesh-translucent",
            &[MeshVertex::layout()],
            surface_format,
            wgpu::PrimitiveTopology::TriangleList,
            Blend::AlphaBlend,
            DepthWrite::Off,
        );
        // The M4 heatmap path: the unlit `line.wgsl` shader (position + color)
        // applied to a TriangleList, so per-vertex colors are shown verbatim.
        let mesh_vertex_color = build_pipeline(
            device,
            &pipeline_layout,
            &line_shader,
            "etendue-mesh-vertex-color",
            &[LineVertex::layout()],
            surface_format,
            wgpu::PrimitiveTopology::TriangleList,
            Blend::Replace,
            DepthWrite::On,
        );
        let lines = build_pipeline(
            device,
            &pipeline_layout,
            &line_shader,
            "etendue-lines",
            &[LineVertex::layout()],
            surface_format,
            wgpu::PrimitiveTopology::LineList,
            Blend::Replace,
            DepthWrite::On,
        );
        let points = build_pipeline(
            device,
            &pipeline_layout,
            &line_shader,
            "etendue-points",
            &[LineVertex::layout()],
            surface_format,
            wgpu::PrimitiveTopology::PointList,
            Blend::Replace,
            DepthWrite::On,
        );

        let depth_view = create_depth_view(device, width, height);

        // The renderer is created before any defocus map / working volume is
        // computed, so the initial scene draws plain target quads with no
        // working-volume overlay; `rebuild_scene` installs the M4 heatmap and
        // the M6 working-volume patch once the application has them.
        let drawables = build_scene(device, &model_layout, scene, None, None, None);

        Self {
            model_layout,
            globals_buffer,
            globals_bind_group,
            mesh_opaque,
            mesh_translucent,
            mesh_vertex_color,
            lines,
            points,
            depth_view,
            drawables,
        }
    }

    /// Recreate the depth texture for a new surface size.
    ///
    /// Called on every window resize; the color target is the surface itself,
    /// so only the depth attachment needs rebuilding. A zero dimension is
    /// ignored — the swapchain is not reconfigured at that size either.
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.depth_view = create_depth_view(device, width, height);
    }

    /// Record the full 3D frame into `encoder`, drawing into `color_view`.
    ///
    /// Uploads the camera uniform via `queue`, then records a single render
    /// pass that clears color + depth and runs the opaque pass followed by the
    /// translucent pass. The egui pass is recorded separately by the caller,
    /// afterwards, into the same surface (loading, not clearing).
    pub fn render(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        color_view: &wgpu::TextureView,
        camera: &OrbitCamera,
        width: u32,
        height: u32,
    ) {
        // Upload this frame's camera + lighting uniform.
        let aspect = width as f32 / height.max(1) as f32;
        let globals = GlobalsUniform::from_camera(camera, aspect);
        queue.write_buffer(&self.globals_buffer, 0, bytemuck::bytes_of(&globals));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("etendue-viewport-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(CLEAR_COLOR),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        pass.set_bind_group(0, &self.globals_bind_group, &[]);

        // --- Opaque pass: meshes + lines + points, depth write on ---------
        for d in self.drawables.iter().filter(|d| d.pass == Pass::Opaque) {
            self.record_drawable(&mut pass, d);
        }

        // --- Translucent pass: alpha-blended meshes, depth write off ------
        // Recorded after every opaque draw so translucent surfaces depth-test
        // against (and are correctly occluded by) the opaque geometry.
        //
        // Within the pass the translucent drawables are recorded back-to-front
        // — farthest from the eye first — so overlapping alpha blends in the
        // correct order. M8 added camera-anatomy quads, M6 the working-volume
        // patch, and M10 the voxel-overlap cloud — all translucent.
        let eye = camera.eye();
        let mut translucent: Vec<&Drawable> = self
            .drawables
            .iter()
            .filter(|d| d.pass == Pass::Translucent)
            .collect();
        translucent.sort_by(|a, b| {
            let da = (a.centroid - eye).norm_squared();
            let db = (b.centroid - eye).norm_squared();
            // Farther (larger distance) first.
            db.partial_cmp(&da).unwrap_or(std::cmp::Ordering::Equal)
        });
        for d in translucent {
            self.record_drawable(&mut pass, d);
        }
    }

    /// Record the draw calls for one drawable into an open render pass.
    fn record_drawable<'p>(&'p self, pass: &mut wgpu::RenderPass<'p>, d: &'p Drawable) {
        pass.set_bind_group(1, &d.model_bind_group, &[]);
        match &d.geometry {
            Geometry::Mesh(mesh) => {
                let pipeline = match d.pass {
                    Pass::Opaque => &self.mesh_opaque,
                    Pass::Translucent => &self.mesh_translucent,
                };
                pass.set_pipeline(pipeline);
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            }
            Geometry::ColoredMesh(mesh) => {
                // The heatmap grid: always opaque, unlit per-vertex color.
                pass.set_pipeline(&self.mesh_vertex_color);
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            }
            Geometry::Lines(lines) => {
                pass.set_pipeline(&self.lines);
                pass.set_vertex_buffer(0, lines.vertices.slice(..));
                pass.draw(0..lines.vertex_count, 0..1);
            }
            Geometry::Points(points) => {
                pass.set_pipeline(&self.points);
                pass.set_vertex_buffer(0, points.vertices.slice(..));
                pass.draw(0..points.vertex_count, 0..1);
            }
        }
    }

    /// Rebuild every scene drawable from a mutated [`Scene`](etendue_core::Scene).
    ///
    /// Called by the application when the M3 parameter panel reports that any
    /// slider or drag-value changed this frame. Because focal-length and
    /// fan-angle edits change geometry (not just transforms), a blanket rebuild
    /// is the simplest correct approach. The scene is small enough that the
    /// rebuild cost is not perceptible.
    ///
    /// The grid and world axes are regenerated as well — their segments are
    /// static, so this is a no-op in effect but keeps `build_scene`'s contract
    /// simple (it always generates the full list).
    ///
    /// `heatmap` is the M4 defocus map for the **first** target: when `Some`,
    /// that target renders as the colored heatmap grid instead of the plain
    /// shaded quad; when `None` (the heatmap toggle is off, or there is no
    /// camera/target to compute it from) every target is the plain quad.
    ///
    /// `working_volume` is the M6 working volume for the **first** camera +
    /// **first** laser: when `Some`, a translucent patch covering the
    /// in-volume cells is drawn on the laser fan plane, lifted toward the
    /// orbit camera; when `None` (the toggle is off, or no working volume
    /// could be computed) no patch is drawn.
    pub fn rebuild_scene(
        &mut self,
        device: &wgpu::Device,
        scene: &etendue_core::Scene,
        heatmap: Option<&DefocusMap>,
        working_volume: Option<&WorkingVolume>,
        voxel_overlap: Option<(&VoxelOverlap, u32)>,
    ) {
        self.drawables = build_scene(
            device,
            &self.model_layout,
            scene,
            heatmap,
            working_volume,
            voxel_overlap,
        );
    }
}

/// A uniform-buffer bind group layout entry at `binding`, visible to `stages`.
fn uniform_entry(binding: u32, stages: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: stages,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

/// Color-blend mode selector for [`build_pipeline`].
#[derive(Debug, Clone, Copy)]
enum Blend {
    /// Opaque: the fragment replaces the target.
    Replace,
    /// Standard `src.a`-weighted alpha blending for the translucent pass.
    AlphaBlend,
}

/// Depth-write selector for [`build_pipeline`].
#[derive(Debug, Clone, Copy)]
enum DepthWrite {
    /// Write depth (opaque geometry).
    On,
    /// Depth-test only, do not write (translucent geometry).
    Off,
}

/// Build one render pipeline.
///
/// Shared by all four viewport pipelines; the topology, blend mode, and
/// depth-write flag are the only axes that vary. Every pipeline uses the same
/// depth format and a `Less` depth-compare, back-face culling off (line/point
/// topologies are not affected by culling, and the translucent quad must be
/// visible from both sides).
#[allow(clippy::too_many_arguments)]
fn build_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    label: &str,
    vertex_layouts: &[wgpu::VertexBufferLayout<'_>],
    surface_format: wgpu::TextureFormat,
    topology: wgpu::PrimitiveTopology,
    blend: Blend,
    depth_write: DepthWrite,
) -> wgpu::RenderPipeline {
    let blend_state = match blend {
        Blend::Replace => wgpu::BlendState::REPLACE,
        Blend::AlphaBlend => wgpu::BlendState::ALPHA_BLENDING,
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: vertex_layouts,
        },
        primitive: wgpu::PrimitiveState {
            topology,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            // No culling: lines/points are unaffected and the translucent
            // quad must be readable from either side.
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(matches!(depth_write, DepthWrite::On)),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: Some(blend_state),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// Allocate a depth texture sized `width` x `height` and return its view.
fn create_depth_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("etendue-depth-texture"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// RGB color of a camera frustum wireframe — a warm amber.
const CAMERA_COLOR: [f32; 3] = [0.95, 0.78, 0.25];
/// RGB color of a laser fan — a saturated red.
const LASER_COLOR: [f32; 3] = [0.95, 0.20, 0.20];
/// Alpha of the translucent laser fan.
const LASER_ALPHA: f32 = 0.30;
/// RGB color of a target quad — a neutral mid grey.
const TARGET_COLOR: [f32; 3] = [0.62, 0.64, 0.68];
/// RGB color of the M5 laser stripe on the target — a bright, near-white red
/// so the projected line reads clearly against both the grey target quad and
/// the defocus heatmap.
const LASER_STRIPE_COLOR: [f32; 3] = [1.0, 0.42, 0.38];

/// RGB color of the M8 imager (sensor plane) — a cool blue distinct from the
/// amber frustum and the warm origin marker.
const IMAGER_COLOR: [f32; 3] = [0.30, 0.50, 0.85];
/// RGB color of the M8 front principal plane `H` — a pale cyan.
const PRINCIPAL_H_COLOR: [f32; 3] = [0.55, 0.85, 0.95];
/// RGB color of the M8 rear principal plane `H'` — a pale lavender, distinct
/// from `H` so the two read as separate planes when the inter-principal gap is
/// non-zero.
const PRINCIPAL_HPRIME_COLOR: [f32; 3] = [0.80, 0.78, 0.95];
/// Alpha of the M8 translucent camera-anatomy quads (imager + principal
/// planes). Slightly lower than the laser fan so the camera body never reads
/// as the dominant volume.
const CAMERA_ANATOMY_ALPHA: f32 = 0.35;
/// RGB color of the M8 lens aperture ring — a warm gold, distinct from both
/// `CAMERA_COLOR` (more orange/amber) and `ORIGIN_MARKER_COLOR` (creamier).
const APERTURE_COLOR: [f32; 3] = [0.95, 0.85, 0.40];

/// RGB color of the F7 mesh-target surface — a slightly warmer mid-green,
/// distinct from the grey planar `TARGET_COLOR` so mesh and planar targets
/// read as different entity kinds.
const MESH_TARGET_COLOR: [f32; 3] = [0.40, 0.70, 0.45];

/// Build every [`Drawable`] for a scene: the ground grid + world axes, then
/// one drawable per camera / laser / target entity.
///
/// Each entity's geometry is generated in its **entity-local** frame by the
/// helpers in [`crate::viewport::scene`]; the entity's `pose` is handed to the
/// renderer as the model matrix, so the GPU does the local→world transform.
/// The model matrix is uploaded once at construction; poses are edited via
/// the parameter panel which rebuilds the scene each frame.
///
/// Mapping from entity to primitive:
///
/// - [`CameraEntity`](etendue_core::CameraEntity) → a **frustum wireframe**
///   (line-list, opaque): the 12 edges of the view frustum between the near
///   and far draw planes.
/// - [`LaserEntity`](etendue_core::LaserEntity) → a **translucent fan**
///   (triangle mesh, translucent pass): a thin two-triangle wedge.
/// - [`TargetEntity`](etendue_core::TargetEntity) → a **shaded quad**
///   (triangle mesh, opaque): the target rectangle — *unless* it is the first
///   target and `heatmap` is `Some`, in which case it is the M4 **defocus
///   heatmap grid** (per-vertex-colored triangle mesh, opaque).
///
/// In addition, the M5 **laser stripe** — the line where the first laser's
/// fan strikes the first target — is drawn as an opaque line-list, in world
/// coordinates (see [`laser_stripe_segments`](crate::viewport::scene::laser_stripe_segments)).
/// It is omitted when the fan misses the target.
///
/// `heatmap` carries the [`DefocusMap`] for the first target only — the
/// `etendue-core` `defocus_map` analysis is defined for one camera against
/// one target, and the MVP scene has exactly one of each. Any further targets
/// always draw as the plain quad.
///
/// `working_volume` carries the M6 [`WorkingVolume`] for the first camera +
/// first laser; when `Some` a translucent cyan patch covering the in-volume
/// cells is added to the translucent pass, sitting on the laser fan plane
/// and lifted toward the orbit camera so it does not z-fight the fan.
fn build_scene(
    device: &wgpu::Device,
    model_layout: &wgpu::BindGroupLayout,
    scene: &etendue_core::Scene,
    heatmap: Option<&DefocusMap>,
    working_volume: Option<&WorkingVolume>,
    voxel_overlap: Option<(&VoxelOverlap, u32)>,
) -> Vec<Drawable> {
    use crate::viewport::scene::{
        VOXEL_OVERLAP_ALPHA, VOXEL_OVERLAP_COLOR, WORKING_VOLUME_ALPHA, WORKING_VOLUME_COLOR,
        camera_aperture_ring, camera_frustum_edges, camera_imager_mesh,
        camera_principal_plane_meshes, isometry_to_matrix4, laser_fan_mesh, laser_stripe_segments,
        mesh_laser_stripe_segments, target_quad_mesh, voxel_overlap_mesh, working_volume_mesh,
    };

    let mut drawables = Vec::new();
    let white = [1.0, 1.0, 1.0, 1.0];

    // --- Ground grid + world axes ---------------------------------------
    // Built directly in world coordinates, so the model matrix is the
    // identity and the centroid (unused for opaque geometry) is the origin.
    let grid = GpuLines::from_segments(device, &grid_and_axes_segments());
    drawables.push(Drawable::new(
        device,
        model_layout,
        Geometry::Lines(grid),
        Matrix4::identity(),
        white,
        Pass::Opaque,
        Point3::origin(),
    ));

    // --- Cameras: frustum wireframes + an origin marker -----------------
    for camera in &scene.cameras {
        let model = isometry_to_matrix4(&camera.pose);

        let edges = camera_frustum_edges(camera, CAMERA_COLOR);
        // Local centroid: the mean of the segment endpoints. Opaque, so it is
        // only carried for completeness.
        let local_centroid = segment_centroid(&edges);
        let lines = GpuLines::from_segments(device, &edges);
        drawables.push(Drawable::new(
            device,
            model_layout,
            Geometry::Lines(lines),
            model,
            white,
            Pass::Opaque,
            local_centroid,
        ));

        // A point marker at the camera-local origin — the frustum apex, i.e.
        // where the camera body sits — so the sensor stays locatable when its
        // frustum is viewed edge-on.
        drawables.push(origin_marker(device, model_layout, model));

        // --- M8 camera anatomy: imager + principal planes + aperture -------
        // The imager (sensor plane) is drawn at z = -sensor_distance in camera
        // local; the two principal planes at z = -g (H) and z = 0 (H'); and a
        // thin aperture ring at z = -g/2 with diameter f/N. All translucent
        // except the ring (a line list), all using the existing pipelines.
        if let Some(imager) = camera_imager_mesh(camera) {
            let local_centroid = mesh_centroid(&imager);
            let gpu = GpuMesh::from_trimesh(device, &imager, IMAGER_COLOR);
            drawables.push(Drawable::new(
                device,
                model_layout,
                Geometry::Mesh(gpu),
                model,
                [1.0, 1.0, 1.0, CAMERA_ANATOMY_ALPHA],
                Pass::Translucent,
                local_centroid,
            ));
        }
        let (h_mesh, h_prime_mesh) = camera_principal_plane_meshes(camera);
        for (mesh, color) in [
            (h_mesh, PRINCIPAL_H_COLOR),
            (h_prime_mesh, PRINCIPAL_HPRIME_COLOR),
        ] {
            let local_centroid = mesh_centroid(&mesh);
            let gpu = GpuMesh::from_trimesh(device, &mesh, color);
            drawables.push(Drawable::new(
                device,
                model_layout,
                Geometry::Mesh(gpu),
                model,
                [1.0, 1.0, 1.0, CAMERA_ANATOMY_ALPHA],
                Pass::Translucent,
                local_centroid,
            ));
        }
        let aperture_segments = camera_aperture_ring(camera, APERTURE_COLOR);
        if !aperture_segments.is_empty() {
            let local_centroid = segment_centroid(&aperture_segments);
            let aperture_gpu = GpuLines::from_segments(device, &aperture_segments);
            drawables.push(Drawable::new(
                device,
                model_layout,
                Geometry::Lines(aperture_gpu),
                model,
                white,
                Pass::Opaque,
                local_centroid,
            ));
        }
    }

    // --- Lasers: translucent fans + an origin marker --------------------
    for laser in &scene.lasers {
        let model = isometry_to_matrix4(&laser.pose);

        let mesh = laser_fan_mesh(laser);
        let local_centroid = mesh_centroid(&mesh);
        let gpu = GpuMesh::from_trimesh(device, &mesh, LASER_COLOR);
        drawables.push(Drawable::new(
            device,
            model_layout,
            Geometry::Mesh(gpu),
            model,
            [1.0, 1.0, 1.0, LASER_ALPHA],
            Pass::Translucent,
            local_centroid,
        ));

        // A point marker at the laser-local origin — the fan apex.
        drawables.push(origin_marker(device, model_layout, model));
    }

    // --- Targets: shaded quads, or the M4 heatmap grid for target 0 -----
    for (i, target) in scene.targets.iter().enumerate() {
        let model = isometry_to_matrix4(&target.pose);

        // The first target shows the defocus heatmap when a map is supplied;
        // every other target — and the first when the toggle is off — is the
        // plain shaded quad from M2.
        if i == 0
            && let Some(map) = heatmap
        {
            drawables.push(heatmap_drawable(device, model_layout, map, model));
        } else {
            let mesh = target_quad_mesh(target);
            let local_centroid = mesh_centroid(&mesh);
            let gpu = GpuMesh::from_trimesh(device, &mesh, TARGET_COLOR);
            drawables.push(Drawable::new(
                device,
                model_layout,
                Geometry::Mesh(gpu),
                model,
                white,
                Pass::Opaque,
                local_centroid,
            ));
        }
    }

    // --- F7 mesh targets: opaque shaded meshes ----------------------------
    // Each `MeshTarget` carries its own `TriMesh`; the mesh is in the target's
    // local frame, so the entity pose becomes the model matrix — exactly the
    // same pattern the planar `TargetEntity` uses.  An empty `mesh_targets`
    // vec (the common case — the MVP default scene has none) simply skips the
    // loop.
    for mesh_target in &scene.mesh_targets {
        let model = isometry_to_matrix4(&mesh_target.pose);
        let local_centroid = mesh_centroid(&mesh_target.mesh);
        let gpu = GpuMesh::from_trimesh(device, &mesh_target.mesh, MESH_TARGET_COLOR);
        drawables.push(Drawable::new(
            device,
            model_layout,
            Geometry::Mesh(gpu),
            model,
            white,
            Pass::Opaque,
            local_centroid,
        ));
    }

    // --- F7 mesh laser stripe: where the fan strikes each mesh target -----
    // World-coordinate segments (one polyline per per-triangle stripe segment),
    // drawn with an identity model matrix like the planar stripe above.
    let mesh_stripe_segments = mesh_laser_stripe_segments(scene, LASER_STRIPE_COLOR);
    if !mesh_stripe_segments.is_empty() {
        let mesh_stripe = GpuLines::from_segments(device, &mesh_stripe_segments);
        drawables.push(Drawable::new(
            device,
            model_layout,
            Geometry::Lines(mesh_stripe),
            Matrix4::identity(),
            white,
            Pass::Opaque,
            segment_centroid(&mesh_stripe_segments),
        ));
    }

    // --- M5 laser stripe: the line where the fan strikes the target -----
    // A derived curve spanning the first laser and the first target. It is
    // already in world coordinates (see `laser_stripe_segments`), so it draws
    // with an identity model matrix, like the ground grid. An empty result
    // (the laser misses the target) simply adds no drawable.
    let stripe_segments = laser_stripe_segments(scene, LASER_STRIPE_COLOR);
    if !stripe_segments.is_empty() {
        let stripe = GpuLines::from_segments(device, &stripe_segments);
        drawables.push(Drawable::new(
            device,
            model_layout,
            Geometry::Lines(stripe),
            Matrix4::identity(),
            white,
            Pass::Opaque,
            segment_centroid(&stripe_segments),
        ));
    }

    // --- M6 working volume: a translucent patch on the laser fan plane --
    // The set of points the camera can simultaneously illuminate, see, and
    // focus on (see `etendue_core::analysis::working_volume`). Drawn as a
    // saturated cyan triangle mesh covering just the in-volume cells, lifted
    // a hair toward the orbit camera off the (translucent) fan so the two
    // surfaces do not z-fight. World-coordinate vertices => identity model.
    //
    // The lift direction depends on which side of the fan plane the orbit
    // camera sits — picked off the laser-to-camera vector below. Since the
    // orbit camera (the user's viewpoint) is not directly accessible here,
    // we pick the lift sign from the scene camera's position; in the MVP
    // single-camera scene this is the natural "front" side of the fan.
    if let (Some(wv), Some(laser), Some(scene_camera)) =
        (working_volume, scene.lasers.first(), scene.cameras.first())
    {
        let camera_position = scene_camera.pose * Point3::origin();
        if let Some(mesh) = working_volume_mesh(wv, laser, camera_position) {
            let local_centroid = mesh_centroid(&mesh);
            let gpu = GpuMesh::from_trimesh(device, &mesh, WORKING_VOLUME_COLOR);
            drawables.push(Drawable::new(
                device,
                model_layout,
                Geometry::Mesh(gpu),
                Matrix4::identity(),
                [1.0, 1.0, 1.0, WORKING_VOLUME_ALPHA],
                Pass::Translucent,
                local_centroid,
            ));
        }
    }

    // --- M10 voxel-overlap: the N-view agreement volume ------------------
    // Per the M10 plan (N-view voxelized overlap), this draws a translucent
    // cube cloud over the voxels whose pair-coverage is at or above the
    // user-chosen `min_overlap` threshold. World-coordinate vertices =>
    // identity model. Skipped when no voxel meets the threshold.
    if let Some((voxels, min_overlap)) = voxel_overlap
        && let Some(mesh) = voxel_overlap_mesh(voxels, min_overlap)
    {
        let local_centroid = mesh_centroid(&mesh);
        let gpu = GpuMesh::from_trimesh(device, &mesh, VOXEL_OVERLAP_COLOR);
        drawables.push(Drawable::new(
            device,
            model_layout,
            Geometry::Mesh(gpu),
            Matrix4::identity(),
            [1.0, 1.0, 1.0, VOXEL_OVERLAP_ALPHA],
            Pass::Translucent,
            local_centroid,
        ));
    }

    drawables
}

/// Build the M4 defocus-heatmap [`Drawable`] for a target.
///
/// Tessellates `map` into a per-vertex-colored grid (see
/// [`crate::viewport::heatmap::heatmap_mesh`]) lying in the target-local
/// `z = 0` plane, and wraps it as an opaque, unlit colored mesh. `model` is
/// the target entity's pose as a model matrix — the same matrix the plain
/// quad would use — so the heatmap lands exactly on the target surface.
///
/// The colormap upper bound is the map's [`DefocusMap::max_coc_px`]; an
/// all-sharp map (maximum `0`, or `None`) collapses the ramp so the grid is
/// uniformly the in-focus color.
fn heatmap_drawable(
    device: &wgpu::Device,
    model_layout: &wgpu::BindGroupLayout,
    map: &DefocusMap,
    model: Matrix4<f32>,
) -> Drawable {
    use crate::viewport::heatmap::heatmap_mesh;

    let scale_max = map.max_coc_px().unwrap_or(0.0);
    let (vertices, indices) = heatmap_mesh(map, scale_max);
    let gpu = GpuColoredMesh::from_colored_vertices(device, &vertices, &indices);
    // The grid lies in the local z = 0 plane centred on the origin, so its
    // local centroid is the origin (unused for opaque geometry anyway).
    Drawable::new(
        device,
        model_layout,
        Geometry::ColoredMesh(gpu),
        model,
        [1.0, 1.0, 1.0, 1.0],
        Pass::Opaque,
        Point3::origin(),
    )
}

/// Build an opaque point-marker [`Drawable`] sitting at the local origin of an
/// entity, placed in the world by `model`.
///
/// Used to mark a camera's frustum apex and a laser's fan apex — the points
/// where the entity body conceptually sits — and, incidentally, to keep the
/// point-list pipeline exercised by every entity-origin marker in the scene.
fn origin_marker(
    device: &wgpu::Device,
    model_layout: &wgpu::BindGroupLayout,
    model: Matrix4<f32>,
) -> Drawable {
    let pts = GpuPoints::from_points(device, &[(Point3::origin(), ORIGIN_MARKER_COLOR)]);
    Drawable::new(
        device,
        model_layout,
        Geometry::Points(pts),
        model,
        [1.0, 1.0, 1.0, 1.0],
        Pass::Opaque,
        Point3::origin(),
    )
}

/// The centroid (mean position) of a triangle mesh's vertices, narrowed to
/// `f32` for the renderer's world-space sort key.
fn mesh_centroid(mesh: &etendue_core::geom::TriMesh) -> Point3<f32> {
    let verts = mesh.vertices();
    if verts.is_empty() {
        return Point3::origin();
    }
    let mut sum = nalgebra::Vector3::<f64>::zeros();
    for v in verts {
        sum += v.coords;
    }
    let mean = sum / verts.len() as f64;
    Point3::new(mean.x as f32, mean.y as f32, mean.z as f32)
}

/// The centroid of a list of line segments — the mean of every endpoint,
/// narrowed to `f32`.
fn segment_centroid(segments: &[(Point3<f64>, Point3<f64>, [f32; 3])]) -> Point3<f32> {
    if segments.is_empty() {
        return Point3::origin();
    }
    let mut sum = nalgebra::Vector3::<f64>::zeros();
    for &(a, b, _) in segments {
        sum += a.coords + b.coords;
    }
    let mean = sum / (segments.len() as f64 * 2.0);
    Point3::new(mean.x as f32, mean.y as f32, mean.z as f32)
}

/// Generate the ground-grid lines plus the three colored world axes as
/// `(start, end, color)` segments in world space.
///
/// The grid is a square lattice in the `z = 0` plane; the axes are short
/// colored segments from the origin — X red, Y green, Z blue.
fn grid_and_axes_segments() -> Vec<(Point3<f64>, Point3<f64>, [f32; 3])> {
    /// Half-extent of the grid, world units.
    const HALF: i32 = 6;
    /// Grid lines on the cardinal axes are skipped — the colored axes draw
    /// over them.
    const GRID_COLOR: [f32; 3] = [0.22, 0.25, 0.30];

    let mut segs = Vec::new();
    let half = f64::from(HALF);
    for i in -HALF..=HALF {
        if i == 0 {
            // The center lines are drawn as colored axes below.
            continue;
        }
        let t = f64::from(i);
        // Lines parallel to X.
        segs.push((
            Point3::new(-half, t, 0.0),
            Point3::new(half, t, 0.0),
            GRID_COLOR,
        ));
        // Lines parallel to Y.
        segs.push((
            Point3::new(t, -half, 0.0),
            Point3::new(t, half, 0.0),
            GRID_COLOR,
        ));
    }

    // World axes from the origin: X red, Y green, Z blue.
    let axis_len = 2.0;
    segs.push((
        Point3::origin(),
        Point3::new(axis_len, 0.0, 0.0),
        [0.90, 0.30, 0.30],
    ));
    segs.push((
        Point3::origin(),
        Point3::new(0.0, axis_len, 0.0),
        [0.35, 0.80, 0.35],
    ));
    segs.push((
        Point3::origin(),
        Point3::new(0.0, 0.0, axis_len),
        [0.40, 0.55, 0.95],
    ));
    segs
}

/// RGB color of an entity-origin point marker — a bright near-white.
const ORIGIN_MARKER_COLOR: [f32; 3] = [1.0, 0.95, 0.80];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_to_cols_is_column_major() {
        // Build a matrix with distinct entries and check the [col][row]
        // layout WGSL expects.
        let m = Matrix4::new(
            1.0, 2.0, 3.0, 4.0, // row 0
            5.0, 6.0, 7.0, 8.0, // row 1
            9.0, 10.0, 11.0, 12.0, // row 2
            13.0, 14.0, 15.0, 16.0, // row 3
        );
        let cols = matrix_to_cols(m);
        // First WGSL column is the matrix's first column: entries (r, 0).
        assert_eq!(cols[0], [1.0, 5.0, 9.0, 13.0]);
        // Last WGSL column is the matrix's last column: entries (r, 3).
        assert_eq!(cols[3], [4.0, 8.0, 12.0, 16.0]);
    }

    #[test]
    fn identity_model_uniform_has_identity_normal_matrix() {
        let u = ModelUniform::new(Matrix4::identity(), [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(u.model, matrix_to_cols(Matrix4::identity()));
        assert_eq!(u.normal, matrix_to_cols(Matrix4::identity()));
    }

    #[test]
    fn translation_does_not_disturb_the_normal_matrix() {
        // The normal matrix is the inverse-transpose; for a pure translation
        // its upper-left 3x3 stays the identity, so normals are unrotated.
        let model = Matrix4::new_translation(&nalgebra::Vector3::new(3.0, -2.0, 5.0));
        let u = ModelUniform::new(model, [1.0; 4]);
        // Upper-left 3x3 identity check.
        for (c, col) in u.normal.iter().enumerate().take(3) {
            for (r, &value) in col.iter().enumerate().take(3) {
                let expected = if r == c { 1.0 } else { 0.0 };
                assert!((value - expected).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn grid_and_axes_have_the_expected_segment_count() {
        // The grid/axes generator is pure (no GPU); verify it independently.
        let segs = grid_and_axes_segments();
        // 2 * (2*HALF) grid lines + 3 axes; HALF = 6 -> 24 + 3.
        assert_eq!(segs.len(), 24 + 3);
    }

    #[test]
    fn mesh_centroid_is_the_mean_of_the_vertices() {
        // A quad spanning ±1 in x and y is centred on the origin.
        let quad = etendue_core::geom::TriMesh::quad(2.0, 2.0);
        let c = mesh_centroid(&quad);
        assert!(c.coords.norm() < 1e-6, "quad centroid should be the origin");
    }

    #[test]
    fn segment_centroid_is_the_mean_of_the_endpoints() {
        // Two segments symmetric about (1, 0, 0): the centroid is that point.
        let segs = vec![
            (
                Point3::new(0.0, 0.0, 0.0),
                Point3::new(2.0, 0.0, 0.0),
                [1.0, 1.0, 1.0],
            ),
            (
                Point3::new(1.0, -1.0, 0.0),
                Point3::new(1.0, 1.0, 0.0),
                [1.0, 1.0, 1.0],
            ),
        ];
        let c = segment_centroid(&segs);
        assert!((c.x - 1.0).abs() < 1e-6);
        assert!(c.y.abs() < 1e-6);
        assert!(c.z.abs() < 1e-6);
    }

    #[test]
    fn segment_centroid_of_empty_is_the_origin() {
        let c = segment_centroid(&[]);
        assert!(c.coords.norm() < 1e-12);
    }
}
