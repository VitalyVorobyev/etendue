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

### Reused from calibration-rs

- `vision-calibration-core` (path dep, no fork or vendor): the four model traits
  (`ProjectionModel` / `DistortionModel` / `SensorModel` / `IntrinsicsModel`), the
  `Camera<S, P, D, Sm, K>` composition, `Pinhole` / `BrownConrady5` /
  `HomographySensor` / `ScheimpflugParams` / `FxFyCxCySkew` / `CameraModel` /
  `CameraParams`, `Ray`, and math aliases. Serde conventions mirrored:
  `#[serde(tag = "type", rename_all = "snake_case")]` + `flatten` + `alias`.

### Verified

- 172 tests pass (129 in `etendue-core`, 42 in `etendue-ui`, 1 doctest).
- Scheimpflug CoC and PoBF physics validated against first-principles hand
  calculations: on-PoBF cancellation exact to machine epsilon (~1e-18); off-axis
  regime (b) c = 51.6529 µm = 14.97185 px matches the textbook formula
  `c = D·f·|z − s_o| / (z·(s_o − f))` independently (M4 kill gate passed).
- `cargo build --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo fmt --all --check`, and `cargo test --workspace` all clean.
