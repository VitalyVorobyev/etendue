# ADR 0006: Analytic Ground Truth and calibration-rs Dataset Emission

- Status: Accepted
- Date: 2026-09-26 (accepted 2026-09-26)

## Context

Synthetic datasets exist to measure calibration-rs, and for that the ground
truth must be exact. Ground truth recovered from rendered images, by running a
detector on them, would carry the detector's bias. That bias is the quantity
P4-3 measures, not truth.

calibration-rs already defines what a dataset is (`vision-calibration-dataset`
0.8, verified 2026-09-26):

- `DatasetSpec`: `deny_unknown_fields`, `version: 1`. It holds cameras,
  target, `robot_poses: RobotPoseSource`, `pose_convention: PoseConvention`,
  `topology`, `pose_pairing`, and an `_unresolved` list that must be empty.
  Validation is the free function `validate(&DatasetSpec)`.
- `DeviceSpec`: datasheet units (`focal_mm`, `pixel_pitch_um`, `_deg`). It
  holds `rig_se3_cam` mounts, `HandeyeMountSpec`, and `LaserPlaneSpec`.

The calibration targets themselves come from calib-targets-rs
(`calib-targets-print` 0.15, verified 2026-09-26):

- `PrintableTargetDocument` wraps a `TargetSpec` (`chessboard`, `charuco`,
  `marker_board`, `puzzle_board`, `puzzlepole`).
- `TargetSpec::resolved_points()` is **public**. It returns
  `ResolvedTargetPoint { position_mm: [f64; 2], grid, id }` in the board plane:
  millimetres, origin at the board's top-left, inner corners only.
- The drawing primitives (`Primitive`, `Scene`) are `pub(crate)`.

## Decision

### Ground truth is analytic

**Feature points**

- Board points come from the same `PrintableTargetDocument` that produced the
  target's texture or mesh, via `TargetSpec::resolved_points()`.
- `etendue-synth` converts them to metres and lifts them to `z = 0` in the
  target entity's frame.
- The mapping from board coordinates (top-left origin, y-down) to the target
  frame is defined **once**, in `etendue-synth`. A test checks it against the
  corners of the generated mesh or texture (P3-3).

**Projection**

- Points are projected with the calibration-rs `CameraModel` (`project_point_c`)
  after the frame-tree transforms from [ADR 0002](0002-frame-tree.md).
- This is the same model that the remap LUT from
  [ADR 0004](0004-canonical-render-camera.md) inverts.

**Visibility per point**

- In front of the camera, inside the image with a border margin, and facing
  the camera (board normal).
- In the photometric tier, a point must also be unoccluded according to the
  Blender object-index pass ([ADR 0005](0005-blender-renderer.md)).
- The geometric tier (P3) uses the first three tests only.

### The output bundle is consumed by calibration-rs unchanged

- **`dataset.json`** — a calibration-rs `DatasetSpec`. It contains:
  - image lists per camera;
  - a dataset `TargetSpec` (e.g. `chessboard { rows, cols, square_size_m }`).
    This is the calibration-rs dataset vocabulary, not the calib-targets-print
    type. The mapping lives in `etendue-synth`, with a test for every target
    kind.
  - a `robot_poses` file (`base_se3_<tcp_link>` per capture);
  - an explicit
    `pose_convention { transform: t_base_tcp, rotation_format: quat_xyzw, translation_units: m }`;
  - the matching `topology`;
  - `pose_pairing: by_index`;
  - an empty `_unresolved`.

  P3-2 is done when `vision_calibration_dataset::validate` accepts every
  emitted manifest.
- **`device.json`** — a nominal `DeviceSpec`: the values a user would enter
  from datasheets. It may optionally be perturbed from the truth, to exercise
  calibration-rs seeding.
- **`gt.json`** — an etendue-owned, versioned schema. It contains:
  - the true `CameraParams` per camera;
  - `cam_se3_rig`;
  - hand-eye as `gripper_se3_rig` or `rig_se3_base`;
  - target poses;
  - laser planes;
  - per-capture ground-truth points (pixel, visibility, board id / grid
    coordinate).

### Laser-plane conventions are explicit

calibration-rs has two conventions:

- `DeviceSpec` `LaserPlaneSpec`: `n·p = d`, rig frame, millimetres.
- The optimizer's `LaserPlane`: `n̂·p + d = 0`, camera frame, metres.

No calibration-rs derivation converts between them. `gt.json` therefore stores
laser planes in the optimizer's form (camera frame, `n̂·p + d = 0`, metres) and
tags each one with its frame and convention. `device.json` uses the
`LaserPlaneSpec` form.

A `DeviceSpec` → optimizer laser-plane derivation is proposed as an upstream
calibration-rs PR in P5, with user review. It is not re-implemented here.

## Consequences

- calibration-rs's `dataset_runner` consumes synthetic datasets directly. The
  closed-loop gates (G5.1, G5.3) compare its output against exact truth.
- Ground truth is independent of rendering. The same ground truth serves the
  geometric and photometric tiers, and it is available before any Blender run.

Open items, each to be decided in the phase named:

1. **Second nalgebra (P3).**
   - calib-targets-rs is on nalgebra 0.35 and calibration-rs on 0.34.
   - Depending on `calib-targets-print` from `etendue-synth` would put a
     second nalgebra into the tree.
   - No nalgebra type crosses the boundary (`resolved_points` returns
     `[f64; 2]`), but it breaks the "one nalgebra" check.
   - Options: upstream alignment on one nalgebra version, a
     nalgebra-independent layout crate or feature in calib-targets-rs, or a
     documented, contained exception. The choice is measured and put to the
     user in P3.
2. **Private primitives (P3-3).** Target meshes need the board primitives as
   public API. The plan is an upstream calib-targets-rs PR draft, with the SVG
   texture (≥ 8 texels per projected pixel) as the fallback. P4-3 measures
   both.
3. **Tag mismatch.** The `puzzle_board` wire tag differs from
   `kind_name() == "puzzleboard"` in calib-targets-print. The target-kind
   mapping is keyed on the serde tag and covered by a test.

## Alternatives considered

- **Ground truth from detections on noise-free renders.** Rejected, because it
  folds detector bias into truth. That bias is measured separately (P4-3).
- **An etendue-specific dataset format with a converter.** Rejected: emitting
  `DatasetSpec` directly means calibration-rs validation guards the contract.

## References

- calibration-rs ADR 0016 (dataset manifest), ADR 0021 (laser manifest),
  ADR 0023 (DeviceSpec)
- calib-targets-rs `calib-targets-print` `model/mod.rs` (`TargetSpec`,
  `resolved_points`)
- `docs/pivot/PLAN.md` §4 (ADR-0006), P3-2, P3-3, P5
