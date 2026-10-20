# Scenes and scenarios

A cell is described by two JSON documents: a **scene** (what is there) and a **scenario** (what
happens). Both carry a `version` (1) and have JSON Schemas,
[`scene.schema.json`](https://github.com/VitalyVorobyev/etendue/tree/main/schemas/scene.schema.json)
and
[`scenario.schema.json`](https://github.com/VitalyVorobyev/etendue/tree/main/schemas/scenario.schema.json);
point your editor at them for validation and completion. To check both:

```bash
etendue validate examples/eye_in_hand_ur5e/scene.json examples/eye_in_hand_ur5e/scenario.json
```

```text
scene ok: 1 robot(s), 1 rig(s), 2 camera(s), 0 laser(s), 0 light(s), 1 target(s), 0 part(s)
scenario ok: 20 step(s), 18.610 s, 1862 sample(s) at dt = 0.01 s, 10 capture(s)
```

## Conventions

- **Units**: metres and radians, unless a field's name says otherwise (`wavelength_nm`,
  `square_size_mm`, `power_w`).
- **World frame**: right-handed, +Z up.
- **Camera frame**: +Z forward, +X right, +Y down (the OpenCV convention).
- **Poses** are rigid transforms named `a_se3_b`, mapping coordinates in frame `b` to frame `a`,
  written as `{"rotation": [qx, qy, qz, qw], "translation": [x, y, z]}`: a unit quaternion, then
  metres.

[Frames and poses](models/frames.md) explains them in full.

## The scene

The scene has an optional `description` and one list per kind of item. Every item has a unique
`id`, a `parent` frame and its pose in that frame, `parent_se3_self`. A parent is `"world"`,
another item's id (a rig, a frame), or a robot link, `"<robot>/<link>"` (for example
`"ur5e/tool0"`). Together they form the frame tree.

| List | An item's fields, besides `id`, `parent` and `parent_se3_self` |
|---|---|
| `frames` | none: a named frame for a fixture, a stand, a mounting plate or a tool offset. |
| `robots` | `manifest`: the path to the robot's `robot.json`, relative to the scene; `initial_q`: the joint positions at the start, in the manifest's joint order (default all zero). The pose places the robot's base link. |
| `rigs` | An optional `layout` that makes its cameras and lasers ([below](#rig-layouts)). |
| `cameras` | `resolution` `[width, height]`; `params`, the camera model ([below](#cameras)); optionally `optics`, its physical optics. |
| `lasers` | A line laser: `fan_half_angle` (rad, below π/2), `fan_length` (its reach, m), `wavelength_nm`, `beam_waist_m` (the 1/e² radius); optionally `waist_distance_m` (where it is focused: the waist's distance from the laser along each ray; default 0, at the laser), `power_w` (for laser images; default 0.5 mW) and `camera` (the one camera that images it, in a rig with one laser per camera). The fan lies in the laser's local `x = 0` plane and opens symmetrically about +Z. |
| `lights` | `power_w`, linear RGB `color` (each 0–1), and a `shape`: `point` (`radius_m`), `spot` (`cone_angle`, the full angle; `blend`, the soft edge, 0–1) or `area` (`size_m` `[x, y]`, emitting along +Z). |
| `targets` | `geometry`: a `printed` target, a `board` or a `rectangle` ([below](#targets)). |
| `parts` | `mesh`: a glTF binary file, relative to the scene, in the part's frame, in metres. |

A short eye-in-hand scene:

```json
{
  "version": 1,
  "description": "A stereo rig on a UR5e's flange views a chessboard on the table.",
  "robots": [{ "id": "ur5e", "parent": "world", "parent_se3_self": { "rotation": [0, 0, 0, 1], "translation": [0, 0, 0] },
               "manifest": "../../assets/robots/ur5e/robot.json", "initial_q": [3.14159, -1.5708, 1.5708, -1.5708, -1.5708, 0] }],
  "rigs":   [{ "id": "rig", "parent": "ur5e/tool0", "parent_se3_self": { "rotation": [0, 0, 0, 1], "translation": [0, 0, 0.03] } }],
  "cameras": [{ "id": "cam", "parent": "rig", "parent_se3_self": { "rotation": [0, 0, 0, 1], "translation": [0, 0, 0] },
                "resolution": [1280, 1024], "params": { "...": "a camera model, below" } }],
  "targets": [{ "id": "board", "parent": "world", "parent_se3_self": { "rotation": [0, 0, 0, 1], "translation": [0.45, 0.15, 0] },
                "geometry": { "type": "board", "board": { "kind": "chessboard", "rows": 6, "cols": 9, "square_size_m": 0.025 } } }]
}
```

The [examples](examples.md) are complete scenes to start from.

## Cameras

A camera's `params` is a calibration-rs camera model, the model calibration estimates:

| Part | Choices |
|---|---|
| `projection` | `pinhole` |
| `distortion` | `none`; `brown_conrady5` (`k1, k2, k3, p1, p2`); `rational` (`k1`–`k6`, `p1, p2`); `thin_prism`; `division`. Each with `iters`, the iteration cap for undistortion. |
| `sensor` | `identity`; `scheimpflug` (`tilt_x`, `tilt_y`, radians); `homography` (`h`, 3 × 3) |
| `intrinsics` | `fx`, `fy`, `cx`, `cy` (pixels), `skew` |

[Camera models](models/camera_models.md) gives the equations.

A camera with `optics` is **physical**:

| Field | Meaning |
|---|---|
| `sensor` | `pixel_pitch_m`, and an optional `name` |
| `lens` | `focal_length_m`, `f_number`; `principal_gap_m` (from H′ to H, default 0), `entrance_pupil_m` (from H to the entrance pupil, default 0), `focusing` (`unit`: the whole lens moves), an optional `name` |
| `focus_distance_m` | from the image plane to the plane in focus, as a focus scale reads it |
| `tilt_rad` | the sensor's tilt `[τx, τy]` |

Then the optics are the source of truth: `params` must hold the equivalent pinhole they give
(its focal lengths, skew and tilt are checked against them), and the camera's pose is its body's
mechanical frame at the image plane, not its entrance pupil. Cameras the Optics and Rigs
workspaces place in a cell, and the library's camera designs, are written this way. See [Physical cameras and the thick lens](physical_cameras.md).

## Targets

A target's `geometry` is one of three types. A planar target is centred on the local origin,
faces +Z, and its printed top edge is at +Y.

- **`printed`**: a target exactly as calib-targets prints it, with its print parameters in
  millimetres, for example
  `{"type": "printed", "target": {"kind": "puzzle_board", "rows": 10, "cols": 12, "square_size_mm": 15}}`.
- **`board`**: a board in calibration-rs's dataset vocabulary (sizes in metres): `chessboard`
  (`rows`, `cols` inner corners, `square_size_m`), `charuco`, `puzzleboard` or `ringgrid`.
- **`rectangle`**: a plain `width` × `height`, nothing to detect.

The `printed` kinds:

| `kind` | Fields |
|---|---|
| `chessboard` | `inner_rows`, `inner_cols`, `square_size_mm`; optional `inner_square_rel` |
| `charuco` | `rows`, `cols` (squares), `square_size_mm`, `marker_size_rel` (marker side over square side), `dictionary` (`DICT_4X4_50`, …); optional `border_bits` (1), `marker_layout` (`opencv_charuco`), `inner_square_rel` |
| `marker_board` | `inner_rows`, `inner_cols`, `square_size_mm`, `circles` (three `{i, j, polarity}`); optional `circle_diameter_rel`, `inner_square_rel` |
| `puzzle_board` | `rows`, `cols`, `square_size_mm`; optional `origin_row`, `origin_col`, `dot_diameter_rel` |
| `puzzlepole` | `circumference_squares` (a supported period: 12, 18, 24, 30, 36 or 42), `start_row`, `axial_squares`, `square_size_mm`; optional `axial_start_col`, `dot_diameter_rel` |

A PuzzlePole is a PuzzleBoard strip wrapped around a cylinder of circumference
`circumference_squares × square_size_mm`. Its frame is calib-targets' pole frame: the axis along
+Z from the first corner column, the seam at +X, the print facing outward.

Every `printed` kind is drawn (in the studio and in Blender renders), has exact ground truth and
is detected by `etendue detect`. A `board` chessboard or ChArUco board is printed, rendered and
detected as its printed equivalent; a `board` PuzzleBoard named by layout and a ring grid have no
detector. A calibration-rs dataset (`dataset.json`) is written only for the targets calibration-rs
takes: a `board`, a printed chessboard (without `inner_square_rel`), and a printed ChArUco board
in OpenCV's layout with one-bit borders. For the other kinds `etendue gt` writes the ground
truth alone.

## Rig layouts

A rig's `layout` makes its cameras and lasers: mono, stereo, or a ring of `count` cameras around
the rig's +Z axis, all with the same camera.

| Field | Meaning |
|---|---|
| `count` | Number of cameras: 1 (mono), 2 (stereo), 3, 6, … |
| `radius_m` | Distance of each camera from the rig axis (0 only for one camera). |
| `working_distance_m` | Where the camera axes meet the rig axis (with the default tilt), and where the lasers aim: `(0, 0, working_distance_m)` in the rig frame. |
| `tilt_rad` | Angle of each camera's axis to the rig axis. By default `atan(radius / working distance)`, so the axes converge on the working point; 0 gives a parallel rig. |
| `phase_rad` | Azimuth of the first camera about the axis (default 0). |
| `image_x` | Which way each camera's image +X points: `tangential` (the default: along the ring, counter-clockwise seen from +Z), `radial` (away from the axis), or `common` (the rig's +X as each camera sees it: classical stereo, whose image rows line up for two cameras on the rig's x axis). |
| `camera` | The camera every position gets: `params`, `resolution` and optionally `optics`. |
| `lasers` | One line laser per camera, if any (below). |

The `lasers` of a layout:

| Field | Meaning |
|---|---|
| `fan_half_angle`, `fan_length`, `wavelength_nm`, `beam_waist_m` | As for a laser; optionally `waist_distance_m` and `power_w`. |
| `camera_se3_laser` | Each laser fixed to its camera's body at this pose, as a triangulation head carries it. |
| `radius_m`, `azimuth_rad` | Without `camera_se3_laser`: each laser stands `radius_m` from the axis, `azimuth_rad` from its camera, aimed at the working point, its fan across its camera's image. |

The cameras are named `<rig>_cam<i>` and the lasers `<rig>_laser<i>`. Each laser names its
camera, so a dataset gives every camera its own laser plane to calibrate. The cameras and lasers
are what count: the layout is kept only so it can be edited again.

In the studio, select a rig and press **Lay out cameras** (Setup workspace): every change of a
field lays the rig out again. The Rigs workspace designs a layout before it goes into a cell.

## Robots

A robot is a URDF plus a manifest (`robot.json`) that names its base link (the frame the
controller reports tool poses in), its tool link, its joints in controller order with their
position, velocity, acceleration and jerk limits, and its link meshes. The robot library has the
Universal Robots UR5e, the ABB IRB 1200-5/0.90 and the igus ReBeL 6DOF; see [Robots](robots.md).

## The scenario

A scenario has a `version`, a baking sample period `dt` (seconds), an optional `description`, a
list of `steps`, and optionally the `generator` that made them. Every move names the `robot` it
moves:

| Step | Fields | Meaning |
|---|---|---|
| `ptp_joints` | `q`, `speed_scale` | Point-to-point move to joint positions `q`. |
| `ptp_pose` | `base_se3_tool`, `q`, `speed_scale` | Point-to-point move to a tool pose in the robot's base frame, solved by inverse kinematics or reached with the stored joints `q`. |
| `lin` | `base_se3_tool`, `speed_scale` | Straight-line tool move to a pose. |
| `wait` | `duration_s` | Hold for a time. |
| `capture` | `id` | Every camera takes an image. The id defaults to `cap_000`, `cap_001`, … |

For example:

```json
{"type": "ptp_joints", "robot": "ur5e", "q": [3.14, -1.57, 1.57, -1.57, -1.57, 0], "speed_scale": 0.5}
```

**Timing.** `speed_scale`, in (0, 1], is the fraction of the joint velocity, acceleration and jerk
limits a move uses (default 1). Every move is time-optimal and jerk-limited. A point-to-point
move is a straight line in joint space, all joints starting and stopping together. A
straight-line move follows the tool line exactly, timed so no joint exceeds its limits. The robot
stops for each capture. See [Motion](models/motion.md).

**Stored joint configurations.** An arm reaches most poses in several configurations (elbow up
or down, wrist flipped). Without `q`, inverse kinematics chooses one each time the scenario is
compiled, starting from where the robot is, and that can land on another configuration and swing
the arm far. With `q`, the move goes there, and a `q` that does not reach the pose is refused.
The studio's **Resolve IK** and `etendue export --resolved` store every `q`; jogged and generated
poses store theirs. See [Robot kinematics](models/kinematics.md).

**Generated scenarios.** A scenario made by the pose generator keeps its spec in `generator`, so
the studio's **Generate poses** and `etendue scenario sphere --spec scenario.json` can edit and
run it again. The steps, not the spec, are what is executed. See
[`scenario sphere`](cli.md#scenario-sphere).

**Baking.** `etendue bake` compiles a scenario into a **baked scenario**: the pose
`world_se3_frame` of every frame at every sample, `dt` apart, with the captures marked. The
studio and the dataset tools work from it. See [File formats](formats.md).

To build a scenario from measured or planned tool poses, see
[`etendue scenario from-poses`](cli.md#scenario-from-poses).
