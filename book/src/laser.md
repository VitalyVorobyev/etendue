# Laser Line Projection

`etendue-core::laser` is the M5 layer: it turns a posed `LaserEntity` plus a
planar `TargetEntity` plus a `CameraEntity` into a **simulated image** of
the laser stripe, with per-vertex pixel width that combines the geometric
cross-section and the defocus blur.

## The pipeline

```text
LaserEntity ──► LaserPlane ──┐
                             ├──► LaserStripe ──► ProjectedStripe
TargetEntity ────────────────┘    (intersect)      (project)
                                     ▲
                     WidthModel ─────┘
```

Four submodules, one per stage:

- `plane`   — `LaserPlane`, the world-frame fan geometry.
- `width`   — `WidthModel` trait + `GaussianBeamWidth`.
- `intersect` — `stripe_on_target`, the plane∩plane intersection clipped to
  the fan extent and the target rectangle.
- `project` — `project_stripe`, the projection through the camera with the
  quadrature pixel-width formula.

## `LaserPlane`

```rust
pub struct LaserPlane {
    origin:    Point3<f64>,    // fan apex (world)
    normal:    Vector3<f64>,   // fan-plane normal (laser-local +x)
    central:   Vector3<f64>,   // central ray direction (laser-local +z)
    in_plane:  Vector3<f64>,   // in-plane axis (laser-local +y)
    half_angle: f64,
    length:     f64,
}

impl LaserPlane {
    pub fn from_entity(laser: &LaserEntity) -> Self;
    pub fn origin(&self)    -> Point3<f64>;
    pub fn normal(&self)    -> Vector3<f64>;
    pub fn central(&self)   -> Vector3<f64>;
    pub fn in_plane(&self)  -> Vector3<f64>;
    pub fn half_angle(&self) -> f64;
    pub fn length(&self)     -> f64;

    pub fn ray_direction(&self, phi: f64) -> Vector3<f64>;   // cos·central + sin·in_plane
    pub fn signed_distance(&self, p: &Point3<f64>) -> f64;
    pub fn fan_angle_of  (&self, p: &Point3<f64>) -> f64;    // atan2 in the fan basis
    pub fn radius_of     (&self, p: &Point3<f64>) -> f64;
    pub fn contains_in_extent(&self, p: &Point3<f64>) -> bool;
}
```

Built from a `LaserEntity` by applying the entity pose to the laser-local
axes. The local convention from M2 is: fan in `x = 0`, opening symmetrically
about local **+z**, spreading within the local `y–z` plane. So `central`,
`in_plane`, and `normal` are the world images of local +z, +y, +x — a
right-handed orthonormal frame with `normal = in_plane × central`. A ray at
fan-angle `phi` has world direction `cos(phi)·central + sin(phi)·in_plane`,
unit length because the basis is orthonormal.

`contains_in_extent` is the **illumination predicate** of the working volume
(see [the working-volume chapter](working_volume.md)): a point lies inside
the fan when its `fan_angle_of` is within `±half_angle` and its `radius_of`
is within `length`.

## `WidthModel` and `GaussianBeamWidth`

```rust
pub trait WidthModel {
    fn width_at(&self, distance_m: f64) -> f64;   // full diameter, metres
}

pub struct GaussianBeamWidth { /* w0, zR */ }

impl GaussianBeamWidth {
    pub fn new        (waist_radius_m: f64, wavelength_m: f64) -> Result<Self>;
    pub fn from_laser (laser: &LaserEntity)                    -> Result<Self>;
    pub fn waist_radius_m  (&self) -> f64;
    pub fn rayleigh_range_m(&self) -> f64;
}

impl WidthModel for GaussianBeamWidth { /* w(d) = w0 sqrt(1 + (d/zR)^2), returns 2 w */ }
```

A focused Gaussian beam has a waist (its narrowest cross-section) and
diverges on either side of it. The `1/e²` radius as a function of axial
distance `z` from the waist is

$$
w(z) \;=\; w_{0}\,\sqrt{1 + (z / z_{R})^{2}}
$$

with the Rayleigh range

$$
z_{R} \;=\; \pi \, w_{0}^{2} \,/\, \lambda.
$$

The model places the waist at the **laser origin** (`z = 0`), so the
distance `d` from the laser origin used by the intersection layer is exactly
the axial coordinate `z`. `width_at` returns the full **diameter** `2·w(d)`
— the natural pairing for the circle-of-confusion *diameter* it is combined
with in quadrature. Only the across-stripe width is modelled; the along-fan
intensity profile is out of scope for the MVP.

The trait is the per-component override hook: a measured line-width table or
a collimated (constant-width) beam can implement `WidthModel` and be passed
to `stripe_on_target` in place of `GaussianBeamWidth`.

## `stripe_on_target`

```rust
pub fn stripe_on_target(
    laser:  &LaserPlane,
    target: &TargetEntity,
    width:  &impl WidthModel,
    n_samples: usize,
) -> Result<Option<LaserStripe>>;
```

Two planes — the laser fan plane (normal `n_L`) and the target plane (normal
`n_T`) — meet in a single straight line with direction `n_L × n_T`. The
function:

1. **Two planes → a line.** Solve the joint plane equations for a point on
   the intersection line. Parallel planes (`d ≈ 0`) return `None`.
2. **Clip to the fan.** Liang–Barsky-style cuts against the two angular
   half-planes (`phi = ±half_angle`) and the radial bound (`r ≤ length`).
   Behind-the-laser segments are excluded because the fan only opens
   forward.
3. **Clip to the target rectangle.** Segment-vs-rectangle in the target's
   local frame.
4. **Sample.** The surviving 3D segment is sampled into `n_samples` evenly
   spaced points; each `StripeSample` records its world-space point, the
   straight-line distance from the laser origin, and the width from the
   `WidthModel` at that distance.

**MVP scope: a single straight segment.** Because the MVP target is one
analytic plane, two planes meet in one line, so the laser stripe is **one
straight 3D segment** — no polyline stitching across triangle boundaries.
The output type is still a polyline (`LaserStripe { samples: Vec<StripeSample> }`)
so the projection layer and a future mesh intersection share one interface.

## `project_stripe` and the quadrature pixel width

```rust
pub fn project_stripe(
    stripe: &LaserStripe,
    camera: &CameraEntity,
) -> Result<ProjectedStripe>;

pub struct ProjectedPoint {
    pub pixel:      Point2<f64>,
    pub geom_px:    f64,
    pub defocus_px: f64,
    pub total_px:   f64,    // = sqrt(geom_px^2 + defocus_px^2)
}
```

Each 3D stripe sample is transformed world → camera-local, then projected by
the calibration-rs camera (`CameraParams::build()` →
`Camera::project_point_c`). Samples with camera-frame `z ≤ 0` (behind the
camera) are silently dropped — they are not an error, just unimageable.

The per-vertex pixel width has two independent contributions added **in
quadrature** (the standard combination for independent blur sources):

$$
w_{\text{total}}\;=\;\sqrt{\,w_{\text{geom}}^{2}\,+\,w_{\text{defocus}}^{2}\,}.
$$

- **`geom_px`** — the physical cross-section `w(d)` as the camera sees it.
  Obtained *projectively*: two points offset from the stripe sample by
  `±w(d)/2` along a direction **perpendicular to both the stripe tangent
  and the camera ray** are projected, and `geom_px` is their pixel
  separation. Doing it this way captures perspective foreshortening for an
  obliquely-viewed target exactly, and the offset direction is the
  across-line direction in which the stripe's thickness is most visible.
- **`defocus_px`** — the circle-of-confusion *diameter* in pixels at the
  3D sample point, from the M4 `ThickLens::coc_diameter_px`. Reusing the M4
  model here is the intended design.

The geometric path uses `tangent × view_ray` from the camera's eye to the
sample as the offset direction; when the stripe is (nearly) viewed end-on,
it falls back to any perpendicular to the tangent. Both endpoints of the
`±` offset are projected and their pixel distance is the geometric width.
The whole construction is in
`crates/etendue-core/src/laser/project.rs`'s `geometric_width_px`.

`ProjectedStripe::max_total_px` exposes the maximum width over the
projected vertices, which the [UI's simulated-image
panel](ui.md) uses to scale the rendered band.
