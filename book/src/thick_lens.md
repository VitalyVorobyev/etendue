# Thick Lens and Defocus

`etendue-core::optics` is the **M4 defocus physics** — the single most
correctness-critical part of the kernel. It computes where a camera is in
focus and how blurred an off-focus object point is, accounting for both
thick-lens conjugates and Scheimpflug sensor tilt.

The full derivation of the Scheimpflug tilted plane of best focus and the
geometric circle of confusion lives in the
[Scheimpflug derivation chapter](scheimpflug.md). This chapter documents the
*API* — what the kernel exposes and how it bundles the camera spec with the
thick-lens scalars.

## Module layout

```text
optics/
├── coc.rs          // pure-physics free functions (the derivation in code)
└── thick_lens.rs   // ThickLens — the camera-aware wrapper
```

`coc.rs` holds the bare physics as documented `f64` functions:
`image_distance`, `sensor_distance`, `pobf_tilt_tan`,
`coc_diameter_at_sensor`, `coc_to_pixels`, plus the `PlaneOfBestFocus` value
type. `thick_lens.rs` wraps them with a camera-aware API.

## `ThickLens`

```rust
pub struct ThickLens {
    params: CameraParams,           // calibration-rs camera spec
    focal_length_m: f64,            // f
    f_number: f64,                  // N; aperture diameter D = f / N
    focus_distance_m: f64,          // s_o (from rear principal plane H')
    principal_gap_m: f64,           // g = H - H', >= 0
    pixel_pitch_m: f64,
}

impl ThickLens {
    pub fn new(
        params: CameraParams,
        focal_length_m: f64,
        f_number: f64,
        focus_distance_m: f64,
        principal_gap_m: f64,
        pixel_pitch_m: f64,
    ) -> Result<Self>;

    pub fn params(&self) -> &CameraParams;
    pub fn focal_length_m(&self) -> f64;
    pub fn f_number(&self) -> f64;
    pub fn focus_distance_m(&self) -> f64;
    pub fn principal_gap_m(&self) -> f64;
    pub fn pixel_pitch_m(&self) -> f64;
    pub fn aperture_diameter_m(&self) -> f64;       // f / N
    pub fn sensor_tilt(&self) -> (f64, f64);        // (τx, τy) from CameraParams

    pub fn sensor_distance_m(&self) -> Result<f64>;
    pub fn plane_of_best_focus(&self) -> Result<PlaneOfBestFocus>;

    pub fn coc_diameter(&self, point_camera: &Point3<f64>) -> Result<f64>;
    pub fn coc_diameter_px(&self, point_camera: &Point3<f64>) -> Result<f64>;
}
```

## Why it holds `CameraParams`, not a built `CameraModel`

Same reason `CameraEntity` does (see [scene and geometry](scene_and_geometry.md)):

> `CameraParams::build()` compiles a `SensorParams::Scheimpflug` into an
> `AnySensor::Homography` — the built `CameraModel` keeps the homography
> but **discards the tilt angles**.

The plane-of-best-focus computation needs `(τx, τy)` directly. `ThickLens`
retains the `CameraParams` and reads the tilt with:

```rust
pub fn scheimpflug_tilt(params: &CameraParams) -> (f64, f64) {
    match &params.sensor {
        SensorParams::Scheimpflug { params } => (params.tilt_x, params.tilt_y),
        SensorParams::Identity | SensorParams::Homography { .. } => (0.0, 0.0),
    }
}
```

A non-Scheimpflug sensor is fronto-parallel by definition; a raw homography
*might* encode a tilt, but the angles cannot be unambiguously recovered, so
treating that case as "no known tilt" is honest.

Callers that need the projection chain still call `lens.params().build()` to
obtain a `CameraModel` on demand.

## The `coc_diameter` API and its upstream-promotion shape

`coc_diameter` is the upstream-promotion candidate. The signature is
deliberately small, self-contained, and **takes a 3D point**:

```rust
pub fn coc_diameter(&self, point_camera: &Point3<f64>) -> Result<f64>;
```

When etendue's optics model stabilises, this method becomes the prototype
for an `ApertureModel<S>` trait in calibration-rs — see [the
roadmap](roadmap.md). A scalar `(depth, off_axis)` pair would be the wrong
signature because under Scheimpflug tilt the CoC depends on the point's
*full* position relative to the tilted PoBF, and the lateral coordinate
cannot be reconstructed from a single scalar. The 3D-point signature stays
correct after the upstream promotion.

`coc_diameter_px` is just `coc_diameter / pixel_pitch_m` — the quantity a
focus heatmap and a depth-of-field criterion consume.

## Physical-parameter source of truth

The `CameraEntity` constructor establishes a hard contract: the **physical
optics fields** (effective focal length, focus distance, principal gap, pixel
pitch) are authoritative; the pixel-unit `fx`/`fy` in `params.intrinsics`
are **derived** from them.

```rust
pub fn sync_intrinsics_from_physical(&mut self) {
    let fx = self.focal_length_px();   // = f / pixel_pitch
    let IntrinsicsParams::FxFyCxCySkew { params } = &mut self.params.intrinsics;
    params.fx = fx;
    params.fy = fx;
}
```

`CameraEntity::new` calls this once at construction; a UI editing any of the
physical fields **must** call it after each edit. The principal-point
(`cx`, `cy`) and the skew are not touched — only `fx`/`fy` are rewritten.
With a 25 mm lens on a 3.45 µm pixel, that gives `fx = fy = 0.025 / 3.45e-6
≈ 7246.4 px`; with the default-scene 16 mm lens, `≈ 4637.7 px`.

`pixel_pitch_m` is the bridge between the lens (in metres) and the sensor
(in pixels), so it appears in both `CameraEntity` and `ThickLens`. There is
no shared `Sensor` aggregate at the etendue level — the pitch is simply
re-passed when the lens is built from the entity.

## The physics, briefly

A full derivation is in [the next chapter](scheimpflug.md). The short
statement:

- An ideal (aberration-free, paraxial) thick lens with effective focal
  length `f`, f-number `N` (aperture `D = f/N`), object-side focus distance
  `s_o` (from the rear principal plane `H'`), and inter-principal gap
  `g = H - H' ≥ 0`.
- The camera-local origin `z = 0` is the rear principal plane `H'`. An
  object point at camera depth `z` is at object distance `d_H = z + g`
  from `H`.
- Conjugate relation: `1/d_H + 1/d_i = 1/f`.
- On-axis sensor distance: `v0 = f (s_o + g) / (s_o + g - f)`.
- Plane of best focus rotated by sensor tilt: `tan(theta) = (s_o + g)/v0 ·
  tan(tau)` — the object-plane tilt is the sensor-plane tilt scaled by the
  inverse magnification.
- Geometric CoC: `c(P) = D · |s_i_P - v_sensor| / s_i_P`, where `s_i_P` is
  the image-side conjugate of P's depth and `v_sensor` is the local axial
  distance to the tilted sensor at P's paraxial image point.

`thick_lens.rs` is the camera-aware wrapper. `coc.rs` is the bare functions.
The Scheimpflug chapter is the why.
