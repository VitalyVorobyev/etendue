# Scene and Geometry

`etendue-core::scene` and `etendue-core::geom` are the data the whole tool
revolves around: the headless representation of the optical-design scene and
the geometric primitives every analysis layer is built on.

## The `Scene`

```rust
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Scene {
    pub cameras: Vec<CameraEntity>,
    pub lasers: Vec<LaserEntity>,
    pub targets: Vec<TargetEntity>,
    #[serde(default)]
    pub mesh_targets: Vec<MeshTarget>,
}
```

A flat record of four entity vectors. The fields are `pub` because the UI
poses and appends entities directly; there is no aggregation logic to wrap.
`mesh_targets` carries `#[serde(default)]`, so scene files written before
mesh targets existed still load.
Save/Load goes through `serde_json::to_string_pretty` / `from_str` — the
parameter panel uses the [`rfd`](https://crates.io/crates/rfd) native dialog
crate to pick paths.

## The entities

All three share the convention `pose: Isometry3<f64>` maps local-frame
coordinates into world coordinates (`p_world = pose * p_local`). World is
right-handed, **+Z up**.

### `CameraEntity`

```rust
pub struct CameraEntity {
    pub pose: Isometry3<f64>,
    pub params: CameraParams,           // calibration-rs spec — see below
    pub resolution: (u32, u32),
    pub optics: PhysicalOptics,         // the five physical-optics scalars
    pub frustum_near: f64,
    pub frustum_far: f64,
}

pub struct PhysicalOptics {
    pub effective_focal_length_m: f64,
    pub f_number: f64,
    pub focus_distance_m: f64,
    pub principal_gap_m: f64,           // H - H', >= 0
    pub pixel_pitch_m: f64,
}
```

The five physical-optics scalars live in the nested `PhysicalOptics` struct;
they are the source of truth, and the pixel-unit `fx`/`fy` in
`params.intrinsics` are derived from `effective_focal_length_m` and
`pixel_pitch_m` by `sync_intrinsics_from_physical`.

Camera-local convention is the calibration-rs convention: **+z forward**, +x
right, +y down. `Camera::project_point_c` rejects `z ≤ 0`;
`Camera::backproject_pixel` returns a ray as a point on the `z = 1` plane.

### Why `CameraParams`, not `CameraModel`

`CameraEntity` retains the *spec* (`CameraParams`), not a built
`CameraModel`. The reason is Scheimpflug tilt:

> `CameraParams::build()` compiles `SensorParams::Scheimpflug` into an
> `AnySensor::Homography`, and the built model no longer carries the tilt
> angles `(tau_x, tau_y)`. The M4 defocus model needs those angles to place
> the tilted plane of best focus, so they must be retained here.

Consumers that only need the projection chain call `.build()` on demand.
Consumers that need the tilt (the [thick-lens layer](thick_lens.md)) read it
straight out of `params.sensor` via `optics::scheimpflug_tilt`.

### `LaserEntity`

```rust
pub struct LaserEntity {
    pub pose: Isometry3<f64>,
    pub fan_half_angle: f64,    // radians, in (0, π/2)
    pub fan_length: f64,        // metres, along the central ray
    pub wavelength_nm: f64,     // nanometres — laser-industry convention
    pub beam_waist_m: f64,      // 1/e² radius w₀ at the waist (origin)
}
```

Laser-local: fan lies in `x = 0`, opens symmetrically about local **+z**.
A ray at fan-angle `phi` has local direction `(0, sin phi, cos phi)`.

### `TargetEntity`

```rust
pub struct TargetEntity {
    pub pose: Isometry3<f64>,
    pub width: f64,    // local x extent, metres
    pub height: f64,   // local y extent, metres
}
```

A finite rectangle in its local `z = 0` plane, outward normal along local
**+z** — the analytic target the defocus map and the planar laser-stripe
intersection sample.

### `MeshTarget`

```rust
pub struct MeshTarget {
    pub pose: Isometry3<f64>,
    pub mesh: TriMesh,
}
```

A posed triangle mesh, for non-planar inspection surfaces. The scene's
`mesh_targets` vector holds them; the laser fan intersects each through
`laser::stripe_segments_on_mesh` (per-triangle plane cuts), and the renderer
draws the mesh directly. `TriMesh` derives `Serialize`/`Deserialize` so a
mesh target round-trips through scene JSON. Sphere / cylinder / step
*parametric* primitives remain post-MVP.

## `Scene::default_mvp`

The default scene is the textbook single-camera / single-laser triangulation
rig the application opens with:

| entity | pose                        | parameters                           |
|--------|-----------------------------|--------------------------------------|
| target | upright at `(0, 0, 0.30)`, normal along world +Y | 0.15 m × 0.12 m                      |
| laser  | at `(0, -0.60, 0.30)`, aimed along world +Y | half-angle 0.30 rad, 660 nm, `w0` = 0.25 mm |
| camera | at `(0.28, -0.55, 0.30)`, aimed at target centre | 1280×1024 @ 3.45 µm, 16 mm f/2.8 focused at the camera-to-target distance |

The camera-to-target standoff is `sqrt(0.28² + 0.55²) ≈ 0.617 m`. A 16 mm
lens at that distance gives roughly a 15.7° × 12.6° field of view — wide
enough to frame the target with a margin. The f/2.8 aperture is open enough
that the CoC runs from ≈ 0 at the focused on-axis point to ≈ 2.6 px at the
oblique target edges, so the heatmap shows a genuine in-focus band with a
visible falloff rather than a uniformly sharp quad. These choices are not
arbitrary; they are documented in the `Scene::default_mvp` doc-comment as
the *FOV / defocus coherence* note.

## Geometry primitives — `geom`

`etendue-core::geom` holds the pure-geometry kernel: `f64`, no rendering, no
UI. The UI crate (`etendue-ui`) converts to `f32` GPU buffers at its own
boundary.

### `TriMesh`

```rust
pub struct TriMesh {
    vertices: Vec<Point3<f64>>,
    normals:  Vec<Vector3<f64>>,    // per-vertex
    indices:  Vec<[u32; 3]>,
}
```

Invariants enforced by `TriMesh::new`: one normal per vertex, every index in
bounds. Per-vertex normals (rather than per-face) keep the GPU vertex layout
uniform: primitives that need hard edges duplicate shared corners,
smooth-surface primitives reuse vertices. Two constructors:
`TriMesh::unit_cube(edge)` (six faces × four corners, flat per-face normals)
and `TriMesh::quad(width, height)` (two triangles, four shared vertices in
`z = 0`).

## Frustum geometry

The camera frustum is not a separate type. It is derived from the
calibration-rs `Camera::backproject_pixel` in the UI's `viewport::scene`
helper: back-project the four sensor corners, scale the returned `Ray.point`
(which sits on the local `z = 1` plane) by `frustum_near` and `frustum_far`
to obtain the 8 corners of the frustum, then connect them with the 12
familiar edges. The wireframe goes into the `lines` GPU pipeline (see [the
UI chapter](ui.md)); wgpu's Metal backend has no `PolygonMode::Line`, so
explicit line-list edges are the portable way.
