# Working Volume

The **working volume** of a single-laser-fan triangulation sensor is the set
of points the sensor can simultaneously *illuminate*, *see*, and *focus on*.
For the MVP — a single planar laser fan plus a single camera — every
illuminated point lies on the fan plane, so the working volume reduces to a
**2D region on the laser fan plane**, not a 3D voxel grid. (3D voxelization
is post-MVP — see [the roadmap](roadmap.md).)

The MVP picks the 2D form for three reasons: it is what the designer
actually wants to read off ("how big is the strip of object I can measure
with this rig?"), it is cheap enough to recompute on every slider drag, and
it is what the UI can paint directly on top of the rendered fan.

## API

```rust
pub const DEFAULT_COC_THRESHOLD_PX: f64 = 1.0;

pub fn working_volume(
    camera: &CameraEntity,
    laser:  &LaserEntity,
    rows:   usize,            // fan-angle resolution, >= 2
    cols:   usize,            // radial resolution, >= 2
    coc_threshold_px: f64,
) -> Result<WorkingVolume>;

pub struct WorkingVolumeCell {
    pub point_world: Point3<f64>,
    pub depth_m:     Option<f64>,    // camera-frame z; None if behind camera
    pub coc_px:      Option<f64>,    // None if unprojectable
    pub illuminated: bool,
    pub visible:     bool,
    pub in_focus:    bool,
    pub in_volume:   bool,           // all three predicates hold
}

pub struct WorkingVolume { /* row-major Vec<WorkingVolumeCell> + grid metadata */ }

impl WorkingVolume {
    pub fn rows(&self) -> usize;
    pub fn cols(&self) -> usize;
    pub fn half_angle_rad(&self) -> f64;
    pub fn fan_length_m(&self)   -> f64;
    pub fn coc_threshold_px(&self) -> f64;
    pub fn cells(&self) -> &[WorkingVolumeCell];
    pub fn get(&self, row: usize, col: usize) -> Option<&WorkingVolumeCell>;
    pub fn phi(&self, row: usize)    -> f64;   // fan-angle of row
    pub fn radius_m(&self, col: usize) -> f64; // radius of column

    pub fn area_m2(&self)        -> f64;
    pub fn depth_range_m(&self)  -> Option<(f64, f64)>;
    pub fn in_volume_count(&self) -> usize;
}
```

## The three predicates

For each `(phi, r)` sample on the fan plane the function evaluates:

1. **Illumination** — `LaserPlane::contains_in_extent(point_world)`. By
   construction of the parameterisation every interior cell is inside the
   fan; the explicit call covers the boundary samples (and stays robust to
   numerical wobble there).
2. **Visibility** — `Camera::project_point_c` against the sample's
   camera-frame coordinates, then a sensor-rectangle bounds check on the
   returned pixel. `project_point_c` rejects camera-frame `z ≤ 0` (behind
   the camera), which is the most common rejection cause.
3. **Focus** — `ThickLens::coc_diameter_px(point_camera)` against
   `coc_threshold_px`. The default 1.0 px threshold is the classic ~1-pixel
   depth-of-field criterion, matching the green band of the on-target
   defocus heatmap so the two visualizations read the same scale.

`in_volume` is the conjunction of all three.

## The (phi, r) parameterisation

Sampling in the laser's natural `(phi, r)` parameterisation (row sweeps fan
angle, column sweeps radius) makes the illumination predicate cheap (a pair
of bound checks) and lets every sample lie on the fan plane by
construction. The grid lays out cleanly for the UI to tessellate as a
triangle mesh on the fan.

- `row ∈ [0, rows)` sweeps `phi` from `-half_angle` at `row = 0` to
  `+half_angle` at `row = rows - 1`.
- `col ∈ [0, cols)` sweeps `r` from `0` at `col = 0` to `fan_length` at
  `col = cols - 1`.
- Cells are stored **row-major** (`index = row * cols + col`).

The MVP UI uses 128 × 128 (≈ 16k samples) — fine enough that the in-volume
region's boundary reads as a clean curve, cheap enough to recompute on every
scene change.

## Area: the fan-plane Jacobian

`area_m2` integrates the cell area over cells whose centroid is in the
working volume. The fan parameterisation's Jacobian on the planar fan is
`r·dphi·dr`:

$$
A \;=\; \sum_{\text{cells in volume}} r_{\text{mid}} \,\cdot\, d\phi \,\cdot\, dr,
\qquad
d\phi = \tfrac{2\,\theta_{1/2}}{\text{rows}-1},\;\;
dr     = \tfrac{L_{\text{fan}}}{\text{cols}-1}.
$$

The midpoint radius `r_mid` is the average of the two sampling radii
spanning the cell, and the cell counts as in-volume if **any** of its four
corners is in-volume — matching the UI's tessellation rule (a triangle is
colored in-volume if any of its three vertices is). A coarser grid
systematically underestimates the area along the boundary; at 128×128 the
error is well below the precision a designer cares about.

## Depth range

`depth_range_m` returns `(min_z, max_z)` in metres, taken over the
**camera-frame z** of cells with `in_volume = true`. This is the second
headline number alongside the area: the working-volume depth coverage along
the camera's optical axis.

## Default-scene numbers

Re-using the default MVP scene's camera + laser (16 mm f/2.8 lens focused at
0.617 m, 660 nm laser with a 0.30 rad fan half-angle, 0.75 m fan length, the
0.28 m baseline), the working-volume analysis at 128 × 128 reports:

| readout              | value                  |
|----------------------|------------------------|
| area                 | ≈ 4990 mm²             |
| depth-z range        | 603.5 – 629.8 mm       |

These are the numbers the parameter panel surfaces alongside the rendered
patch. They depend continuously on every M3 / M4 / M5 slider — drag the
focus distance and the depth range follows; drag the f-number and the area
opens or shrinks; drag the Scheimpflug tilt and the in-focus band rotates
across the fan.

## Scope note

This is the **2D MVP working volume** on the laser plane. The full 3D
voxel-grid analysis — per-voxel visibility from each of `N` cameras, multi-
view triangulation angle, per-voxel resolution — is post-MVP. The
single-fan / single-camera case is exactly the one the 2D specialization
handles correctly and inexpensively, and the post-MVP voxel work additively
extends, not replaces, it.
