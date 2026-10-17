# Working Volume

The **working volume** of a single-laser-fan triangulation sensor is the set
of points the sensor can simultaneously *illuminate*, *see*, and *focus on*.
For a single planar laser fan and a single camera, every illuminated point
lies on the fan plane, so the working volume reduces to a **2D region on the
laser fan plane**, not a 3D voxel grid. The 3D analysis for rigs with several
camera and laser pairs is
[`voxelized_overlap`](symmetric_rigs.md#n-view-voxel-overlap).

The 2D form is what a designer actually wants to read off ("how big is the
strip of object I can measure with this rig?"), it is cheap enough to
recompute after every parameter change, and it can be drawn directly on top
of the fan.

## API

The functions live in `etendue_core::analysis`.

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

1. **Illumination**: `LaserPlane::contains_in_extent(point_world)`. By
   construction of the parameterisation every interior cell is inside the
   fan; the explicit call covers the boundary samples (and stays robust to
   numerical wobble there).
2. **Visibility**: `Camera::project_point_c` against the sample's
   camera-frame coordinates, then a sensor-rectangle bounds check on the
   returned pixel. `project_point_c` rejects camera-frame `z ≤ 0` (behind
   the camera), which is the most common rejection cause.
3. **Focus**: `ThickLens::coc_diameter_px(point_camera)` against
   `coc_threshold_px`. The default 1.0 px threshold is the classic ~1-pixel
   depth-of-field criterion.

`in_volume` is the conjunction of all three.

## The (phi, r) parameterisation

Sampling in the laser's natural `(phi, r)` parameterisation (row sweeps fan
angle, column sweeps radius) makes the illumination predicate cheap (a pair
of bound checks) and puts every sample on the fan plane by construction.
The grid also tessellates directly into a triangle mesh on the fan.

- `row ∈ [0, rows)` sweeps `phi` from `-half_angle` at `row = 0` to
  `+half_angle` at `row = rows - 1`.
- `col ∈ [0, cols)` sweeps `r` from `0` at `col = 0` to `fan_length` at
  `col = cols - 1`.
- Cells are stored **row-major** (`index = row * cols + col`).

A 128 × 128 grid (≈ 16k samples) is fine enough that the in-volume region's
boundary reads as a clean curve, and cheap enough to recompute on every
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
corners is in-volume (the same rule as colouring a triangle in-volume when
any of its three vertices is). A coarser grid systematically underestimates
the area along the boundary; at 128×128 the error is well below the
precision a designer cares about.

## Depth range

`depth_range_m` returns `(min_z, max_z)` in metres, taken over the
**camera-frame z** of cells with `in_volume = true`. This is the second
headline number alongside the area: the working-volume depth coverage along
the camera's optical axis.

## Default-scene numbers

For the camera and laser of `Scene::default_mvp` (16 mm f/2.8 lens focused
at 0.617 m, 660 nm laser with a 0.30 rad fan half-angle, 0.75 m fan length,
0.28 m baseline), the working-volume analysis at 128 × 128 with the default
threshold reports:

| readout              | value                  |
|----------------------|------------------------|
| area                 | ≈ 4990 mm²             |
| depth-z range        | 603.5 – 629.8 mm       |

Both depend continuously on the camera and laser parameters: changing the
focus distance moves the depth range, changing the f-number opens or shrinks
the area, and changing the Scheimpflug tilt rotates the in-focus band across
the fan.

## Scope note

This is the 2D working volume of one camera and one laser fan, on the laser
plane. For rigs with several pairs, [`voxelized_overlap`](symmetric_rigs.md)
evaluates per-voxel coverage in 3D. The single-fan, single-camera case is
exactly the one the 2D form handles correctly and inexpensively.
