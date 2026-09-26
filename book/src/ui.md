# The UI Layer

`etendue-ui` is the desktop binary — a hand-written winit + wgpu + egui
render loop, deliberately *not* `eframe`. The design tool needs
interleaved control of a custom 3D wgpu render pass and the egui frame, and
needs direct access to the device, surface, and per-frame command encoding
to do it cleanly. eframe abstracts those away; this binary owns them.

## Why egui + wgpu (over bevy or eframe)

bevy was the alternative considered in the handoff. It would have given an
ECS scene graph for free — a nice fit for "many cameras, many lasers, many
gizmos" — but the cost is a framework to learn, an `egui` integration
crate (`bevy_egui`), and an indirection between scene state and the render
loop. eframe was the second alternative, but its render-loop ownership made
the custom translucent-pass / vertex-color-pass interleaving awkward.

The chosen path is the simpler one: a winit event loop, wgpu pipelines this
crate compiles itself, and egui painting on top of the finished 3D image.
The 3D viewport requirements are unexotic (orbit/pan/zoom, wireframes,
flat-shaded triangles, translucent surfaces, lines/points/axes,
click-picking) — all standard wgpu territory.

## The five wgpu pipelines

```text
viewport/renderer.rs:

| pipeline            | shader      | topology   | blend          | depth write |
|---------------------|-------------|------------|----------------|-------------|
| mesh_opaque         | mesh.wgsl   | triangles  | replace        | yes         |
| mesh_translucent    | mesh.wgsl   | triangles  | alpha blend    | no          |
| mesh_vertex_color   | line.wgsl   | triangles  | replace        | yes         |
| lines               | line.wgsl   | line-list  | replace        | yes         |
| points              | line.wgsl   | point-list | replace        | yes         |
```

Two shaders, five pipelines. `mesh.wgsl` is the Lambert-shaded one;
`line.wgsl` is the unlit per-vertex-color one. The same shader serves the
line, point, and the M4 heatmap triangle pipeline (`mesh_vertex_color`):
unlit per-vertex colors paint the heatmap faithfully without Lambert
shading darkening the ramp.

### Bind groups

Two bind groups across all pipelines:

- **group 0** — `Globals` uniform (view-projection matrix, light direction,
  eye position). Bound once per frame.
- **group 1** — per-draw `Model` uniform (model matrix + flat RGBA tint).
  An RGBA `a < 1` routes a mesh through the `mesh_translucent` pipeline.

### Pass order: opaque, then translucent

One render pass: clear color + depth, draw opaque, draw translucent. The
translucent pass is depth-tested but **depth-write off**, so translucent
surfaces are correctly occluded by opaque geometry yet do not occlude one
another. With more than one translucent drawable the blend result still
depends on draw order, so translucent drawables are sorted **back-to-front**
by world-space centroid distance to the view camera. The scene now has
several translucent drawables: the laser fan, the M8 camera-anatomy quads,
the M6 working-volume patch, and the M10 voxel-overlap cloud.

## The `f64 → f32` GPU boundary

The kernel is `f64` everywhere. GPU buffers are `f32` (and `mat4x4<f32>`
WGSL uniforms). The packing happens at the `viewport::mesh` boundary, the
**only** place `f32` lives in the binary:

```rust
#[repr(C)]
#[derive(Pod, Zeroable)]
struct MeshVertex { position: [f32; 3], normal: [f32; 3] }
```

`bytemuck` does the byte-upload; `nalgebra` matrices are narrowed component
by component before they go into a `GlobalsUniform`. The kernel never sees
`f32`; the GPU never sees `f64`.

## Wireframes via explicit line-list edges

wgpu's Metal backend has no `PolygonMode::Line`. Wireframes (camera frusta,
the ground grid, the world axes) are explicit `LineVertex` line-list buffers
in the `lines` pipeline — pre-built once per scene rebuild. The frustum
edge list is derived from the calibration-rs `Camera::backproject_pixel` at
the four sensor corners (scaled to `frustum_near` and `frustum_far`),
giving the 8 corners and the 12 familiar edges; see [scene and
geometry](scene_and_geometry.md).

## Picking

Click-picking is not implemented in v0.1.0 — entity selection and dragging
are not yet wired. The orbital camera (mouse-drag) is the only pointer
interaction. CPU picking via ray–mesh intersection is planned; the triangle
mesh geometry is already available through `TriMesh::vertices()` /
`TriMesh::indices()`.

## The `Drawable` abstraction

```rust
pub struct Drawable {
    geometry:  Geometry,           // mesh | colored mesh | lines | points
    model:     ModelUniform,       // model matrix + RGBA tint
    pass:      Pass,               // Opaque | Translucent
    centroid:  Point3<f32>,        // for back-to-front sort
    // ...
}
```

`Renderer::rebuild_scene` turns a `Scene` (plus an optional `DefocusMap` and
`WorkingVolume`) into a `Vec<Drawable>`. The renderer partitions them by
pass, sorts the translucent slice back-to-front, and emits the draw calls.

## The orbit camera

`viewport::camera::OrbitCamera` is the user's viewpoint — distinct from the
simulated `CameraEntity`. It carries an azimuth, elevation, distance, and a
target point; the parameter panel does not edit it (mouse drags do).
Orthogonal to everything optical in the kernel.

## The panels

`panels::params::scene_panel` — the right-hand side panel:

- Save Scene / Load Scene buttons (native dialogs via `rfd`,
  `serde_json::to_string_pretty`).
- A **Defocus heatmap** toggle + color-scale legend (the legend's `max`
  matches the rendered colors).
- A **Working volume** toggle + area-m² / depth-range readouts.
- Collapsible sections per scene entity — camera, laser, target — with
  physical-parameter sliders. Editing the camera's `effective_focal_length_m`
  or `pixel_pitch_m` triggers a `sync_intrinsics_from_physical()` after the
  edit (the physical fields are authoritative; see [the thick-lens
  chapter](thick_lens.md)).

`panels::image_view::simulated_image_panel` — the bottom dock:

- An `egui_plot` 2D view at the camera's pixel-coordinate frame.
- The projected laser line drawn with **per-vertex width encoding**: a
  filled polygon band whose half-width at each vertex is `total_px / 2`,
  so the visualised thickness is exactly the simulated imaged line width
  (geometric ⊕ defocus blur, in quadrature; see [the laser
  chapter](laser.md)).

## The reactivity path

The whole "edit a slider, watch everything update" experience flows through
one branch in `Graphics::render`:

```text
scene_changed
  ├──► recompute defocus map      (analysis::defocus_map)
  ├──► recompute working volume   (analysis::working_volume)
  ├──► recompute projected stripe (laser::project_stripe of laser::stripe_on_target)
  └──► Renderer::rebuild_scene(scene, defocus_map, working_volume)
```

All four happen in the same frame the `scene_changed` bit is set, so the
heatmap, the fan-plane patch, and the simulated-image panel are coherent
with the 3D scene at every frame. Each recomputation is cheap (a few
thousand `f64` projections), and the rebuild lays out the GPU buffers from
scratch — the MVP scene is small enough that incremental updates buy
nothing.
