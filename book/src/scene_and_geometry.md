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
}
```

A flat record of three entity vectors. The fields are `pub` because the UI
poses and appends entities directly; there is no aggregation logic to wrap.
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
    pub effective_focal_length_m: f64,
    pub f_number: f64,
    pub focus_distance_m: f64,
    pub principal_gap_m: f64,           // H - H', >= 0
    pub pixel_pitch_m: f64,
    pub frustum_near: f64,
    pub frustum_far: f64,
}
```

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
**+z**. The MVP target geometry. Sphere / cylinder / step primitives are
post-MVP.

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

### `Ray3` and Möller–Trumbore

```rust
pub struct Ray3 { pub origin: Point3<f64>, pub direction: Vector3<f64> }
pub struct RayHit { pub t: f64, pub point: Point3<f64>, pub bary: (f64, f64) }

impl Ray3 {
    pub fn new(origin: Point3<f64>, direction: Vector3<f64>) -> Option<Self>;
    pub fn intersect_triangle(&self, a: Point3<f64>, b: Point3<f64>, c: Point3<f64>) -> Option<RayHit>;
    pub fn intersect_mesh(&self, mesh: &TriMesh) -> Option<RayHit>;
}
```

A parametric half-line `origin + t·direction`, `t ≥ 0`. The direction is
**not** required to be unit length — only a zero direction is rejected at
construction. `intersect_triangle` is Möller–Trumbore with Cramer's rule and
a scale-relative epsilon; the triangle is treated as **double-sided** (a hit
from either face counts), which simplifies the picking path. `intersect_mesh`
is a linear scan over triangles — adequate at the handful-of-scene-meshes
scale; no BVH.

This kernel is the basis for the UI's CPU click-picking and could later back
a laser-vs-mesh intersection when the MVP plane-only restriction is lifted.

## Frustum geometry

The camera frustum is not a separate type. It is derived from the
calibration-rs `Camera::backproject_pixel` in the UI's `viewport::scene`
helper: back-project the four sensor corners, scale the returned `Ray.point`
(which sits on the local `z = 1` plane) by `frustum_near` and `frustum_far`
to obtain the 8 corners of the frustum, then connect them with the 12
familiar edges. The wireframe goes into the `lines` GPU pipeline (see [the
UI chapter](ui.md)); wgpu's Metal backend has no `PolygonMode::Line`, so
explicit line-list edges are the portable way.
