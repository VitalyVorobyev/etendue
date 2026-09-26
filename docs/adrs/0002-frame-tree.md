# ADR 0002: Frame Tree

- Status: Accepted
- Date: 2026-09-26 (accepted 2026-09-26)

## Context

Today's `etendue_core::Scene` is a flat list of cameras, lasers, and targets.
Each one carries `pose: Isometry3<f64>`, which maps local coordinates into the
world frame. The pivot needs more structure:

- robots whose links move;
- multi-camera rigs;
- cameras attached to a robot flange (eye-in-hand);
- targets attached to a flange while the rig stays fixed (eye-to-hand);
- lights and passive parts.

Synthetic datasets must also map exactly onto the calibration-rs vocabulary
(`vision-calibration-dataset`):

- `RigLayoutSpec.cameras[].rig_se3_cam`;
- `HandeyeMountSpec::EyeInHand { gripper_se3_rig }` and
  `HandeyeMountSpec::EyeToHand { rig_se3_base }`, serialised as
  `tag = "mode"`;
- `DatasetSpec.robot_poses` with an explicit `PoseConvention`.

## Decision

### Transform naming and wire format

- Names follow calibration-rs ADR 0009: `a_se3_b` maps coordinates in frame
  `b` to frame `a`.
- Every SE(3) value on the wire is
  `{"rotation": [qx, qy, qz, qw], "translation": [tx, ty, tz]}`. That is
  nalgebra's `Isometry3` serde: a Hamilton unit quaternion, scalar last, with
  translation in metres. calibration-rs pins this form in
  `vision-calibration-core/src/math/mod.rs`, and its app's `se3.ts` consumes it.
- ADR 0009's flat `[qx,qy,qz,qw,tx,ty,tz]` order is the optimizer's parameter
  layout, not the JSON form.

### Attachment

Every placeable entity carries two fields:

```json
{ "parent": "<FrameRef>", "parent_se3_self": { "rotation": [...], "translation": [...] } }
```

This covers cameras, rigs, lasers, lights, targets, parts, and robot bases.

`FrameRef` has three forms:

- `"world"`, the root.
- `"<robot_id>/<link_name>"`, a link of a robot. `link_name` comes from the
  robot's URDF.
- `"<entity_id>"`, any other entity. A rig, for example, is the parent of its
  cameras.

Rules for ids and the tree:

- Robot ids and entity ids share one namespace and must be unique. They match
  `[A-Za-z0-9_-]+`, so they contain no `/`, and they are not `world`.
- The attachment graph must be a tree rooted at `world`. Every parent
  resolves, and there are no cycles. `etendue validate` rejects anything else.
- A robot's own `parent_se3_self` is its `parent_se3_base`. Its link poses come
  from forward kinematics ([ADR 0003](0003-baking.md)).

### Hand-eye topologies without special cases

Each robot declares a `tcp_link`, the link whose pose the controller reports.
The dataset's `robot_poses` are exported as `base_se3_<tcp_link>` from the same
kinematics, with
`PoseConvention { transform: t_base_tcp, rotation_format: quat_xyzw, translation_units: m }`.

**Eye-in-hand**

- The rig attaches to `"<robot>/<tcp_link>"` with `parent_se3_self = tcp_se3_rig`.
  This is exactly calibration-rs `gripper_se3_rig`.
- If the rig attaches to a fixed-joint descendant of the TCP link, the exporter
  folds the fixed offset into `gripper_se3_rig`.
- The target attaches to `world` or to a fixture.

**Eye-to-hand**

- The rig attaches to `world` or to a fixture.
- The target attaches to `"<robot>/<tcp_link>"`.
- The exporter derives calibration-rs `rig_se3_base = (world_se3_rig)⁻¹ · world_se3_base`
  from the resolved world poses.

**Rig cameras**

- Cameras attach to their rig, so `parent_se3_self = rig_se3_cam`. That has the
  same direction as `DeviceSpec` `CameraMountSpec.rig_se3_cam`.
- calibration-rs's pipeline inverts it to `cam_se3_rig` itself.

### Relation to the existing kernel

- The `pose` fields on `CameraEntity`, `LaserEntity`, `TargetEntity`, and
  `MeshTarget` are documented as `world_se3_self`. They are **not renamed**.
- `etendue-core` builds its analysis `Scene` from a resolved spec, so frames
  flatten to world poses. The dependency runs only in that direction
  ([ADR 0001](0001-pivot.md) §2).

### Frames and units

- World frame: right-handed, **+Z up**, metres.
- Camera frame: calibration-rs/OpenCV (+Z forward, +X right, +Y down).
- The local conventions for laser and target frames stay as documented in
  `etendue-core/src/scene/entity.rs`.
- `etendue-scene` files use metres and radians.
- `DeviceSpec` datasheet units (`_mm`, `_um`, `_deg`) are consumed through
  calibration-rs's own derivation layer (`vision-calibration-pipeline`
  `device_seed`), never re-implemented here.

## Consequences

- Both hand-eye topologies, fixture-mounted rigs, and robot-held targets are
  just attachments, not code paths.
- The export step, not the schema, computes the derived calibration-rs
  transforms (`gripper_se3_rig`, `rig_se3_base`, `cam_se3_rig`). Each
  derivation gets a round-trip test against the resolved world poses.
- Choosing the wrong `tcp_link` silently corrupts hand-eye ground truth.
  `etendue validate` therefore requires eye-in-hand rigs to attach to the TCP
  link or a fixed-joint descendant of it.

## Alternatives considered

- **A topology enum** (`EyeInHand`, `EyeToHand`, `Static`, …) on the scene.
  Rejected. It multiplies with rigs, lights, and fixtures, and it duplicates
  information the tree already holds.
- **World poses only, with no tree.** Rejected because it cannot express
  robot motion.
- **Renaming `pose` to `world_se3_self` in `etendue-core`.** Rejected: it
  churns the API and the saved scenes for no semantic gain.

## References

- calibration-rs ADR 0009 (pose naming), ADR 0016 (DatasetSpec), ADR 0023
  (DeviceSpec seed derivation)
- `vision-calibration-dataset` `device_spec.rs` (`HandeyeMountSpec`,
  `RigLayoutSpec`) and `spec.rs` (`PoseConvention`)
- `docs/pivot/PLAN.md` §3, §4
