# ADR 0011: Calibration Procedures

- Status: Proposed (2026-11-01)
- Date: 2026-11-01

## Context

`etendue calibrate` (P13) picks one of four calibration-rs workflows from the dataset's shape:

| Dataset | Workflow |
|---|---|
| one camera | `single_cam_handeye` |
| several cameras | `rig_handeye` |
| one camera and a laser | `laserline_device`, then `single_cam_handeye` for the hand-eye |
| several cameras with lasers | `rig_handeye_laserline` |

Each workflow runs with a configuration fixed in code: the hand-eye mode from the truth, a
Scheimpflug seed and sensor refinement when the cameras tilt, and the solver's iterations.

- **No fixed or free choice.** A user cannot say which parameters stay fixed and which are
  free.
- **No stages.** A user cannot run a calibration as stages: intrinsics per camera, then the rig,
  then the hand-eye, then the lasers.
- **Unused building blocks.** Several pieces calibration-rs 0.11 offers go unused:
  - workflows: `planar_intrinsics`, `rig_extrinsics`, `rig_laserline_device`;
  - every workflow's `step_*` and `*_with_seed` functions;
  - the camera config's per-slot fix lists (`CameraFix`, `camera_overrides`);
  - robot-pose refinement and its σ;
  - the robust losses;
  - the uncertainty and the quality verdict.

The user wants a **bank of calibration procedures**: intrinsics, extrinsics, hand-eye, laser
planes and the non-linear refinements, with what is fixed and what is free, and the sequence of
stages that calibrates a whole system.

## Decision

### 1. A procedure is a versioned document

`CalibrationProcedure` lives in `etendue-scene`, with its schema in
`schemas/procedure.schema.json`. A procedure is an id, a name, a description, and an ordered list
of **stages**. Each stage names the following.

- **`problem`**: a calibration-rs workflow, one of:
  - `planar_intrinsics`
  - `rig_extrinsics`
  - `single_cam_handeye`
  - `rig_handeye`
  - `laserline_device`
  - `rig_laserline_device`
  - `rig_handeye_laserline`
- **`cameras`, `lasers`**: the devices it covers, by scene id. `*` means all of them.
- **`camera`**: the calibration-rs camera config, passed through unchanged:
  - the model (distortion kind, sensor kind);
  - `fix` per slot (`{"intrinsics": ["cx", "cy"], "distortion": ["k3"], "sensor": ["tilt_y"]}`);
  - overrides per camera.
- **`free`**: which of the workflow's other estimates move, using calibration-rs's own config
  names:
  - the target poses (`fix_poses`);
  - the hand-eye (`fix_handeye`);
  - the target reference (`fix_target_ref`);
  - the plane (`fix_plane`);
  - `rig.refine_intrinsics`, `rig.refine_sensor`;
  - `handeye_optimize.refine_*`;
  - the robot-pose refinement and its σ.
- **`solver`**: the robust loss, its scale, and the number of iterations.
- **`seed`**, for each estimate the stage starts from, one of:
  - `"stage:<id>"`: an earlier stage's result;
  - `"nominal"`: the design, which is what a user would enter from datasheets;
  - `"truth"`: simulation only, and marked as such in the report;
  - absent: the workflow's own linear initialisation.

### 2. One engine runs every procedure

`etendue calibrate --procedure <file|id>` runs the stages in order.

- **How a stage runs.** Each stage is built from the workflow's `step_*` functions, through
  `*_with_seed` when it has seeds. Its result is kept for the stages after it.
- **What the report holds.** It gives every stage's estimate and residuals, then the last
  stage's comparison with the truth (G5.1 / G5.3 as today).
- **Checks before running.** Before anything runs, the procedure is checked against the dataset:
  - the cameras and lasers exist;
  - each problem fits the topology;
  - every seed names an earlier stage that produces that estimate;
  - each fix list names parameters of the model (`CameraConfig::validate`).

### 3. The bank holds the procedures

Procedures live in the bank's and the project's `procedures/` (ADR 0010). The built-in ones
are:

| Id | Stages |
|---|---|
| `intrinsics` | `planar_intrinsics` per camera |
| `stereo` | intrinsics per camera → `rig_extrinsics` (joint BA) |
| `eye_in_hand`, `eye_to_hand` | intrinsics → (rig) → `*_handeye`, with the robot poses refined |
| `laser_head` | `laserline_device` (Scheimpflug, seeded) → `single_cam_handeye` |
| `laser_rig` | intrinsics per camera → `rig_handeye` → `rig_laserline_device` → `rig_handeye_laserline` (joint) |
| `auto` | today's choice by dataset shape, as four built-ins. It is the default, so `etendue calibrate` without `--procedure` stays as it is |

### 4. The studio edits procedures

The Calibrate workspace gains a procedure editor:

- a list of stages;
- per stage, a table of the parameters of the chosen cameras and lasers, each fixed or free,
  and the seed of each estimate;
- validation as §2.

A run records the procedure it used.

### 5. Missing hooks go upstream

`rig_handeye_laserline` has only `run_calibration`: no steps, and seeds only for the intrinsics.
Composing the `laser_rig` procedure from stages may need more:

- step functions;
- a hand-eye and plane seed;
- fix lists for the hand-eye and planes in the joint stage.

Each missing hook becomes a **draft** calibration-rs PR, for review. Until one is merged, the
stage runs the workflow whole, seeded where it can be, and the report says which seeds it could
not take.

## Consequences

- **Each run is described by its procedure.** A calibration is reproducible from its procedure
  and its dataset, and `runs/<name>/config.json` holds both.
- **P17 builds on it.** The simulation lab (P17) reports per stage and compares procedures on
  one dataset.
- **P18 builds on it.** Pose planning (P18) predicts uncertainty for the free parameters of the
  chosen procedure.
- **Calibration-rs's configs are reused, not copied.** The camera config and the fix names are
  calibration-rs's own, passed through, so a new calibration-rs option is available without an
  etendue release. The cost is that a calibration-rs rename breaks old procedures. The procedure
  version records which calibration-rs vocabulary it uses.
- **G16.1 guards the default.** `auto` must reproduce today's reports bit for bit.

## Alternatives considered

- **etendue's own fix/free vocabulary**, translated to calibration-rs. Rejected: it duplicates
  calibration-rs's names, and the two would drift apart.
- **One joint bundle adjustment with a free/fixed mask** instead of stages. Rejected: the linear
  initialisations need the staged order, and the user asked for sequences.

## References

- `docs/pivot/PLAN.md` P16
- `crates/etendue-cli/src/calibrate.rs` (today's workflows)
- calibration-rs `vision-calibration-pipeline`: `common::config` (`CameraConfig`, `CameraFix`,
  `RigConfig`, `RobotPoseConfig`), and each workflow's `steps.rs`
- [ADR 0010](0010-component-bank.md) (where procedures are stored)
