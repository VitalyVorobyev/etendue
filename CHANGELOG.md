# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Pivot P1: scene schema, kinematics, baking (headless)

#### Added

- **`etendue-scene`** — versioned JSON documents: `SceneSpec`, `ScenarioSpec`,
  `BakedScenario`, and `RobotManifest` (`robot.json`).
  - `SceneSpec` is a frame tree of robots, rigs, cameras, lasers, lights,
    targets, and parts (ADR 0002). Cameras are calibration-rs `CameraParams`;
    boards use the calibration-rs dataset `TargetSpec`.
  - `FrameGraph` checks for dangling parents and cycles, orders frames
    topologically, and resolves `world_se3_frame`.
  - Validation collects every issue. Poses must be unit quaternions to 1e-9.
  - JSON Schemas are behind the `schemars` feature and committed in `schemas/`.
  - Property tests check the JSON round trip bit-for-bit.
- **`etendue-kinematics`**, the only home of FK and IK.
  - `RobotModel` is a URDF (via `urdf-rs` 0.10) bound to a manifest. The base
    may be a REP-199 `base` link that hangs off the chain.
  - Custom serial-chain FK on nalgebra 0.34, plus the geometric Jacobian.
  - Damped-least-squares IK with deterministic restarts.
  - Analytic OPW IK (`rs-opw-kinematics`, default features off) behind the
    `opw` feature. It is validated against etendue's own FK.
  - `compile`/`bake` scenarios: synchronised trapezoidal `ptp_joints` and
    `ptp_pose`, spline-timed Cartesian `lin` with exact derivative bounds, and
    stop-and-shoot `capture` and `wait`. Property tests check that joint
    limits are never exceeded and that captures happen at rest.
- **`etendue-cli`** (binary `etendue`): `etendue validate <scene> [<scenario>]`
  and `etendue bake <scene> <scenario> -o <baked.json>`.
  - Example scenes: `examples/eye_in_hand_ur5e/` and
    `examples/eye_to_hand_ur5e/`.
- **Robot assets.** `tools/robot-assets` is a uv project that builds
  `assets/robots/<id>/` from upstream descriptions pinned by SHA:
  - UR5e (BSD-3-Clause) and ABB IRB 1200-5/0.90 (Apache-2.0, OPW);
  - xacro → URDF, per-link GLB meshes (git-ignored, regenerated), and
    `robot.json` with licences and limit provenance.
- **Pinocchio FK fixtures** (`tools/fixtures/`).
- **Gates**, recorded in `docs/measurements/`:
  - G1.1: FK vs Pinocchio ≤ 1.4e-15 on 10k configurations.
  - G1.2: OPW 0 failures, max 5.5e-10 m; DLS residuals ≤ 1e-10, with a
    failure rate of 0 % from near seeds and 0.1–0.35 % from random seeds.
  - G1.3: mesh round trip ≤ 3e-8 m.
- **`xtask`**: `cargo xtask emit-schemas [--check]` and
  `cargo xtask check-layering`, which enforces the ADR 0001 dependency rules
  and the single nalgebra.
  - CI job `checks` runs them, plus `--all-features` tests per crate and a CLI
    smoke test.

#### Changed

- The `etendue-ui` binary is renamed `etendue` → `etendue-ui`, because
  `etendue` is now the CLI. Run the GUI with `cargo run -p etendue-ui`.
- `serde_json` has `float_roundtrip` enabled workspace-wide. The default
  parser can be off by one ULP, which the scene round-trip property test
  caught.
- etendue crates are declared as workspace dependencies with path and
  version, so dependents stay publishable.

#### Security

- `anyhow` 1.0.104 fixes RUSTSEC-2026-0190.
- RUSTSEC-2026-0194 and -0195 (`quick-xml` 0.39, via `urdf-rs`) are ignored
  with a reason in `deny.toml`. urdf-rs uses quick-xml only for URDF
  *serialization*, which etendue never calls.

### Pivot P0: groundwork

Pivot groundwork (P0 of `docs/pivot/PLAN.md`).

#### Added

- **ADRs 0001–0006** (`docs/adrs/`, accepted 2026-09-26):
  - the package-family pivot;
  - the frame tree;
  - Rust-only kinematics baking;
  - the canonical render camera and remap LUT;
  - Blender as a thin renderer;
  - analytic ground truth with calibration-rs dataset emission.
- **`etendue-wasm`** — a wasm-bindgen facade and P0 spike (`publish = false`,
  destined for npm `@etendue/wasm`).
  - It exports `project_points` and `default_mvp_scene_json`.
  - Gate **G0.1** passes: the wasm build under Node and the native build
    project 731 probe points bit-for-bit identically
    (`docs/measurements/g0_1_wasm_parity.md`).
  - CI gains a `wasm` job (wasm32 clippy and the parity check).
- `etendue_core::Error::Calibration`, which wraps `vision_calibration_core::Error`.

#### Changed

- **calibration-rs from crates.io.** `vision-calibration-core = "0.8"`
  (and `vision-calibration-dataset = "0.8"`, not yet used) replace the path
  dependency. `[patch.crates-io]` redirects to the sibling checkout for local
  development.
  - `etendue-core` now packages (`cargo package`).
  - `etendue-ui` is `publish = false` and frozen until the web-parity gate.
- **Migrated to calibration-rs 0.8.** `CameraParams::build()` is now fallible.
  `working_volume`, `voxelized_overlap`, and `project_stripe` propagate its
  error; the UI frustum returns `None`.
  - This fixes the red `main` build against calibration-rs ≥ 0.8.0.
  - `Cargo.lock` recorded `vision-calibration-core` 0.4.0 and is refreshed.
- CI clones calibration-rs at a pinned tag (`CALIBRATION_RS_REF`), not
  `main`, and tests with `--locked`.
- `rust-toolchain.toml` adds the `wasm32-unknown-unknown` target.
- The workspace `rust-version` is 1.93, matching calibration-rs.
- The out-of-scope list now defers to ADR 0001: realistic rendering is in
  scope only as the Blender backend.

#### Security

- Lockfile bumps clear RUSTSEC-2026-0194, RUSTSEC-2026-0195 (`quick-xml`, via
  `wayland-scanner` 0.31.11) and RUSTSEC-2026-0257 (`webbrowser` 1.2.4). The
  yanked `chacha20` 0.10.0 is replaced by 0.10.2.
- RUSTSEC-2026-0192 (`ttf-parser` unmaintained, reached through `etendue-ui`'s
  winit Linux decorations) is still open. It is reported, not ignored.

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
