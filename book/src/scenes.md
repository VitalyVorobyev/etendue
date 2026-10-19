# Scenes and scenarios

A cell is described by two JSON documents: a **scene** (what is there) and a **scenario** (what
happens). Both carry a `version` and have JSON Schemas in the repository's
[`schemas/`](https://github.com/VitalyVorobyev/etendue/tree/main/schemas) folder
(`scene.schema.json`, `scenario.schema.json`); point your editor at them for validation and
completion. `etendue validate scene.json scenario.json` checks both.

## Conventions

- **Units**: metres and radians, unless a field's name says otherwise (`wavelength_nm`,
  `power_w`).
- **World frame**: right-handed, +Z up.
- **Camera frame**: +Z forward, +X right, +Y down (the OpenCV / calibration-rs convention).
- **Poses** are rigid transforms named `a_se3_b`, mapping coordinates in frame `b` to frame `a`,
  written as `{"rotation": [qx, qy, qz, qw], "translation": [x, y, z]}` (a unit quaternion, then
  metres).

## The scene

Every entity has an `id`, a `parent` frame and its pose in that frame, `parent_se3_self`. A parent
is `"world"`, another entity's id (a rig, say), or a robot link, `"<robot>/<link>"` (for example
`"ur5e/tool0"`). Together they form the frame tree.

| Entity | Fields |
|---|---|
| `frames` | Named auxiliary frames: fixtures, stands, mounting plates, tool offsets. |
| `robots` | `manifest` (path to the robot's `robot.json`, relative to the scene), `initial_q` (joint positions the scenario starts from). |
| `rigs` | A frame that groups cameras and lasers, e.g. a stereo rig on a robot flange. An optional `layout` lays it out (below). |
| `cameras` | `params`: a calibration-rs camera model (projection, distortion — Brown–Conrady, rational, thin prism, division — sensor — identity, Scheimpflug tilt, homography — and intrinsics `fx, fy, cx, cy, skew`); `resolution` `[width, height]`. |
| `lasers` | A line laser: `fan_half_angle`, `fan_length`, `wavelength_nm`, `beam_waist_m`, and optionally `waist_distance_m` (where it is focused: the waist's distance from the laser along each ray; default 0, at the laser), `power_w` (laser images; default 0.5 mW) and `camera` (the one camera that images it). The fan lies in the laser's local `x = 0` plane and opens symmetrically about local +Z. |
| `lights` | `power_w`, linear RGB `color`, and a `shape`: `point` (with a radius), `spot` (cone angle, soft edge) or `area` (a rectangle). |
| `targets` | `geometry`: a `printed` target — any target calib-targets prints, with its print parameters in millimetres (below) — a calibration `board` in calibration-rs's dataset vocabulary, or a plain `rectangle`. A planar target is centred on the local origin, facing +Z, its top edge at +Y. |
| `parts` | Static meshes (glTF binary, relative to the scene) for context: tables, fixtures, stands. |

A short eye-in-hand scene:

```json
{
  "version": 1,
  "robots": [{ "id": "ur5e", "parent": "world", "parent_se3_self": { "rotation": [0, 0, 0, 1], "translation": [0, 0, 0] },
               "manifest": "../../assets/robots/ur5e/robot.json", "initial_q": [3.14159, -1.5708, 1.5708, -1.5708, -1.5708, 0] }],
  "rigs":   [{ "id": "rig", "parent": "ur5e/tool0", "parent_se3_self": { "rotation": [0, 0, 0, 1], "translation": [0, 0, 0.03] } }],
  "cameras": [{ "id": "cam", "parent": "rig", "parent_se3_self": { "rotation": [0, 0, 0, 1], "translation": [0, 0, 0] },
                "resolution": [1280, 1024], "params": { "...": "a calibration-rs camera model" } }],
  "targets": [{ "id": "board", "parent": "world", "parent_se3_self": { "rotation": [0, 0, 0, 1], "translation": [0.45, 0.15, 0] },
                "geometry": { "type": "board", "board": { "kind": "chessboard", "rows": 6, "cols": 9, "square_size_m": 0.025 } } }]
}
```

The [examples](examples.md) are complete scenes to start from.

## Targets

A `printed` target is written exactly as `calib-targets-print` writes it (`kind` and its fields,
sizes in millimetres), for example
`{"type": "printed", "target": {"kind": "puzzle_board", "rows": 10, "cols": 12, "square_size_mm": 15}}`.

| `kind` | Fields |
|---|---|
| `chessboard` | `inner_rows`, `inner_cols`, `square_size_mm` |
| `charuco` | `rows`, `cols`, `square_size_mm`, `marker_size_rel`, `dictionary` (`DICT_4X4_50`, …) |
| `marker_board` | `inner_rows`, `inner_cols`, `square_size_mm`, `circles` (three `{i, j, polarity}`), `circle_diameter_rel` |
| `puzzle_board` | `rows`, `cols`, `square_size_mm`, `origin_row`, `origin_col` |
| `puzzlepole` | `circumference_squares` and `start_row` (a supported period: 12, 18, 24, 30, 36 or 42 squares), `axial_squares`, `square_size_mm` |

A PuzzlePole is a PuzzleBoard strip wrapped around a cylinder of circumference
`circumference_squares × square_size_mm`. Its frame is calib-targets' pole frame: the axis along
+Z from the first corner column, the seam at +X, the print facing outward.

Every kind is drawn (in the studio and in Blender renders) and has exact ground truth. A dataset
for calibration-rs (`dataset.json`) is written for a chessboard and a ChArUco board; for the other
kinds `etendue gt` writes the ground truth alone.

## Rig layouts

A rig's `layout` makes its cameras and lasers: mono, stereo, or a ring of `count` cameras
around the rig's +Z axis, all with the same camera model.

| Field | Meaning |
|---|---|
| `count` | Number of cameras: 1 (mono), 2 (stereo), 3, 6, … |
| `radius_m` | Distance of each camera from the rig axis (0 for one camera). |
| `phase_rad` | Azimuth of the first camera about the axis. |
| `working_distance_m` | Where the camera axes meet the rig axis, and where the lasers aim: `(0, 0, working_distance_m)` in the rig frame. |
| `tilt_rad` | Angle of each camera's axis to the rig axis; by default `atan(radius / working distance)`, so the axes converge on the working point. |
| `image_x` | `tangential` (the default: image +X along the ring) or `radial` (away from the axis). |
| `camera` | `params` and `resolution` of every camera. |
| `lasers` | One line laser per camera: `radius_m` from the axis at `azimuth_rad` from its camera, aimed at the working point, its fan across its camera's image; `fan_half_angle`, `fan_length`, `wavelength_nm`, `beam_waist_m` (and `waist_distance_m`, `power_w`) as for a laser. |

The cameras are `<rig>_cam<i>` and the lasers `<rig>_laser<i>`; each laser names its camera
(`camera`), so a dataset gives every camera its own laser plane to calibrate. In the studio,
select a rig and use **Lay out cameras**: every change lays the rig out again.

## Robots

A robot is a URDF plus a manifest (`robot.json`, schema `robot_manifest.schema.json`) that names
its base link (the frame the controller reports tool poses in), its tool link, joint limits
(position, velocity, acceleration and jerk), and its link meshes. The robot library
(`assets/robots/`) has the Universal Robots UR5e, the ABB IRB 1200-5/0.90 and the igus ReBeL 6DOF;
the FAIRINO FR5 can be built locally (see [Robots](robots.md)). Inverse kinematics is damped least
squares with deterministic restarts, or analytic for robots with an OPW geometry.

## The scenario

A scenario has a `version`, a baking sample period `dt` (seconds) and a list of `steps`. Every move
names the `robot` it moves:

| Step | Meaning |
|---|---|
| `ptp_joints` | Point-to-point move to joint positions `q`. |
| `ptp_pose` | Point-to-point move to a tool pose, `base_se3_tool` (solved by inverse kinematics, or the stored joint configuration `q`). |
| `lin` | Straight-line tool move to a pose. |
| `wait` | Hold for `duration_s` seconds. |
| `capture` | Every camera takes an image (optionally with an `id`). |

For example, `{"type": "ptp_joints", "robot": "ur5e", "q": [3.14, -1.57, 1.57, -1.57, -1.57, 0], "speed_scale": 0.5}`.
Moves take a `speed_scale`, the fraction of the joint velocity, acceleration and jerk limits to
use. Every move is time-optimal and jerk-limited (Ruckig): a point-to-point move is a straight
line in joint space, every joint starting and stopping together; a straight-line move follows the
tool line exactly, timed so no joint exceeds its limits. The robot stops for each capture.

A `ptp_pose` may carry `q`, the joint configuration that reaches the pose. Without it, inverse
kinematics chooses one each time the scenario is compiled, from where the robot is; that can land
on another branch of the arm (elbow up instead of down) and swing it far. With it, the move goes
there, and the compiler refuses a `q` that does not reach the pose. The studio's **Resolve IK**
and `etendue export --resolved` store every `q`.

`etendue bake` compiles a scenario into a **baked scenario** (`baked_scenario.schema.json`): the
pose `world_se3_frame` of every frame at every sample, `dt` apart, with the captures marked. The
studio and the dataset tools work from it.

To build a scenario from measured or planned tool poses, see `etendue scenario from-poses` in
[Command line](cli.md).
