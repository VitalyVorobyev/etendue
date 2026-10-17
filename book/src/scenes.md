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
| `robots` | `manifest` (path to the robot's `robot.json`, relative to the scene), `initial_q` (joint positions the scenario starts from). |
| `rigs` | A frame that groups cameras and lasers, e.g. a stereo rig on a robot flange. |
| `cameras` | `params`: a calibration-rs camera model (projection, distortion — Brown–Conrady, rational, thin prism, division — sensor — identity, Scheimpflug tilt, homography — and intrinsics `fx, fy, cx, cy, skew`); `resolution` `[width, height]`. |
| `lasers` | A line laser: `fan_half_angle`, `fan_length`, `wavelength_nm`, `beam_waist_m`. The fan lies in the laser's local `x = 0` plane and opens symmetrically about local +Z. |
| `lights` | `power_w`, linear RGB `color`, and a `shape`: `point` (with a radius), `spot` (cone angle, soft edge) or `area` (a rectangle). |
| `targets` | `geometry`: a calibration `board` (chessboard, ChArUco, …, as calib-targets prints it: centred on the local origin, facing +Z, its top edge at +Y) or a plain `rectangle`. |
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

## Robots

A robot is a URDF plus a manifest (`robot.json`, schema `robot_manifest.schema.json`) that names
its base link (the frame the controller reports tool poses in), its tool link, joint limits
(position, velocity, acceleration), and its link meshes. The UR5e and the ABB IRB 1200 are in the
robot library (`assets/robots/`). Inverse kinematics is damped least squares with deterministic
restarts, or analytic for robots with an OPW geometry.

## The scenario

A scenario is a list of `steps` for the scene's robots:

| Step | Meaning |
|---|---|
| `ptp_joints` | Point-to-point move to joint positions `q`. |
| `ptp_pose` | Point-to-point move to a tool pose, `base_se3_tool` (solved by inverse kinematics). |
| `lin` | Straight-line tool move to a pose. |
| `wait` | Hold for `duration_s` seconds. |
| `capture` | Every camera takes an image (optionally with an `id`). |

Moves take a `speed_scale`, the fraction of the joint velocity and acceleration limits to use.
Point-to-point moves are synchronised trapezoidal profiles; straight-line moves are spline-timed.
The robot stops for each capture.

`etendue bake` compiles a scenario into a **baked scenario** (`baked_scenario.schema.json`): the
pose `world_se3_frame` of every frame at every sample, `dt` apart, with the captures marked. The
studio and the dataset tools work from it.

To build a scenario from measured or planned tool poses, see `etendue scenario from-poses` in
[Command line](cli.md).
