# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-05-20

### Added

- **Workspace skeleton (M0)** — `etendue-core` + `etendue-ui` crates; resolver "3",
  edition 2024; nalgebra 0.34 (hard-pinned to match the calibration-rs path dependency);
  egui 0.34.2 / egui-wgpu 0.34 / egui-winit 0.34 / egui_plot 0.35.0 / wgpu 29.0.3 /
  winit 0.30.13; `debug = 1` dev profile; `opt-level = 2` for all dependencies.

- **3D viewport (M1)** — hand-written wgpu render loop inside egui; depth buffer;
  orbit / pan / zoom camera; grid + world axes; flat-shaded + wireframe meshes;
  line and point primitives; opaque + translucent two-pass render pipeline.

- **Scene and entities (M2)** — `CameraEntity` / `LaserEntity` / `TargetEntity` with
  `Isometry3<f64>` poses; camera frustum built from `Camera::backproject_pixel`; translucent
  laser fan; shaded target quad; `Scene::default_mvp` triangulation rig.

- **Parameter panel, reactivity, JSON I/O, and bank schema (M3)** — egui side panel with
  6-DOF pose editors; same-frame scene rebuild on slider edit; Scene ⇄ JSON via native
  `rfd` dialogs; `bank::schema::{SensorSpec, LensSpec, LaserSpec}` with tagged enums
  mirroring calibration-rs serde conventions
  (`#[serde(tag = "type", rename_all = "snake_case")]` + `flatten` + `alias`);
  9 seed component JSON files.

- **Defocus physics and heatmap (M4)** — `optics::thick_lens::ThickLens` (wraps a
  calibration-rs `CameraModel`; clean `coc_diameter` / `coc_diameter_px` interface
  designed for future promotion to a calibration-rs `ApertureModel<S>` trait);
  Scheimpflug plane-of-best-focus and off-axis circle-of-confusion derived from first
  principles (derivation in `docs/derivations/scheimpflug_pobf.md`);
  `analysis::defocus_map`; per-vertex-color 64×64 heatmap on the target quad with a
  green in-focus band (CoC ≤ 1 px) grading through yellow to red; physical optics
  sliders (focal length mm, f-number, focus distance m, principal-plane gap mm)
  replacing the M3 pixel-based focal slider — `fx` / `fy` are now derived via
  `CameraEntity::sync_intrinsics_from_physical`.

- **Laser line projection and simulated-image panel (M5)** —
  `laser::{LaserPlane, GaussianBeamWidth, stripe_on_target, project_stripe}`;
  plane∩plane intersection with fan-wedge + radial-disc + target-rectangle clipping;
  projection through `Camera::project_point_c` with per-point pixel width =
  √(geom² + defocus²) (geom_px from offset-point projection across the stripe,
  defocus_px from `ThickLens::coc_diameter_px`); `egui_plot` simulated-image bottom
  panel with width-encoded polygon band.

- **Working volume and MVP assembly (M6)** — `analysis::working_volume` (sampled
  128×128 grid on the laser plane; three predicates: illuminated, visible, in-focus at
  CoC ≤ 1 px); translucent cyan working-volume patch in the viewport; area (mm²) and
  depth-range (mm) readouts; show / hide toggles; default scene opens with both overlays
  on.

- **UI stability and viewport polish (M7)** — egui / wgpu / winit version lock at the
  current pinned set; camera-anatomy overlay in the 3D viewport (M8 camera body +
  sensor plane wireframe); render loop hardening for multi-monitor / high-DPI setups.

- **Camera anatomy renderer (M8)** — 3D visualization of the camera's physical body
  and sensor plane in the viewport; lens principal-plane markers; the anatomy overlay
  is toggled alongside the frustum wireframe.

- **Scheimpflug solver (M9)** — `solver::solve_scheimpflug` finds the optimal
  sensor-tilt `(τx, τy)` and focus distance that minimise worst-case circle of
  confusion across a user-defined `[d_min, d_max]` depth window; Nelder-Mead via
  `argmin 0.11` with `default-features = false` (no second nalgebra); convergence
  tests against analytic fronto-parallel optimum; solver section wired into the
  parameter panel with **Apply** button.

- **Symmetric rigs and N-view voxel overlap (M10)** — `Scene::triangulation_ring`
  builds N camera+laser pairs rotationally symmetric about a world axis; the M10
  parameter-panel section exposes N, axis, and **Generate**; `analysis::voxelized_overlap`
  evaluates per-voxel visible/illuminated/focused predicates across all N pairs and
  counts how many pairs agree; viewport renders agreeing voxels as a translucent cloud;
  N-fold symmetry and distance-invariance tests pass.

- **Gaussian PSF model** — `optics::psf::coc_to_gaussian_sigma` converts the
  geometric circle-of-confusion diameter to a FWHM-matched Gaussian sigma;
  `GAUSSIAN_FWHM_PER_SIGMA` constant; used by the simulated-image panel to render
  a physically-motivated Gaussian halo around the laser stripe.

- **Triangle-mesh kernel and mesh laser intersection** — `geom::TriMesh` with
  `unit_cube` / `quad` primitives and index/normal-count validation; `laser::intersect::
  stripe_segments_on_mesh` for fan-plane × triangle-mesh intersection (fan-wedge
  clipping, per-triangle stripe segments); `scene::MeshTarget` entity with serde
  support; viewport renders mesh targets and mesh laser stripes; parameter panel
  exposes an "Add cube mesh target" button.

### Reused from calibration-rs

- `vision-calibration-core` (path dep, no fork or vendor): the four model traits
  (`ProjectionModel` / `DistortionModel` / `SensorModel` / `IntrinsicsModel`), the
  `Camera<S, P, D, Sm, K>` composition, `Pinhole` / `BrownConrady5` /
  `HomographySensor` / `ScheimpflugParams` / `FxFyCxCySkew` / `CameraModel` /
  `CameraParams`, `Ray`, and math aliases. Serde conventions mirrored:
  `#[serde(tag = "type", rename_all = "snake_case")]` + `flatten` + `alias`.

### Verified

- **205** tests pass (**152** in `etendue-core`, **53** in `etendue-ui`, 0 doctests).
- Scheimpflug CoC and PoBF physics validated against first-principles hand
  calculations: on-PoBF cancellation exact to machine epsilon (~1e-18); off-axis
  regime (b) c = 51.6529 µm = 14.97185 px matches the textbook formula
  `c = D·f·|z − s_o| / (z·(s_o − f))` independently (M4 kill gate passed).
- M9 solver convergence test: Nelder-Mead reaches τ ≈ 0 and s_o ≈ focus distance for
  a fronto-parallel target (analytic optimum); worst-case-CoC-improvement test passes.
- M10 ring builder: N-fold symmetry and distance-invariance to target centre verified
  for N = 4, 6, 8.
- Mesh intersection contract: per-triangle stripe segments sum to analytic planar
  stripe length within tolerance.
- `cargo build --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo fmt --all --check`, and `cargo test --workspace` all clean.
