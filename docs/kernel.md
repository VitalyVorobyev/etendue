# The optics kernel (`etendue-core`): developer notes

The book's "Models and algorithms" part explains the models to users. This file keeps what only
a developer of `etendue-core` needs: the module map, the API shapes, the legacy kernel's
conventions and how they differ from the physical camera's, and which tests guard what. It
absorbs the dev-only parts of the book pages that were removed for v0.2.0 (`thick_lens.md`,
`gaussian_psf.md`, `scheimpflug_solver.md`, `symmetric_rigs.md`, `working_volume.md`,
`scene_and_geometry.md`).

The rustdoc (`cargo doc --no-deps -p etendue-core`) is the reference; this is the map.

## Two layers

| Layer | Modules | Used by |
|---|---|---|
| **Physical camera and its analyses** (ADR 0008) | `optics::camera` (`PhysicalCamera`), `optics::paraxial`, `from_scene`, `head`, `rig` | the CLI (`load` resolves scenes), `etendue-synth`, `@etendue/wasm`, the studio |
| **Legacy kernel** (the egui era) | `scene` (`Scene`, `CameraEntity`, …), `optics::{coc, thick_lens, psf}`, `analysis` (`defocus_map`, `working_volume`, `voxelized_overlap`), `solver` (Scheimpflug solver), `laser::project` | its own tests; `laser::{plane, width, intersect}` are shared with the product (`etendue-synth::laser`, `laser_render`) |

Nothing outside `etendue-core` calls `ThickLens`, `CameraEntity`, `working_volume`,
`voxelized_overlap`, `solve_scheimpflug` or `project_stripe` (checked with `grep` for v0.2.0).
The product's equivalents are `PhysicalCamera::coc_px`, `Head::map`, `rig::coverage` and
`Head::optimise`.

### Conventions that differ between the layers

| | Legacy (`coc`, `ThickLens`, `CameraEntity`) | `PhysicalCamera` |
|---|---|---|
| camera origin | the rear principal point H′ | the image-plane datum S (mechanical frame M) |
| focus distance | `s_o`, measured from H′ | `D`, measured from S (as a focus scale reads it) |
| principal gap | `g`, with an object at depth `z` at `d_H = z + g` from H (H on the image side of H′) | signed `g = z_H − z_H′`, positive when H is on the object side |
| centre of projection | H′ (a pinhole at the camera origin) | the entrance pupil E (`mech_se3_projection`) |
| aperture | at the principal planes | entrance pupil `f/N`, exit pupil `m_p f/N` |
| `fx` | `sync_intrinsics_from_physical`: `f / pitch` (wrong away from infinity focus: 9 % at 0.3 m for 25 mm) | `m0 (d − e) / pitch` (focus breathing) |
| sensor tilt | `coc`'s `tau_x` is the negative of calibration-rs's `tilt_x`; `ThickLens` converts with `optics::coc_tilt` | calibration-rs's angles throughout |

`tests/camera_frames.rs` pins the legacy model against `PhysicalCamera` where they overlap
(G9.1, G9.4: `docs/measurements/g9_1_g9_4_camera_model.md`).

## `PhysicalCamera` (`optics::camera`)

`PhysicalCamera::new(lens, pixel_pitch_m, focus_distance_m, tilt, as_built)`; `as_built` supplies
`cx, cy` and the distortion. Accessors: `cardinal_points`, `image_distance_m` (`v0`),
`object_distance_m` (`d`), `magnification`, `pupil_magnification`, `working_f_number`,
`entrance_pupil_diameter_m`, `exit_pupil_diameter_m`, `sensor_plane`, `mech_se3_projection`,
`pinhole` (the derived `CameraParams`). Geometry: `project`, `backproject`, `image_of`,
`object_of`, `plane_of_best_focus`. Blur: `coc_m`, `coc_px`, `airy_diameter_m(λ)`,
`depth_of_field(coc_limit_m)`, `object_pixel_m(z)`.

- The tilted-sensor homography is decomposed into calibration-rs's `K · H(τ′)`; with a two-axis
  tilt and `m_p ≠ 1` a roll ψ of the projection frame (found by bisection) makes `K` upper
  triangular, leaving a skew.
- `from_scene::resolve` turns a scene into projection frames (each physical camera at E, its
  `params` the derived pinhole, `optics` removed, attached entities re-expressed);
  `check_derived` reports stored `fx`/`fy` beyond a relative `DERIVED_TOLERANCE` (1e-9), skew
  beyond 1e-9·fx, tilt beyond 1e-9 rad.
- `optics::paraxial` is the reference trace the closed forms are tested against (exit pupil found
  by imaging the entrance pupil, not from the closed-form `m_p`), and the source of the section's
  ray fans.

## `head` and `rig`

- `Head::from_design(&HeadDesign)`; `sample(a, b)` → `Sample { visible, pixel, coc_px, airy_px,
  blur_px, lateral_m_per_px, depth_m_per_px, triangulation_rad, beam_px }`; `map(window, grid)`
  (row by row from `near`, each row from `−width/2`); `scheimpflug()` → `Setting`;
  `optimise(window, Search { grid, f_number_range, tilt_free })` → `Optimum`; `section(window,
  probes, blur_limit_px)` → `Section` (2-D traces, rays, hinge points, depth-of-field polylines,
  and the camera in 3-D as `solid`); `apply(design, setting)`.
- The optimiser's cost is the worst `blur_px` over the visible samples, or `1e6 + hidden` when
  any sample is hidden; tilts at or beyond 0.5 rad and f-numbers outside the range cost `+∞`.
  Initial steps: 0.01 rad per tilt, 2 % of the focus, 20 % of the f-number; Nelder–Mead with
  `cost_tolerance` 1e-9 and 400 iterations a run, restarted from the best point (at most 8 runs)
  until a run stops improving by a relative 1e-9.
- `rig::coverage(layout, grid, criteria)` → `Coverage { views, lit }` (u8 counts, x fastest);
  `Grid` allows 1–512 voxels per axis; `Criteria { max_blur_px: 2, wavelength_nm: 550 }` by
  default. `rig::sharpness` gives each camera's `Sharpness::{Everywhere, Nowhere, Between}`.

Gates: G10.1–G10.3 (`g10_head_kernel.md`), G11.1–G11.3 (`g11_rigs.md`).

## Legacy kernel reference

### The scene (`scene`, `geom`)

`Scene { cameras: Vec<CameraEntity>, lasers: Vec<LaserEntity>, targets: Vec<TargetEntity>,
mesh_targets: Vec<MeshTarget> }`, every entity with `pose: Isometry3<f64>` = `world_se3_self`
(ADR 0002 keeps the name). All have validating `new` constructors.

- `CameraEntity { pose, params: CameraParams, resolution, optics: PhysicalOptics, frustum_near,
  frustum_far }`, `PhysicalOptics { effective_focal_length_m, f_number, focus_distance_m,
  principal_gap_m, pixel_pitch_m }`. It keeps `CameraParams`, not a built `CameraModel`, because
  `build()` compiles a Scheimpflug sensor into a homography and loses the tilt angles (CLAUDE.md
  gotcha 2). `thick_lens()` builds the `ThickLens`.
- `LaserEntity { pose, fan_half_angle, fan_length, wavelength_nm, beam_waist_m,
  waist_distance_m }`: fan in local `x = 0`, central ray +z, a ray at fan angle φ has local
  direction `(0, sin φ, cos φ)`.
- `TargetEntity { pose, width, height }`: a rectangle in local `z = 0`, normal +z.
  `MeshTarget { pose, mesh: TriMesh }`.
- `Scene::default_mvp()`: a target upright at `(0, 0, 0.30)` facing +Y (0.15 × 0.12 m); a laser at
  `(0, −0.60, 0.30)` aimed along +Y (half-angle 0.30 rad, 0.75 m, 660 nm, `w0` 0.25 mm); a camera
  at `(0.28, −0.55, 0.30)` aimed at the target centre (1280 × 1024 at 3.45 µm, 16 mm f/2.8 focused
  at the 0.617 m standoff).
- `geom::TriMesh { vertices, normals (per vertex), indices }` with `unit_cube(edge)` and
  `quad(w, h)`; the frustum is not a type: back-project the four sensor corners and scale by
  `frustum_near`/`frustum_far`.

### Defocus (`optics::coc`, `thick_lens`, `psf`)

- `coc`: `image_distance`, `sensor_distance`, `pobf_tilt_tan`, `coc_diameter_at_sensor`,
  `coc_to_pixels`, `PlaneOfBestFocus`. Derivation: `docs/derivations/scheimpflug_pobf.md`
  (worked numbers there are the tests' expectations: `c = 14.9719 px` for 25 mm f/4 at 0.30 m,
  a point at 0.33 m, 3.45 µm; the 5° tilt PoBF at 43.90°).
- `ThickLens::new(params, f, N, s_o, g, pitch)`; `coc_diameter(&Point3)` takes a 3-D point
  because under tilt the blur depends on the full position; `coc_diameter_px`;
  `plane_of_best_focus`; `scheimpflug_tilt(&CameraParams)` reads `(tilt_x, tilt_y)` (identity and
  raw homographies count as untilted).
- `psf`: `GAUSSIAN_FWHM_PER_SIGMA = 2·√(2 ln 2) ≈ 2.3548`, `coc_to_gaussian_sigma(coc)` (FWHM
  matching; 0 for non-positive or non-finite input). Tests: the constant to machine precision, the
  round trip, bad inputs.

### Laser (`laser`)

- `LaserPlane::from_entity` (world frame: `origin`, `normal` = local +x, `central` = +z,
  `in_plane` = +y, `half_angle`, `length`); `ray_direction(φ)`, `signed_distance`,
  `fan_angle_of`, `radius_of`, `contains_in_extent` (angle and radius, with a 1e-9 tolerance).
- `WidthModel::width_at(distance_m)` returns the full `1/e²` diameter; `GaussianBeamWidth::new(w0,
  λ)`, `from_laser`, `with_waist_distance(d0)`. The trait is the hook for a measured width table.
- `stripe_on_target(plane, target, width_model, samples)`: plane ∩ plane, Liang–Barsky clip to the
  fan's angular half-planes and radius, clip to the rectangle, sample (`StripeSample { point,
  distance_from_laser_m, width_m }`). `stripe_segments_on_mesh` cuts per triangle;
  `stripe_on_cylinder` handles the PuzzlePole (used by `etendue-synth::laser`).
- `project_stripe(stripe, camera)` → `ProjectedPoint { pixel, world_m, geom_px, defocus_px,
  total_px = √(geom² + defocus²) }`; `geom_px` projects `±w/2` offsets along `tangent × view_ray`.
  `ProjectedStripe::max_total_px`, `resolution_mm_per_px`, `resolution_summary_mm_per_px`.

### Analyses (`analysis`)

- `working_volume(camera, laser, rows, cols, coc_threshold_px)`: the 2-D working volume on the
  laser plane, sampled in `(φ, r)` (rows sweep φ from −α to +α, columns r from 0 to the fan
  length, row-major). Predicates per cell: illuminated (`contains_in_extent`), visible
  (`project_point_c` and the sensor rectangle), in focus (`ThickLens::coc_diameter_px` ≤ the
  threshold, default `DEFAULT_COC_THRESHOLD_PX` = 1). `area_m2` sums `r_mid · dφ · dr` over cells
  with any in-volume corner; `depth_range_m` is the camera-frame z range. On `default_mvp` at
  128 × 128: about 4990 mm², z 603.5–629.8 mm.
- `voxelized_overlap(scene, VoxelBox, VoxelResolution, coc_threshold_px, illum_thickness_m)`:
  pair `i` is `(cameras[i], lasers[i])`; a voxel counts a pair when it is visible, within
  `illum_thickness_m / 2` of the fan plane (in front, within angle and reach) and in focus.
  `VoxelOverlap::{counts, count_at_least, max_overlap, voxel_centre}`. The product's
  `rig::coverage` replaces it for scene rigs.
- `defocus_map`: per-target CoC samples.

### The Scheimpflug solver (`solver`)

`solve_scheimpflug(camera, laser, &SolverSpec { d_opt, depth_range, grid: SampleGrid })` →
`SolverResult { tau_x, tau_y, s_o, max_coc_px, n_valid_samples, converged, iterations }`. The
camera is not mutated.

- Cost: the worst CoC over the fan sampled on `SampleGrid` (default 32 × 32, `(φ, r)`), keeping
  samples in front of the camera, on the sensor and at a camera depth in `depth_range`; `+∞` when
  none survive, for `s_o ≤ f`, or a tilt at or beyond π/2. `d_opt` is design intent; the window
  drives the cost.
- `solver::nelder_mead`: reflection 1, expansion 2, contraction ½, shrink ½; stops when the
  standard deviation of the simplex's costs is ≤ 1e-6 or after 500 iterations; restarted from the
  best point (at most 8 runs). Initial steps 1° per tilt and max(5 % of `d_opt`, 5 mm) in focus.
  Tilts are searched in `coc`'s convention and returned in calibration-rs's.
- `SolverError` (`#[non_exhaustive]`): `InvalidSpec`, `InvalidCamera`, `SolverFailure`.
- Tests: `fronto_parallel_fan_drives_tilt_to_zero_and_focus_to_d_opt`,
  `solving_lowers_worst_case_coc`, and `assert_reproduces_argmin` pinning the optima argmin found
  before the in-tree port (G9.2, `g9_2_solver.md`). No `getrandom` in the wasm graph (G9.3).

## Tests that guard the models

| Test | Guards |
|---|---|
| `crates/etendue-core/tests/camera_frames.rs` | `PhysicalCamera` vs the paraxial trace (25 cases), the thin-lens limit, refocusing, the legacy model where it overlaps (G9.1, G9.4) |
| `crates/etendue-core/tests/resolve_scene.rs` | `from_scene::resolve`: projection frames, attached entities, idempotence |
| `crates/etendue-core/tests/head.rs` | the Scheimpflug setting and hinge rule, the optimiser on a thin window, depth-of-field polylines, `solid` (G10.2, G10.3) |
| `crates/etendue-core/tests/rig_coverage.rs` | two-view volume against the analytic frusta (G11.2), sharp ranges |
| `optics::coc`, `thick_lens`, `psf`, `solver` unit tests | the derivation's worked numbers, conversions, the solver |
