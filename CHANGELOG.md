# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **`etendue real calibrate <id | dataset.toml> -o <folder>`** (P20a): calibrates a real dataset
  (a calibration-rs `dataset.toml` of images and robot poses) with calibration-rs's dataset runner
  and keeps the run: residuals per feature and per view, σ, the quality verdict and the views left
  out, with no truth to compare against. A dataset is named by its manifest, by an id in the
  git-ignored `local/datasets.toml`, or as `kuka_1`/`DS8` from a calibration-rs checkout beside
  etendue. Its estimates and residuals equal calibration-rs's own run of the manifest bit for bit
  (gate R1).
- **`etendue real twin <run> [--trials N] [--seed S] [--exact] -o <folder>`** (P20b): builds the
  twin of a real calibration (`etendue-synth::twin`): its result as the truth, its robot poses, and
  in each view the points its detector found, observed with the noise the real residuals imply
  (calibration-rs's reprojection σ and its robot pose error). The twin is calibrated with the real
  run's workflow and configuration over seeded trials and kept as a run, and the real run is set
  against it (`etendue_cli::real::versus`): residual RMS, whiteness across the image and within
  views, σ against the twin's scatter. Gate T1 and characterisation T2 on `kuka_1` and `DS8`:
  `docs/measurements/p20_twin.md`.
- **`etendue real split-half <run> [--random K] [--seed S] [--procedure <id | file>] -o <report.json>`**
  (P20c): checks the pose planner's σ on a real calibration's own poses. The planner's system σ and
  per-parameter σ, predicted on the twin (`etendue_synth::plan::predict`), are set against the real
  calibration's covariance and a split-half estimate: halves of the real views calibrated
  separately, the difference of their estimates against the planner's `√(σ_A² + σ_B²)`, pooled over
  the interleaved split, the halves in capture order and `K` seeded random splits. Gate P1 on
  `kuka_1` and `DS8`: `docs/measurements/p20c_planner.md`.
- The planner can marginalise each view's robot pose correction under a prior
  (`etendue_synth::plan::RobotPrior`), as calibration-rs refines robot poses by default;
  `scenario plan` is unchanged (exact poses).
- **The planner's noise model learnt from a real calibration** (P20c2):
  `etendue_synth::plan::NoiseModel` (pixel σ, robot prior, and a per-view term, the view scatter
  `k`: each view counts for `1 / k` of its white information) and `NoiseModel::learnt`, which takes
  a real run's pixel σ, robot pose error and view scatter, and leaves out of the robot error the
  part of the pose corrections that is view-correlated pixel error. `etendue real split-half`
  predicts with it, and reports P20c's model (white views) beside it. Gate P1 now passes on
  `kuka_1` and `DS8`: `docs/measurements/p20c2_noise_model.md`. With `k = 1` (the default, and
  every synthetic prediction) nothing changes; `scenario plan` is unchanged.

### Changed

- `etendue_synth::plan::predict` takes a `NoiseModel` in place of the pixel σ and robot prior, and
  `etendue_cli::real::split_half` likewise; the split calibrations are a step of their own
  (`calibrate_splits`, then `judge` against a model). A split-half report holds its `model` (and
  `white_model`, `planner_white_mm`) in place of `noise_px` and `robot`.

- A calibration report's per-feature errors are calibration-rs's own (`error_px`) rather than
  recomputed, which differed in the last bit for about one residual in six.
- A kept run's `config.json` is version 2 when the run is of a real dataset (it names the
  manifest under `real`); synthetic runs stay version 1, so an older etendue still reads them and
  lists a real run as unreadable, with the reason. A real run's twin is version 3 (it names the
  real run under `twin`).

## [0.3.0] - 2026-11-12

### Added

- **The stripe triangulation row** of a laser calibration report (G5.3c): every true stripe
  pixel triangulated with the calibrated camera, laser plane, hand-eye and robot pose, against the
  true point in the robot's base, rms and largest per camera; the largest is checked at 0.05 mm on
  exact data. It does not depend on how a Scheimpflug camera's projection splits between tilt,
  principal point and camera frame.
- **The studio's cell builder** (Setup ▸ New cell…): a stepper that makes a cell from library
  items — optics (a camera or a head), one camera or a rig design (with per-position names and
  cameras), eye-in-hand or eye-to-hand through a mount, a target sized against the field of view
  at the working distance, and a robot with its base and start pose. The cell shows in the 3-D
  view as it is built, the kernel checks what each camera sees of the target and aims the robot by
  IK, and the result is an ordinary scene with an empty program, one undoable edit. Bank and
  built-in items it uses are copied into a writable project. It recreates `eye_in_hand_ur5e` and
  `eye_to_hand_ur5e` exactly (`docs/measurements/g15_cell_builder.md`).
- Built-in library items for those examples: the mounts `flange_plate_30mm` and
  `board_holder_10mm`, the rig `stereo_80mm_parallel` and the camera `python1300_pinhole_right`.
- **The bank** (ADR 0010): a user library shared by every project, in the same layout as a
  project's library, with an `etendue-bank.json` manifest. `etendue library init|import|check|list`
  make, fill, check and list it; `list` shows each item from the layer it resolves to (project,
  bank, built-in). `etendue project check --bank` notes copies whose bank source has changed.
- **The studio's procedure editor** (Calibrate, and the Library workspace for procedure items):
  stages in order, each with its workflow and camera, the parameters held fixed per slot, its
  seeds, the options that apply to its workflow, and its solver. The kernel checks the procedure
  on every edit. A calibration runs a library procedure as it is or edited (by value; the run
  folder's name carries its id), and an edited one is kept in the project or the bank. A
  procedure passed by value is now checked as a library file's is, before anything runs.
- **The studio's Library workspace**: every item the open project sees (project, bank,
  built-in), filtered by text, kind and layer, with what hides what and which project copies are
  stale. Forms for sensors, lenses, lasers and mounts, a checked JSON editor for targets, cameras,
  heads and rigs, and name, description and source for every item. New, duplicate, rename,
  delete, import and export; use in project, save to bank, update a stale copy. The desktop
  shell's `remove_file` removes files only inside the folders it lets the studio write.
- **Calibration procedures** (ADR 0011): `etendue calibrate --procedure <id | file>` calibrates
  in stages: each stage is a calibration-rs workflow, now including planar intrinsics and rig
  extrinsics without a robot, with its camera model, the parameters it holds fixed, what else
  is free, its solver and its seeds (an earlier stage, the design, or the truth). Six built-ins
  (`intrinsics`, `stereo`, `handeye`, `rig_handeye`, `laser_head`, `laser_rig`) ship as
  `examples/library/procedures/`; `procedure` is a library kind. The report names its procedure
  and lists its stages (report version 3). Without `--procedure`, the choice by the dataset's
  shape is unchanged, bit for bit.
- **Calibration-pose planning**: `etendue scenario plan` chooses, from the sphere generator's
  feasible poses, the budget that minimises the predicted system σ (how far the calibrated cell
  would misplace a point known in the robot's base frame), with the σ after each pose and the
  thinned grid's for comparison. Eye-to-hand cells (the target on the robot) are planned too.
- **Suggest poses** (studio, Motion → Generate poses): a pose budget, the predicted system σ
  of the suggested poses and of the evenly thinned grid against the number of poses, and
  **Apply _n_ suggested poses**. `@etendue/wasm` `planPoses`; the plan report has the grid's
  curve (`grid_sigma_curve_mm`).
- **Eye-to-hand pose generation**: `etendue scenario sphere` places a target the robot carries
  in front of a fixed sensor, from the same spheres; `examples/eye_to_hand_ur5e/sphere.json` is
  its spec for the example.
- **The simulation report**: every target point's residual with its capture, camera and point;
  calibration-rs's predicted σ (`uncertainty`) and quality verdict (`quality`); and, over trials,
  `param_stats`: each parameter's truth, mean, spread, bias, RMSE and predicted σ. `etendue
  calibrate --trials N` prints them.
- **The simulation lab in the studio**: the Calibrate workspace's result has tabs. **Per view**
  lists every view's residual RMS (sortable); choosing one shows its capture in the camera view,
  each observed point with its reprojection and its residual magnified ×1–×1000, coloured by size,
  over the rendered image when there is one. **Parameters** (two or more trials) tables each
  parameter's truth, mean, bias, spread and calibration-rs's predicted σ, tints std/σ outside
  0.7–1.4, and draws a camera parameter's values over the trials with the truth and the ±σ band.
  **Solver** lists the procedure's stages and calibration-rs's quality checks; its verdict is a
  badge beside the result.
- Library items version 2: `source` (datasheet), `origin` (where a project's copy came from, with
  the source's content hash) and the `mount` kind (an adapter on a robot's flange). Version 1
  still reads.

### Changed

- **calibration-rs 0.18.0** (from 0.12.0; crates and the `vision-calibration` wheel), P19c/P19d.
  The hand-eye exports now carry their hand-eye as one tagged value (`handeye.mode`), so a run's
  `result.json` has the new shape. What moved in the reports is recorded in
  `docs/measurements/calibration_rs_0_18.md`.
- **The report and kept-run documents have JSON Schemas** (`schemas/calibration_run.schema.json`,
  `schemas/kept_runs.schema.json`), and the studio's types for them are generated from those
  schemas, as the scene documents' are. calibration-rs's quality verdict is typed, so its new
  `covariance_linearity` check now has a label in the studio.
- **A kept run that does not read is shown, with the reason** (a newer etendue's run, or a
  damaged one), instead of disappearing from the studio's list.
- **A laser rig's joint stage can start from a rig hand-eye stage, and laser planes can be
  seeded** (calibration-rs#256): a `rig_handeye_laserline` stage's rig seed takes a `rig_handeye`
  stage's result as a whole, and a new `planes` seed takes the design's planes or an earlier
  stage's, on both laser workflows. Measured in `docs/measurements/p19d_upstream_adoption.md`.
- **One table says which calibration workflow takes which option and seed** (P19e). The
  procedure check, `auto`, the CLI and the studio's procedure editor all read it (wasm
  `procedureRules`), so the editor offers exactly what the check accepts. "Refine intrinsics
  with the rig" is now offered and accepted on a rig-extrinsics stage, where calibration-rs has
  it. A procedure whose stage options calibration-rs would not read is refused before it runs.
- **calibration-rs's results are read through its own types.** `etendue calibrate` no longer
  reads the workflows' exports as untyped JSON with silent defaults: a field calibration-rs
  renames is a compile error. A config, seed or input key that calibration-rs does not read is now
  an error naming it, where it was silently dropped.
- **Docs brought up to date** (P19a): the README and the book's studio chapter describe the
  Library workspace, calibration procedures, the simulation lab and Suggest poses; the roadmap
  records the upstream calibration-rs PRs as merged and adds P19, a consolidation phase.
- `etendue detect` finds a chessboard's corners with chess-corners' ChESS and its saddle-point
  refiner instead of Radon, and the studio's dataset generation with it. On the closed-loop renders
  it is about five times more accurate (0.012 against 0.058 px RMS), and the hand-eye from the
  detected corners is 7 times closer to the truth. These are synthetic images; the other target
  kinds keep Radon. `--corners radon|saddle_point` chooses for every kind
  (`docs/measurements/p5_1c_saddle_point.md`).
- **calibration-rs 0.12.0** (was 0.9.0, through 0.11.0), **calib-targets 0.16.1** (was 0.15.5)
  and **chess-corners 2.1** (was 1.2). The Python cross-check uses the `vision-calibration` 0.12.0
  wheel (one `Camera` class).
- calibration-rs 0.12 measures `view_scatter` by leaving out one view at a time, so the predicted
  σ in reports (`uncertainty`, `param_stats`) changes. On the laser examples it moves by up to
  4 %; the estimates are unchanged (`docs/measurements/calibration_rs_0_12.md`).
- A camera with calibration-rs 0.12's fisheye models (the equidistant projection, Kannala-Brandt
  distortion) is refused by scene and library validation: etendue's optics, render remap and
  calibration seeds assume a pinhole.
- Camera intrinsics are `{fx, fy, cx, cy}`: calibration-rs dropped skew and the
  `"type": "fx_fy_cx_cy_skew"` tag. Older scenes still load (the extra keys are ignored); the
  examples, the library and the schemas use the new shape.
- `etendue calibrate` uses calibration-rs 0.11's workflow defaults. Rigs now refine intrinsics in
  the rig and hand-eye bundle adjustments, and robot-pose refinement and the laserline losses
  follow the new defaults. The Scheimpflug laser example (`laser_scheimpflug_ur5e`) now passes
  the closed-loop check on exact data.
- The corner-bias study (`etendue measure g4-2`) no longer has a Förstner setup: chess-corners 2
  removed that refiner.

### Removed

- **The egui-era optics kernel in `etendue-core`** (P19f, about 4,800 lines): `CameraEntity`,
  `ThickLens`, the thick-lens CoC formulas (`optics::coc`, `psf`), the working volume, defocus map
  and voxel overlap (`analysis`), the `Scene` aggregate, the Scheimpflug solver and
  `laser::project`. Nothing outside the crate's own tests used them once the physical camera
  (ADR 0008) replaced them. Their product equivalents are `PhysicalCamera`'s blur, `Head::map`,
  `rig::coverage` and `Head::optimise`. `docs/kernel.md` records what they were.
- A physical camera whose sensor tilts about both axes behind a lens with its entrance pupil off
  the principal plane (pupil magnification ≠ 1) is refused. Its equivalent pinhole needs a skew,
  which calibration-rs no longer models. One-axis tilts, and any tilt with the pupil at the
  principal plane, are unchanged.

### Fixed

- **The studio no longer loses unsaved work** (P19b). Opening another cell, scene or project,
  closing the desktop app's window, or closing the browser tab asks first when the cell has unsaved
  changes. A Library item's draft survives a trip to another workspace, and choosing another item
  asks before discarding it.
- **Undo, Redo and Save act on the cell only where it is shown** (Setup, Motion, Images,
  Calibrate). In Library, Optics and Rigs the buttons are absent and the shortcuts do nothing,
  instead of changing or saving the cell out of sight.
- **An error while drawing a pane stays in that pane**: it says what went wrong and offers to try
  again, and the rest of the studio keeps working, instead of the whole window going blank.
- **A triangulation head's result now holds together.** `auto` on a head and the `laser_head`
  procedure estimated the laser plane with one camera solution and the hand-eye with another
  (`auto` with an untilted model), so a stripe point triangulated in the robot's frame landed
  0.1–13 mm off under 0.1 px of noise (`auto` on a Scheimpflug head 29 mm off even on exact
  data).
  Both now hold the device stage's camera for the hand-eye: about 0.013 mm
  (`docs/measurements/head_triangulation.md`).

## [0.2.0] - 2026-10-20

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
  - `scenario from-poses`: a stop-and-shoot scenario from tool poses;
  - `scenario sphere`: calibration viewpoints on spheres around the target;
  - `calibrate`: calibrate a generated dataset with calibration-rs and compare the result with the
    ground truth, exactly, with seeded noise and trials (`--noise`, `--trials`), or from detected
    features (`--features`), keeping each run (`--run`);
  - `export`: capture poses, the timed joint trajectory, the motion report and the resolved
    scenario;
  - `project check`: validate a project folder, its cells and its component library.
- **Examples**: eye-in-hand and eye-to-hand UR5e cells, a hand-eye calibration cell, and
  laser-line cells with plain and Scheimpflug sensors.
- **`@etendue/wasm`**, the WebAssembly kernel the studio runs on (an npm package built from
  source): baking, projection, back-projection and remap tables in the browser.
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
  the ground truth; `features.json` records the target `kind` and why a view failed. Its accuracy
  per kind is measured in `docs/measurements/g4_4_targets.md`.
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
  rays, the plane of best focus and its depth of field, draggable focus, aim, laser and window,
  or the head in 3-D), a
  map of the laser plane (blur, depth and lateral resolution, triangulation angle, stripe width),
  the laser line on the sensor at several ranges with its width in pixels, charts of blur, stripe
  width and resolution against range, one-click Scheimpflug, an optimiser over tilt, focus and
  f-number with a search over the library's lenses, saving to the library and placing in a cell.
- **Robot library**: compare the studio's robots (axes, joint ranges and speeds, how far the TCP
  gets from the base) and add one to the cell or replace the selected robot.
- **Rigs workspace slices**: to scale, with each camera's sharp range along its axis
  (`etendue-core::rig::sharpness`, `rigSharpness` in `@etendue/wasm`) and handles for the working
  distance, ρ, φ, the phase and the box.
- **Focused lasers**: a laser's `waist_distance_m` (scene, rig and library) says where it is
  focused; the beam is narrowest there (`GaussianBeamWidth::with_waist_distance`), for the stripe
  in ground truth, laser images and the Optics workspace. Absent, a scene laser's waist is at the
  laser, as before, and a library laser is focusable: the Optics workspace focuses it at the
  head's aim point. `@etendue/wasm` has `headMap`, `headSection`, `headScheimpflug`, `headOptimise` and
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

  The workspace also draws the estimate over the truth in 3-D: cameras, laser planes and the
  target, exaggerated ×1 to ×1000. At each capture, a camera view shows what the calibration
  observes (noise, outliers and dropouts) over the true corners. The report's `estimate.offsets`
  carry the estimate in the truth's frames.

  Every run is kept as a folder: `config.json`, `report.json`, and calibration-rs's own export as
  `result.json`. It goes in the project's `runs/` (or the dataset's own `runs/`), and
  `etendue calibrate --run <folder>` keeps one the same way. The workspace lists a dataset's
  kept runs and shows any of them again. With a project open for writing, new datasets default
  to its `datasets/<cell>`.

  In a browser, the Calibrate workspace previews what a calibration observes, exactly or with
  noise, at each capture. Generating and calibrating need the desktop app.
- **Program editing in 3D** (Motion):
  - Re-teach a step in place from the Jog panel.
  - The TCP path is coloured by step, with warnings marked and the current move highlighted.
  - Clicking the path seeks the playhead to that sample.
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

- **The book** (https://vitalyvorobyev.github.io/etendue/): tutorials, a user guide, and the
  models and algorithms behind etendue (camera models, physical cameras, defocus, Scheimpflug,
  laser heads, rig coverage, kinematics, motion, pose generation, synthetic images and closed-loop
  calibration), with interactive widgets that run etendue's kernel in the page.
- **Installers**: the studio for macOS (Apple silicon, `.dmg`), Windows (`.msi` and a setup
  `.exe`) and Linux (`.AppImage`, `.deb`), and the `etendue` CLI for the same platforms, attached
  to the GitHub release with their SHA-256 checksums. The builds are not code-signed.

### Changed

- **Document versions**: scenes, scenarios, robot manifests, projects and library items are
  version 1; `gt.json` is version 3 (stripe widths, each camera's `mech_se3_projection`); the
  Blender render job is version 4.
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
- `etendue detect` printed the laser-stripe table twice.
- Regenerating a dataset in the studio left the previous `laser_features.json` behind.
- On Windows, the studio could not generate a dataset or save a scene on another drive than the
  robot library: the scene now names that robot by its absolute path.
- The CLI's help and verdicts, and the studio's labels, name the closed-loop check in words rather
  than by development ids (the report's `gate` field is unchanged).

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

[Unreleased]: https://github.com/VitalyVorobyev/etendue/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/VitalyVorobyev/etendue/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/VitalyVorobyev/etendue/releases/tag/v0.2.0
