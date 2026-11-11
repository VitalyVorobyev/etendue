# The optics kernel (`etendue-core`): developer notes

The book's "Models and algorithms" part explains the models to users. This file keeps what only
a developer of `etendue-core` needs: the module map, the API shapes, what the removed legacy
kernel was, and which tests guard what. It
absorbs the dev-only parts of the book pages that were removed for v0.2.0 (`thick_lens.md`,
`gaussian_psf.md`, `scheimpflug_solver.md`, `symmetric_rigs.md`, `working_volume.md`,
`scene_and_geometry.md`).

The rustdoc (`cargo doc --no-deps -p etendue-core`) is the reference; this is the map.

## The modules

| Modules | Used by |
|---|---|
| `optics::camera` (`PhysicalCamera`, ADR 0008), `optics::paraxial`, `from_scene`, `head`, `rig` | the CLI (`load` resolves scenes), `etendue-synth`, `@etendue/wasm`, the studio |
| `scene` (`LaserEntity`, `TargetEntity`, `MeshTarget`), `laser::{plane, width, intersect}`, `geom` | `etendue-synth::laser`, `laser_render` |
| `solver::nelder_mead`, `library` | `head::optimise`; library content hashes |

### The egui-era kernel, removed in P19f

The first kernel (the egui app's, before ADR 0008) modelled a camera as `CameraEntity` with a
`ThickLens` whose origin was the rear principal point H′, and computed blur with the thick-lens
CoC formulas of `docs/derivations/scheimpflug_pobf.md` (`optics::coc`, the Gaussian `psf`). Its
analyses were the working volume, the defocus map and the voxelised overlap, its designer the
Scheimpflug solver (G9.2), and `laser::project` imaged stripes through `CameraEntity`. Once
`PhysicalCamera` replaced the camera model nothing outside `etendue-core`'s own tests used any of
it, and P19f (v0.3.0) removed it: about 4,800 lines.

| Removed | The product's equivalent |
|---|---|
| `CameraEntity`, `ThickLens`, `optics::coc`, `psf` | `PhysicalCamera` (`coc_m`, `coc_px`, `plane_of_best_focus`) |
| `analysis::working_volume`, `defocus_map` | `Head::map` |
| `analysis::voxelized_overlap` | `rig::coverage` |
| `solver::solve_scheimpflug` | `Head::scheimpflug`, `Head::optimise` |
| `laser::project` | `etendue-synth`'s stripe images through the physical camera |

Two conventions of the old layer still matter when reading its derivation or old measurements:
its camera origin was H′ with the focus distance `s_o` measured from H′ (the physical camera's is
the image-plane datum S, `D` from S), and its `tau_x` was the negative of calibration-rs's
`tilt_x`. The physical camera uses calibration-rs's angles throughout. `tests/head.rs` keeps the
derivation's plane-of-best-focus closed form as an independent check of the hinge rule.

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

## The laser line (`laser`, `scene`)

- `LaserEntity { pose, fan_half_angle, fan_length, wavelength_nm, beam_waist_m,
  waist_distance_m }`: fan in local `x = 0`, central ray +z, a ray at fan angle φ has local
  direction `(0, sin φ, cos φ)`. `TargetEntity { pose, width, height }`: a rectangle in local
  `z = 0`, normal +z; `MeshTarget { pose, mesh: TriMesh }`. Poses are `world_se3_self`.
- `LaserPlane::from_entity` (world frame: `origin`, `normal` = local +x, `central` = +z,
  `in_plane` = +y, `half_angle`, `length`); `signed_distance`, `fan_angle_of`, `radius_of`,
  `contains_in_extent` (angle and radius, with a 1e-9 tolerance).
- `WidthModel::width_at(distance_m)` returns the full `1/e²` diameter; `GaussianBeamWidth::new(w0,
  λ)`, `from_laser`, `with_waist_distance(d0)`. The trait is the hook for a measured width table.
- `stripe_on_target(plane, target, width_model, samples)`: plane ∩ plane, Liang–Barsky clip to the
  fan's angular half-planes and radius, clip to the rectangle, sample (`StripeSample { point,
  distance_from_laser_m, width_m }`). `stripe_segments_on_mesh` cuts per triangle;
  `stripe_on_cylinder` handles the PuzzlePole (used by `etendue-synth::laser`).

## Tests that guard the models

| Test | Guards |
|---|---|
| `crates/etendue-core/tests/camera_frames.rs` | `PhysicalCamera` vs the paraxial trace (25 cases), the thin-lens limit, refocusing (G9.4) |
| `crates/etendue-core/tests/resolve_scene.rs` | `from_scene::resolve`: projection frames, attached entities, idempotence |
| `crates/etendue-core/tests/head.rs` | the Scheimpflug setting and hinge rule, the optimiser on a thin window, depth-of-field polylines, `solid` (G10.2, G10.3) |
| `crates/etendue-core/tests/rig_coverage.rs` | two-view volume against the analytic frusta (G11.2), sharp ranges |
| `solver::nelder_mead` unit tests | the minimiser on its test functions |
