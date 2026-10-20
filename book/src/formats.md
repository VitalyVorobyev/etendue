# File formats

Every document etendue reads or writes is JSON, except the exported CSV files and the
`etendue.toml` settings. The documents you write by hand have published JSON Schemas in the
repository's [`schemas/`](https://github.com/VitalyVorobyev/etendue/tree/main/schemas) folder:
point your editor at them for validation and completion.

## Documents with a schema

| Document | Schema | Version | Written by | Read by |
|---|---|---|---|---|
| Scene (`scene.json`) | [`scene.schema.json`](https://github.com/VitalyVorobyev/etendue/tree/main/schemas/scene.schema.json) | 1 | you; the studio (**Save**) | `validate`, `bake`, `export`, `gt`, `render`, `scenario`, `project check`; the studio |
| Scenario (`scenario.json`) | [`scenario.schema.json`](https://github.com/VitalyVorobyev/etendue/tree/main/schemas/scenario.schema.json) | 1 | you; the studio; `scenario from-poses`; `scenario sphere`; `export --resolved` | `validate`, `bake`, `gt`, `render`, `export`; the studio |
| Baked scenario | [`baked_scenario.schema.json`](https://github.com/VitalyVorobyev/etendue/tree/main/schemas/baked_scenario.schema.json) | 1 | `bake`; the studio (**Export › Baked motion**) | your own tools |
| Robot manifest (`robot.json`) | [`robot_manifest.schema.json`](https://github.com/VitalyVorobyev/etendue/tree/main/schemas/robot_manifest.schema.json) | 1 | the robot build tool ([Robots](robots.md)) | every command that reads a scene; the studio |
| Project manifest (`etendue-project.json`) | [`project.schema.json`](https://github.com/VitalyVorobyev/etendue/tree/main/schemas/project.schema.json) | 1 | you; the studio (**New project…**, **Save as a cell**) | `project check`; the studio |
| Library item (`library/<folder>/<id>.json`) | [`library.schema.json`](https://github.com/VitalyVorobyev/etendue/tree/main/schemas/library.schema.json) | 1 | you; the studio (**Save to library**) | `project check`; the studio |
| Capture poses (`export --captures *.json`) | [`captures.schema.json`](https://github.com/VitalyVorobyev/etendue/tree/main/schemas/captures.schema.json) | 1 | `export`; the studio (**Export › Captures**) | your robot controller or tools |
| Motion report | [`motion_report.schema.json`](https://github.com/VitalyVorobyev/etendue/tree/main/schemas/motion_report.schema.json) | 1 | `export --report`; the studio (**Export › Motion report**) | your own tools |

- **Scene** and **scenario**: [Scenes and scenarios](scenes.md).
- **Baked scenario**: `dt`, the `frames` (in tree order, `"world"` first), the `robots`, and the
  `samples` at `t = k · dt`, each with the pose `world_se3_frame` of every frame (in the order of
  `frames`), each robot's joints, and the capture, if one is taken at that sample. With
  `bake --derivatives`, the joint velocities and accelerations too.
- **Robot manifest**: `id`, `name`, the `urdf` path, `base_link` and `tcp_link`, the `joints` in
  controller order with their limits and `limit_source`, the link meshes (`visuals`), and the
  `source` and `license` of the description.
- **Project manifest** and **library items**: [Projects and the library](library.md).
- **Capture poses**: the `robot`, its `base_link`, `tcp_link` and `joint_names`, and per capture
  its id, time, tool pose in the base frame and joint positions. The CSV form is in
  [Command line](cli.md#export).
- **Motion report**: the total `duration_s`, the number of `captures`, and per step its timing,
  limit use, joint margins, distance from singularities and warnings
  ([Command line](cli.md#export)).

## Documents without a schema

| Document | Version | Written by | Read by | Described in |
|---|---|---|---|---|
| `gt.json`, the ground truth | 3 | `gt`; the studio | `detect`, `calibrate`; the studio | [Datasets](datasets.md#gtjson) |
| `dataset.json` | 1 (calibration-rs's) | `gt`; the studio | calibration-rs | [Datasets](datasets.md#datasetjson) |
| `robot_poses.json` | — | `gt`; the studio | `scenario from-poses`; calibration-rs | [Datasets](datasets.md#robot_posesjson) |
| `features.json` | 1 | `detect`; the studio | `calibrate --features`; the studio | [Datasets](datasets.md#featuresjson) |
| `laser_features.json` | 1 | `detect`; the studio | `calibrate --laser-features`; the studio | [Datasets](datasets.md#laser_featuresjson) |
| Calibration report (`report.json`) | 2 | `calibrate -o`, `calibrate --run`; the studio | the studio (kept runs) | [Calibration](calibration.md#the-json-report) |
| Run configuration (`config.json`) | 1 | `calibrate --run`; the studio | the studio (kept runs) | [Calibration](calibration.md#runs) |
| Calibration result (`result.json`) | calibration-rs's | `calibrate --run`; the studio | calibration-rs and your tools | [Calibration](calibration.md#runs) |
| Observation noise | — | you; the studio's noise form | `calibrate --noise` | [Calibration](calibration.md#the-noise-model) |
| Sensor model | — | you | `render --sensor`; the studio | [Rendering](rendering.md#sensor-model) |
| Pose generator spec | — | you; kept in a scenario's `generator` | `scenario sphere`; the studio | [Command line](cli.md#scenario-sphere) |
| Render job (`job.json`) | 4 | `render` | etendue's Blender script | — (internal) |

A newer etendue may raise a document's version. Library items and project manifests are read
version first, so a file from a newer etendue is refused because of its version, not because of a
field this etendue does not know. Scenes and scenarios are checked to be version 1.

## CSV exports

`export --captures *.csv` and `export --trajectory`, and the studio's matching **Export** items,
write CSV with a header row, in SI units. The columns are listed in
[Command line](cli.md#export).

## Poses on disk

Every pose — `parent_se3_self` in a scene, `base_se3_tool` in a scenario, the poses in the
ground truth and the reports — is written as a rotation quaternion and a translation:

```json
{"rotation": [qx, qy, qz, qw], "translation": [x, y, z]}
```

- The quaternion is `[x, y, z, w]`, scalar last, and must be of unit length.
- The translation is in metres.
- A pose named `a_se3_b` maps coordinates in frame `b` to frame `a`: a point `p_b` in `b` is
  `p_a = R · p_b + t` in `a`. So `parent_se3_self` places an item in its parent, and
  `base_se3_tool` is the tool's pose in the robot's base frame.

`robot_poses.json` and the CSV exports flatten the same pose into columns
(`tx, ty, tz, qx, qy, qz, qw`, and the CSV also as a rotation vector `rx, ry, rz`).
[Frames and poses](models/frames.md) explains the conventions.

## `etendue.toml`

A settings file, found by searching upward from the scene's folder, then from the working
directory. It pins the Blender version renders are made with:

```toml
[blender]
version = "5.2"
```

See [Installation](installation.md#blender).
