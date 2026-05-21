# Symmetric Rigs and N-View Voxel Overlap (M10)

M10 added two related features: a **symmetric-rig builder** that replicates
a camera+laser pair around a rotation axis, and an **N-view voxel-overlap
analysis** that counts, per voxel, how many pairs simultaneously see and
illuminate it.

## Symmetric-rig builder

`Scene::triangulation_ring` generates N rotated copies of a template
`(camera, laser)` pair:

```rust
pub fn triangulation_ring(
    n:                usize,
    axis:             Vector3<f64>,   // world-space rotation axis (normalised internally)
    target_center:    Point3<f64>,    // pivot point
    camera_template:  &CameraEntity,
    laser_template:   &LaserEntity,
) -> Result<(Vec<CameraEntity>, Vec<LaserEntity>)>
```

Pair `i` is the template rigidly rotated by `2π · i / n` around the line
through `target_center` in the direction `axis`. The baseline distance,
fan half-angle, and lens parameters are preserved by the rigid rotation.

The parameter panel's **Symmetric rig (M10)** collapsible exposes:

- **N pairs** slider (2–8)
- **Axis** radio: `+X`, `+Y`, `+Z`
- **Generate ring (replaces pairs)** button — replaces the scene's cameras
  and lasers with the N generated pairs (the targets and mesh_targets are
  unchanged)

### Tests

Two numerical tests:

- `triangulation_ring_has_n_fold_rotational_symmetry` — each consecutive
  pair is exactly `2π/n` apart by the `angular_step_rad` predicate.
- `triangulation_ring_preserves_distance_to_target_centre` — every
  generated camera sits at the same world distance to `target_center` as
  the template (the rigid rotation preserves distances).

## N-view voxel overlap

`voxelized_overlap` evaluates a 3D voxel grid, counting per voxel how many
`(camera, laser)` pairs of the rig simultaneously see, illuminate, and
focus on that voxel:

```rust
pub fn voxelized_overlap(
    cameras:  &[CameraEntity],
    lasers:   &[LaserEntity],
    bounds:   VoxelBox,          // axis-aligned world-space bounding box
    res:      VoxelResolution,   // (nx, ny, nz) voxel counts
) -> Result<VoxelOverlap>
```

### Per-voxel predicates

For each pair `(camera_i, laser_i)` and each voxel centre:

1. **Illuminated** — `LaserPlane::contains_in_extent(centre)`.
2. **Visible** — `Camera::project_point_c(centre)` lands within the sensor
   rectangle and `z > 0`.
3. **In focus** — `ThickLens::coc_diameter_px(centre)` ≤
   `DEFAULT_COC_THRESHOLD_PX` (1.0 px).

The voxel's overlap count is the number of pairs where all three predicates
hold. The analysis is `O(nx·ny·nz · n_pairs)` — at the default 20×20×20
grid and 6 pairs, roughly 48 000 `f64` evaluations, completing in well
under one frame.

### VoxelOverlap

```rust
pub struct VoxelOverlap {
    counts:   Vec<u32>,       // row-major [iz * nx*ny + iy * nx + ix]
    bounds:   VoxelBox,
    res:      VoxelResolution,
}

impl VoxelOverlap {
    pub fn voxel_centre(&self, ix: usize, iy: usize, iz: usize) -> Point3<f64>;
    pub fn counts(&self) -> &[u32];
    pub fn resolution(&self) -> &VoxelResolution;
    pub fn bounds(&self) -> &VoxelBox;
}
```

### Viewport rendering

`voxel_overlap_mesh` in `viewport::scene` builds one axis-aligned cube per
voxel whose count is ≥ `min_overlap`, packed into a single `TriMesh` with
flat per-face normals. The cubes are 85 % of the voxel edge size so adjacent
voxels read as distinct rather than fusing into a solid block.

The UI's **N-view voxel overlap** checkbox (visible when the scene has ≥ 2
pairs) and **min agreeing pairs** slider control the threshold; changing
either triggers a `rebuild_scene`.

### Displayed-pair selector

A separate `displayed_pair` slider (shown when there are ≥ 2 pairs) controls
which pair drives the defocus heatmap, the working-volume patch, and the
simulated-image panel. The Scheimpflug solver section acts on the same
displayed pair (F10 fix).
