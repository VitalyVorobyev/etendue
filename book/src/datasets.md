# Datasets

A dataset is a folder. `etendue gt` writes its ground truth, `etendue render` its images, and
`etendue detect` its detections; `etendue calibrate` reads it. The studio's **Generate**
(Calibrate workspace) runs the first three into one folder and also keeps the scene and scenario
it used.

```text
out/
├── dataset.json                       calibration-rs dataset manifest (gt)
├── gt.json                            the ground truth (gt)
├── robot_poses.json                   the robot's tool pose at each capture (gt, robot cells)
├── scene.json, scenario.json          the scene and scenario used (the studio)
├── job.json                           the Blender render job (render)
├── exr/<camera>/<capture>.exr         Blender's linear renders (render)
├── exr/<laser>/view<i>/<capture>.exr  depth views from each laser, for its shadows (render)
├── images/<camera>/<capture>.png      the camera images (render)
├── images/<camera>/<capture>_laser.png   the laser images, for cameras that image a laser (render)
├── features.json                      the detected target points (detect)
├── laser_features.json                the detected laser stripes (detect, laser datasets)
└── runs/<name>/                       calibration runs kept with the dataset (calibrate --run, the studio)
```

`render` also leaves its Blender script in a hidden `.etendue_blender/` folder. A new studio run
starts from an empty dataset: the previous run's images and detections are removed.

## The calibration problem

The kind of problem a dataset poses (its **topology**) follows from where the cameras, the
target and the lasers hang in the scene:

| Scene | Topology |
|---|---|
| One camera, no robot | `planar_intrinsics` (`scheimpflug_intrinsics` for a tilted sensor) |
| Several cameras, no robot | `rig_extrinsics` |
| One camera and a robot | `single_cam_handeye` |
| Several cameras and a robot | `rig_handeye` |
| One camera carrying a line laser, and a robot (a triangulation head) | `laserline_device` |
| Several cameras with lasers, and a robot | `rig_handeye_laserline` |

A robot makes it a hand-eye problem: **eye-in-hand** when the cameras ride on the robot,
**eye-to-hand** when the target does. Cameras and target both on robots is refused. Lasers must
be on the hand-eye rig (or on the single camera), and there must be either one laser that every
camera images, or one laser per camera, each naming its `camera`. Each camera then images, and
calibrates, its own laser.

A dataset needs exactly one target that is not a plain rectangle, and at least one capture.

## `dataset.json`

A [calibration-rs](https://github.com/VitalyVorobyev/calibration-rs) dataset manifest. It names:
- the `topology`;
- the `target`, as calibration-rs describes it;
- each camera's `images` (`images/<camera>/<capture>.png`), and for a laser dataset its
  `laser_images`;
- for a robot cell, the robot poses file (`robot_poses.json`), its columns and its
  `pose_convention`: the tool pose in the robot's base frame, quaternion `x, y, z, w`, metres,
  paired with the images by index.

calibration-rs loads it directly. It is written only for targets calibration-rs takes (a
chessboard, a ChArUco board in OpenCV's layout); for the other target kinds `gt.json` is written
alone ([Targets](scenes.md#targets)).

## `robot_poses.json`

One row per capture: the `capture` id and the tool pose in the robot's base frame, `tx ty tz`
(metres) and `qx qy qz qw`. `etendue scenario from-poses` reads the same rows.

```json
[{"capture": "s1_t1_p2_r1", "tx": 0.3048, "ty": 0.3562, "tz": 0.5211,
  "qx": -0.9388, "qy": 0.1230, "qz": -0.1141, "qw": -0.3010}, …]
```

## `gt.json`

The exact answer, for comparing a calibration with:

- `topology` and `pixel_centre` (`integer`: pixel `i` is centred at coordinate `i`);
- `cameras`: each camera's `id`, `params` (its full camera model), `resolution`,
  `cam_se3_rig` (on a rig), and, for a physical camera, `mech_se3_projection`: where its entrance
  pupil (the frame calibration recovers) sits in its body's frame;
- `rig`: the rig the cameras are mounted on;
- `handeye`: `eye_in_hand` (with `gripper_se3_rig` and `base_se3_target`) or `eye_to_hand`
  (with `rig_se3_base` and `gripper_se3_target`), the robot and its tool link;
- `target`: its `id`, its `geometry`, and its `points` in the target frame (`position_m`, with
  calib-targets' `grid` label or `id`);
- `lasers`: each laser's `id`, the `camera` that images it (one laser per camera), and its plane
  in the rig frame (`rig_plane`) and in each camera's frame (`camera_planes`): a unit `normal`
  and a `distance`, with `n·p + d = 0`;
- `captures`: per capture its `id`, time `t`, `base_se3_tcp` and `world_se3_target`, the laser
  stripes on the target (`laser_stripes`: points and the beam's width at each), and per camera a
  view: the `image` path, `world_se3_camera`, and every target `point` with its `pixel`, or why
  it is not seen (`occluded`: `behind_camera`, `outside_image`, `back_facing`). Laser views add
  the `laser_image` path and the stripe's projected points (`laser_points`).

Visibility is geometric: a point is visible when it projects into the image, at least 2 px from
the edge, from the target's front side. Occlusion by the robot's own links is not modelled.

**Rig frames.** The rig frame in `gt.json` is etendue's (the scene's rig). calibration-rs reports
rig quantities in its reference camera's frame (the first camera's). Compare through
`cam_se3_rig`, or compare per-camera quantities: `etendue calibrate` does this for you.

## `features.json`

`etendue detect`'s output, laid out like `gt.json`'s captures. Per view:
- its `status`: `ok`; `no_image` (not rendered); `no_board`; `partial` (too little of the board
  seen to label it); `ambiguous` (the board's orientation is not decided); `undecoded` (a grid
  was found but its markers, circles or code did not identify it);
- the detected `points` (`point`, an index into `gt.json`'s target points, and `pixel`);
- a `check` against the ground truth: `matched`, `mislabelled` (nearer another point's true
  pixel), `unmatched` (no true pixel near), and the RMS and largest distance from the true pixels.

`detect` prints the checks as a table, for example on the rendered `laser_ur5e` example:

```text
| camera | views ok | no board | partial | ambiguous | undecoded | points | mislabelled | unmatched | off board | RMS vs GT px | max px |
|---|---|---|---|---|---|---|---|---|---|---|---|
| cam_left | 24/24 | 0 | 0 | 0 | 0 | 1296 | 0 | 0 | 0 | 0.0578 | 0.1374 |
| cam_right | 24/24 | 0 | 0 | 0 | 0 | 1296 | 0 | 0 | 0 | 0.0582 | 0.1372 |
```

## `laser_features.json`

The laser stripes `etendue detect` extracts from the laser images: per capture and view, the
stripe's centre `pixels`, with a check against the true stripe. The printed table gives, per
camera, the pixels found, the outliers (more than 3 px from the true stripe), the RMS, mean and
largest distance, and the fitted stripe width over the beam's:

```text
camera       images   pixels outliers    rms px   mean px    max px  width/beam
cam_left         24     7147        0    0.0784   -0.0010     1.589  1.034 (1.031–1.038)
cam_right        24     7133        0    0.0814   +0.0001     2.241  1.031 (1.030–1.033)
```

## Calibrating a dataset

`etendue calibrate <dir>` (or **Calibrate** in the studio) calibrates a dataset with a robot and
compares the result with `gt.json`; see [Calibration](calibration.md). Runs kept with
`--run` go in the folder you name, conventionally `runs/<name>/` in the dataset or the project.

Any other tool that reads calibration-rs datasets can calibrate `dataset.json` too. Compare its
result with `gt.json`, minding the [rig frame](#gtjson).
