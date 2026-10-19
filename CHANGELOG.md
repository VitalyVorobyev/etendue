# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

etendue is now a tool for designing robot–camera–laser calibration cells and generating synthetic
calibration datasets with exact ground truth. The 0.1.0 desktop app is replaced by etendue studio.

### Added

- **etendue studio**, the desktop app (macOS, Windows, Linux; unsigned builds in the GitHub
  release): open a scene and scenario, play the robot's motion in 3D, see each camera's view with
  the board projected through its camera model, and generate a dataset — ground truth, Blender
  render and board detection in one run, with progress and cancel. It carries the examples and
  the robot library. The same studio runs in a browser (without dataset generation) when built
  from source.
- **`cargo app` and `cargo app-release`**: build and run the desktop app (debug) from a source
  checkout, or build its release app and installers, in one command each (robot meshes, wasm
  kernel and web packages first).
- **Scene and scenario documents**: a versioned JSON scene (a frame tree of robots, rigs, cameras,
  line lasers, lights, calibration boards and parts) and scenario (`ptp_joints`, `ptp_pose`, `lin`,
  `wait` and `capture` steps), with JSON Schemas in `schemas/`. Cameras use calibration-rs camera
  models, including distortion and Scheimpflug tilt; boards are what calib-targets prints.
- **Robots**: URDF robots with forward kinematics, damped-least-squares and analytic (OPW) inverse
  kinematics, and synchronised, limit-respecting motion. The UR5e and the ABB IRB 1200 are
  included.
- **Command line** (`etendue`):
  - `validate`: check a scene and a scenario (limits, reachability);
  - `bake`: every frame's pose at every sample;
  - `gt`: the analytic ground truth as a calibration-rs dataset (planar or Scheimpflug intrinsics,
    rig extrinsics, single-camera or rig hand-eye, eye-in-hand or eye-to-hand, and laser-line
    rigs), with `gt.json` holding the exact answer;
  - `render`: Blender (Cycles) renders through an ideal pinhole, resampled exactly onto each
    camera's model, with optional supersampling and a sensor model (exposure, noise,
    quantisation, raw output);
  - `detect`: target detection (chess-corners' Radon corners, calib-targets' detectors for
    every target kind), checked against the ground truth;
  - `scenario from-poses`: a stop-and-shoot scenario from tool poses.
- **Examples**: eye-in-hand and eye-to-hand UR5e cells, a hand-eye calibration cell, and
  laser-line cells with plain and Scheimpflug sensors.
- **`@etendue/wasm`** (npm): baking, projection, back-projection and remap tables in the
  browser.
- **Rust crates**: `etendue-scene`, `etendue-kinematics`, `etendue-synth`, and the `etendue_cli`
  pipeline library.
- **Robots**: the igus ReBeL 6DOF joins the library; the FAIRINO FR5 can be built locally (its
  description has no licence, so it is never committed or shipped). Every joint has a jerk limit
  (`max_jerk`, with its provenance). `tools/robot-assets/build.py` reads STL meshes, adds missing
  tool frames (`[[robot.frames]]`), and builds local-only robots with `--include-local`; the book
  has a recipe for adding a robot. The studio adds and removes library robots.
- **Feasible, jerk-limited motion**: every PTP and LIN move is time-optimal under the joints'
  velocity, acceleration and jerk limits (Ruckig, via `rsruckig`); PTP moves are straight in joint
  space. A `ptp_pose` can store its joint configuration (`q`) so a bake never switches IK branch;
  `resolve` stores them all. A motion report gives each step's limit use, joint-limit margin,
  singularity margin and warnings (`motion_report.schema.json`). `etendue export` writes the
  capture poses (CSV or JSON, `captures.schema.json`), the timed joint trajectory (CSV), the
  report and the resolved scenario; `etendue bake --derivatives` records joint velocities and
  accelerations. The studio's Motion workspace shows the report, charts velocity and acceleration
  against the limits, resolves IK, and exports all of it.
- **Rig layouts**: mono, stereo and ring rigs (`RigSpec.layout`): N cameras around the rig axis
  converging on a working point at a tunable tilt, optionally with one line laser per camera
  (`LaserSpec.camera`). Datasets then give every camera its own laser plane (rig hand-eye
  laserline with per-camera planes). The studio's rig builder lays a rig out live;
  `expandRig` in `@etendue/wasm`.
- **Every calib-targets target**: `printed` targets — chessboard, ChArUco, marker board,
  PuzzleBoard and the PuzzlePole cylinder — drawn from one triangle mesh in the studio and in
  Blender (render job version 3), with exact ground truth (3-D points and normals on the pole) and
  laser stripes on the cylinder. `dataset.json` is written for the targets calibration-rs takes.
  The studio's target editor sets any kind and its print parameters; `@etendue/wasm` has
  `targetMesh`, `targetPoints`, `projectTarget` and `poleCircumferences`.
- **Detection for every target**: `etendue detect` takes the target from `gt.json` (no
  `dataset.json` needed) and labels chessboards, ChArUco boards (by id), marker boards (by their
  circles), PuzzleBoards (by master id) and PuzzlePoles (by pole id), every label checked against
  the ground truth; `features.json` records the target `kind` and why a view failed. Gate G4.4
  (`docs/measurements/g4_4_targets.md`) characterises it per kind.
- **Laser images**: `etendue render` writes `<capture>_laser.png` for every camera that images a
  laser. The stripe is composited in Rust from the render's depth, normal and albedo passes, with
  a Gaussian beam, Lambertian reflection and shadows from depth views at the laser. New
  `LaserSpec.power_w` and `--laser-ambient`. The render job is version 4. The studio's camera
  views draw each camera's laser stripes. `etendue detect` extracts the stripes into
  `laser_features.json`, with the stripe's width against the beam's, and `closed_loop.py
  --features` calibrates a laser rig from detected corners and stripes. `gt.json` (version 3)
  gives the beam's diameter at every stripe point (`width_m`).
- **Sphere pose generator**: eye-in-hand calibration poses on spheres around a target (radii,
  polar and azimuth steps, rolls), kept when the robot reaches them with joint and singularity
  margins and every camera sees the target, ordered into a short tour and pinned to their joint
  configurations (`etendue scenario sphere`, `generatePoses` in `@etendue/wasm`, the studio's
  Motion workspace with the candidates drawn in 3D). The scenario keeps its `generator` spec. New
  example `rig3_laser_ur5e`: a three-camera, three-laser ring made with the rig builder and the
  generator. `closed_loop_ur5e`, `laser_ur5e` and `laser_scheimpflug_ur5e` now use 24 generated
  viewpoints (were 20); `tools/closed-loop/make_scenario.py` is removed. G5.1 and G5.3 still
  pass.
- **Studio editing**: Setup, Motion, Images and Dataset workspaces; every entity's parent, pose
  and parameters are editable, entities can be added and removed, and edits bake at once with
  undo and redo. Save and Save As (robot manifest paths follow the scene), export of the scenario
  and the baked motion, and datasets generated from the scene as edited (written into the
  dataset as `scene.json`). `EtendueScene.updateScene` in `@etendue/wasm` replaces a session's
  scene without re-parsing its robots.
- **Live editing**: the inspector's numbers are sliders with typed fields, grouped in folds; a
  gizmo moves and turns the selected item in 3D (local or parent axes, snapping). A pose change is
  shown as it moves without baking (followers and robot links included); a release is one undo
  step.
- **Physical cameras** (ADR 0008): a camera's optional `optics` (sensor pitch, focal length,
  f-number, principal-plane gap, entrance pupil, focus distance, sensor tilt) is the source of
  truth for its pinhole, which is derived with focus breathing. The camera's pose is its
  mechanical frame (the image-plane datum), independent of the lens; projection, blur and the
  frame invariance are tested. `@etendue/wasm` has `deriveOptics` and `mechSe3Projection`;
  `gt.json` records each camera's `mech_se3_projection`.
- **Projects and the component library** (ADR 0009): a project folder (`etendue-project.json`)
  holds cells, robots, defaults and a library of sensors, lenses, lasers, targets, cameras, heads
  and rigs (`library.schema.json`, `project.schema.json`). `examples/` is the reference project,
  `etendue project check` validates one, and the studio opens, creates and saves projects (on the
  desktop, or through the browser's folder access).
- **The Optics workspace**: design a laser-triangulation head from the library's parts against
  the window it must measure in. A Scheimpflug section (sensor, principal planes, pupils, traced
  rays, the plane of best focus and its depth of field, draggable focus, aim, laser and window), a
  map of the laser plane (blur, depth and lateral resolution, triangulation angle), one-click
  Scheimpflug, an optimiser over tilt, focus and f-number, saving to the library and placing in a
  cell. `@etendue/wasm` has `headMap`, `headSection`, `headScheimpflug`, `headOptimise` and
  `headApply` (`etendue-core::head`).
- **Native calibration** (`etendue calibrate`): a dataset is calibrated with calibration-rs's
  own workflows in Rust and compared with its ground truth, row for row as the closed-loop script
  does. The rows equal the script's to 2e-12.
  - **Workflows:** single-camera and rig hand-eye, rig hand-eye laserline, and the laserline
    device for a single triangulation head.
  - **Hand-eye:** eye-in-hand and eye-to-hand.
  - **Observations:** exact; or with seeded noise (pixel, outliers, dropouts, laser, robot pose),
    with `--trials` statistics; or detected (`--features`).
  - **Report:** JSON with the rows, residuals and the estimate.

  `etendue-synth::observe` synthesises the noisy observations. A single camera with its laser
  on a robot now emits a `laserline_device` dataset. In the studio (desktop), the Dataset
  workspace becomes **Calibrate**: generate a dataset, then calibrate it (exact, with noise and
  trials, or detected) and compare. It shows a table against the gate, the trial spread and a
  residual histogram.
- **Robot jogging** (Motion ▸ Jog): move a robot at the playhead by its joints or in Cartesian
  steps (tool or base axes), or drag its TCP with the gizmo (IK from the current joints). The TCP
  pose, its manipulability, joints near their limits and out-of-reach targets are shown. The pose
  can be taught as a PTP step, with its joints stored and optionally a capture. Steps can be
  moved and deleted, and the program's TCP path is drawn with its captures. `@etendue/wasm` has
  `robotLinks`, `robotLinkPoses`, `robotTcp` and `robotIk`, and robot FK and IK are bit-identical
  in wasm and natively.
- **The Rigs workspace**: repeat a camera or a head (its laser fixed on the camera's body,
  `LaserLayout.camera_se3_laser`) as a stereo pair (baseline, convergence) or a ring (k cameras,
  radius ρ, tilt φ, phase), and see what it covers.
  - The rig in 3D with its coverage voxels (seen by 1, 2 or all cameras, or measured by a
    laser–camera pair).
  - Side and top slices, and the volumes against φ.
  - Camera units can be refocused on the working distance.
  - Save to the library (`rigs/`), and place in a cell.

  Stereo is `image_x: "common"`: the image rows line up. `@etendue/wasm` has `rigCoverage`
  (`etendue-core::rig::coverage`).

### Changed

- **Blender 5.2**: `etendue.toml` pins the 5.2 series. A two-part pin (`"5.2"`) accepts every
  5.2.x; a three-part pin stays exact. The render gates were re-run on 5.2.2.
- calibration-rs 0.9.0.
- calib-targets 0.15.5: ChArUco and marker boards turned past 45° in the image are labelled
  correctly (they were one square off).
- `etendue-core`'s Scheimpflug design solver runs on an in-tree, deterministic Nelder–Mead and
  restarts until a restart stops helping. It is always built (the `solver` feature and the
  `argmin` dependency are gone) and builds for WebAssembly.

### Fixed

- **The defocus model's sensor tilt about x had the wrong sign.** calibration-rs's `tilt_x` is the
  negative of `etendue-core`'s defocus `tau_x`. The bridge passed it through, so for a camera tilted
  about x, the plane of best focus, every blur value and the solver's proposal were mirrored.
- Camera intrinsics derived from a lens use the image distance at the focus setting, not the focal
  length (about 9 % for a 25 mm lens focused at 0.3 m).

### Removed

- The 0.1.0 egui desktop app (`etendue-ui`). Its optics — defocus, Scheimpflug plane of best
  focus, laser line, working volume and the Scheimpflug solver — remain in `etendue-core`; the
  studio's Optics workspace replaces its design panels.
- `etendue-core`'s `Scene::triangulation_ring`: rig layouts (`RigSpec.layout`) build rings.

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
