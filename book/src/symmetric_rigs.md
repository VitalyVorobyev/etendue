# Symmetric Rigs and N-View Voxel Overlap

Two related features cover rigs with more than one camera and laser:
a **symmetric-rig builder** that replicates a camera+laser pair around a
rotation axis, and an **N-view voxel-overlap analysis** that counts, per
voxel, how many pairs simultaneously see, illuminate, and focus on it.

## Symmetric-rig builder

A rig's `layout` builds a ring of N camera and laser pairs around the rig axis
(see [Rig layouts](scenes.md#rig-layouts)). The studio's rig builder and
`expandRig` in `@etendue/wasm` lay it out. `voxelized_overlap` below works on an
`etendue-core` `Scene`, pairing its cameras and lasers by index.

## N-view voxel overlap

`voxelized_overlap` (in `etendue_core::analysis`) evaluates a 3D voxel grid,
counting per voxel how many `(camera, laser)` pairs of the rig
simultaneously see, illuminate, and focus on that voxel:

```rust
pub fn voxelized_overlap(
    scene:             &Scene,
    bounds:            VoxelBox,          // axis-aligned world-space bounding box
    resolution:        VoxelResolution,   // (nx, ny, nz) voxel counts
    coc_threshold_px:  f64,               // focus threshold, e.g. DEFAULT_COC_THRESHOLD_PX
    illum_thickness_m: f64,               // thickness of the illuminated band around the fan plane
) -> Result<VoxelOverlap>

pub struct VoxelBox { pub min: Point3<f64>, pub max: Point3<f64> }
pub struct VoxelResolution { pub nx: usize, pub ny: usize, pub nz: usize }
```

Pair `i` is `(scene.cameras[i], scene.lasers[i])` for
`i ∈ 0..min(cameras, lasers)`. Targets are not consulted: the overlap is a
property of the rig and the inspection volume, not of any target inside it.

### Per-voxel predicates

For each pair `(camera_i, laser_i)` and each voxel centre `p`:

1. **Visible**: `p` is in front of the camera (`z > 0` in the camera
   frame), its pinhole projection lands within the sensor rectangle, and
   `Camera::project_point_c` also accepts it (so distortion and the
   Scheimpflug homography agree with the pinhole check).
2. **Illuminated**: `p` lies within `illum_thickness_m / 2` of the laser
   fan plane (`LaserPlane::signed_distance`), in front of the laser, inside
   the fan's angular extent `±half_angle`, and within its radial reach
   `length`. Without the thickness band, any voxel centre not exactly on the
   zero-thickness plane would fail.
3. **In focus**: `ThickLens::coc_diameter_px(p_camera)` ≤
   `coc_threshold_px`.

The voxel's overlap count is the number of pairs where all three predicates
hold. The analysis is `O(nx·ny·nz · n_pairs)`: a 20×20×20 grid with 6 pairs
is 48 000 predicate evaluations and completes in well under a frame.

### VoxelOverlap

```rust
pub struct VoxelOverlap {
    bounds:     VoxelBox,
    resolution: VoxelResolution,
    n_pairs:    usize,
    counts:     Vec<u32>,       // row-major [iz * nx*ny + iy * nx + ix]
}

impl VoxelOverlap {
    pub fn bounds(&self) -> &VoxelBox;
    pub fn resolution(&self) -> &VoxelResolution;
    pub fn n_pairs(&self) -> usize;
    pub fn counts(&self) -> &[u32];
    pub fn voxel_centre(&self, ix: usize, iy: usize, iz: usize) -> Point3<f64>;
    pub fn voxel_count(&self) -> usize;
    pub fn count_at_least(&self, min_count: u32) -> usize;
    pub fn max_overlap(&self) -> u32;
}
```

Counts rather than booleans keep the result composable: a viewer can
colour-code by count, threshold to any minimum agreement (for example
"≥ 4 pairs see this voxel"), or compute coverage statistics.
`count_at_least(n_pairs)` is the number of voxels every pair covers, the
rig's full-agreement working volume; `count_at_least(1)` is the number
covered by any pair.
