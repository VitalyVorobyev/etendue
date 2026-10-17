# Datasets

A dataset directory holds the ground truth from `etendue gt`, the images from `etendue render`,
and the detections from `etendue detect`. The studio's Dataset tab writes the same files, plus the
scenario it used (`scenario.json`).

```text
out/
├── dataset.json          calibration-rs DatasetSpec: what to calibrate, where the images and poses are
├── robot_poses.json      the robot's tool pose at each capture (hand-eye datasets)
├── gt.json               the ground truth
├── job.json              the Blender render job
├── exr/<camera>/…        canonical renders (linear EXR)
├── images/<camera>/<capture>.png
└── features.json         detected board points per view
```

## `dataset.json`

A calibration-rs dataset manifest. Its `topology` follows from the scene:

| Scene | Topology |
|---|---|
| One camera, no robot | `planar_intrinsics` (or `scheimpflug_intrinsics` for a tilted sensor) |
| Several cameras, no robot | `rig_extrinsics` |
| One camera and a robot | `single_cam_handeye` |
| Several cameras and a robot | `rig_handeye` |
| One line laser on a hand-eye rig of two or more cameras | `rig_handeye_laserline` (other laser setups are not emitted yet) |

It names the target (the board, as calibration-rs describes it), each camera's images, and the
robot poses file with its convention. calibration-rs can load it directly.

## `robot_poses.json`

One row per capture: `capture` id and the tool pose in the robot's base frame, `tx ty tz` (metres)
and `qx qy qz qw`. `etendue scenario from-poses` reads the same rows.

## `gt.json`

The exact answer, for comparing a calibration against:

- `cameras`: each camera's `id`, `params` (its full model), `resolution` and `cam_se3_rig`;
- `handeye`: `eye_in_hand` (with `gripper_se3_rig` and `base_se3_target`) or `eye_to_hand`;
- `target`: the board's `points` in the board frame (`position_m`, and calib-targets' `grid` label
  and `id`);
- `lasers`: each laser's plane in the rig frame (`rig_plane`) and in each camera's frame
  (`camera_planes`: unit `normal` and `distance`, with `n·p + d = 0`);
- `captures`: per capture its `id`, time `t`, `base_se3_tcp` and `world_se3_target`, and per
  camera a view: the `image` path, `world_se3_camera`, and every board `point` with its `pixel` —
  or why it is not seen (`occluded`: `behind_camera`, `outside_image`, `back_facing`). Laser views
  add the stripe's points (`laser_points`);
- `pixel_centre`: the pixel convention (`integer`: pixel `i` at coordinate `i`).

Visibility is geometric: a point is visible if it projects into the image from the board's front
side. Occlusion by the robot's own links is not modelled.

The rig frame in `gt.json` is etendue's (the scene's rig entity). calibration-rs reports rig
quantities in its reference camera's frame (the first camera); compare through `cam_se3_rig`, or
compare per-camera quantities.

## `features.json`

`etendue detect`'s output, laid out like `gt.json`'s captures: per view, its `status` (`ok`,
`no_image`, `no_board`, `partial`, `ambiguous`), the detected `points` (`point` index into
`gt.json`'s target points, and `pixel`), and a `check` against the ground truth (`matched`,
`mislabelled`, `unmatched`, RMS and maximum distance).
