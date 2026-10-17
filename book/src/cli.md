# Command line

The `etendue` command runs each step on its own. `etendue <command> --help` lists every option.

```text
etendue validate <scene> [scenario]
etendue bake     <scene> <scenario> -o <baked.json> [--pretty]
etendue gt       <scene> <scenario> -o <dir>
etendue render   <scene> <scenario> -o <dir> [options]
etendue detect   <dir>
etendue scenario from-poses <poses> --robot <id> -o <scenario.json> [options]
```

## `validate`

Checks a scene — robot models, the frame tree, every entity — and, if given, compiles the
scenario in full: joint limits, inverse-kinematics reachability of every pose, timing. It prints a
summary or the list of problems.

## `bake`

Compiles the scenario and writes the baked scenario: `world_se3_frame` for every frame at every
sample. `--pretty` indents the JSON; `--derivatives` also records joint velocities and
accelerations.

## `export`

```bash
etendue export <scene.json> <scenario.json> [--robot <id>] \
    [--captures poses.csv|poses.json] [--trajectory trajectory.csv] \
    [--report report.json] [--resolved scenario.json]
```

Exports a scenario's motion, for a robot controller or a spreadsheet (SI units throughout):

| Option | Writes |
|---|---|
| `--captures` | The tool pose in the robot's base frame and the joint positions at every capture. CSV columns: `capture, t, x, y, z`, the rotation as a quaternion `qx, qy, qz, qw` and as a rotation vector `rx, ry, rz` (axis × angle, the UR convention), then one column per joint. JSON: `captures.schema.json`. |
| `--trajectory` | The timed joint trajectory (CSV): `t`, the capture id on capture samples, then every joint's position, velocity and acceleration. |
| `--report` | The motion report (`motion_report.schema.json`): per step its duration, each joint's peak velocity, acceleration and jerk as fractions of its limits, the binding limit, the smallest distance from a joint limit, the smallest singular value of the tool Jacobian, how a `ptp_pose` found its configuration, and warnings (IK only from a restart seed, near a joint limit, near a singularity). |
| `--resolved` | The scenario with every `ptp_pose`'s joint configuration stored, so that it can never be compiled onto another IK branch. |

`--robot` chooses the robot when the scene has several.

## `gt`

Writes the analytic ground truth into `<dir>`: `dataset.json` (a calibration-rs `DatasetSpec`),
`robot_poses.json` and `gt.json`. The dataset type follows from the scene: one camera without a
robot is an intrinsics dataset, several cameras a rig, a robot makes it hand-eye (eye-in-hand if
the cameras ride on the robot, eye-to-hand if the board does), and a laser on the rig a laser-line
dataset. The image paths it names are where `render` writes. See [Datasets](datasets.md).

## `render`

Renders every capture with Blender and writes `images/<camera>/<capture>.png` into `<dir>`, and
`<capture>_laser.png` for a camera that images a laser (plus the canonical renders in `exr/` and
the render job, `job.json`). See
[Rendering](rendering.md).

| Option | Default | Meaning |
|---|---|---|
| `--samples` | 64 | Cycles samples per canonical pixel. |
| `--supersample` | 1 | Canonical pixels per camera pixel at the image centre. 4 is converged for corner accuracy. |
| `--sensor <file>` | — | A sensor model (JSON): raw mono PNGs with exposure, noise and quantisation. Without it, sRGB PNGs. |
| `--exposure` | 1 | Linear scale before sRGB encoding (without `--sensor`). |
| `--ambient` | 1 | Radiance of a uniform white environment; 0 lights the scene with its lights only. |
| `--laser-ambient` | 0.1 | Laser images (`<capture>_laser.png`): the fraction of the scene's light left on with the laser. |
| `--camera <id>` | all | Render only this camera (repeatable). |
| `--cpu` | GPU | Render on the CPU: bit-identical between runs. |
| `--seed` | 0 | Cycles seed. |
| `--blender <path>` | see [Installation](installation.md) | The Blender executable. |
| `--allow-blender-version` | off | Render with a Blender other than the pinned version (with a warning). |

Typical use, for corner-accurate linear images:

```bash
etendue gt     scene.json scenario.json -o out
etendue render scene.json scenario.json -o out --supersample 4 --samples 16 --sensor sensor.json
```

## `detect`

Detects the target in every image of a rendered dataset (`gt` and `render` into the same
directory) and writes `features.json`: per view, the detected pixel of each target point. The
target's kind comes from `gt.json`:

| Kind | Detected by | Labels |
|---|---|---|
| chessboard | chess-corners' Radon corners, calib-targets' chessboard detector | grid position; the whole board must be found, and the square colours settle its half-turn |
| ChArUco | calib-targets' ChArUco detector | ChArUco corner id |
| marker board | calib-targets' marker-board detector (the three circles align the grid) | grid position |
| PuzzleBoard | calib-targets' PuzzleBoard detector, searching the printed board only | corner id |
| PuzzlePole | calib-targets' PuzzlePole detector | corner id on the cylinder |

For the last four, calib-targets' own corner finder proposes the corners and each moves to the
Radon corner within 1 px, so every kind is located as accurately as the chessboard. Every label is
then checked against the ground truth — reported, never corrected — and a summary table is
printed. A plain rectangle has nothing to detect.

With laser images it also extracts each stripe's centre line into `laser_features.json`:
1. The board image is subtracted at its estimated dimming and divided out, so ink and paper do
   not matter.
2. A Gaussian is fitted across the stripe in every column (or row).
3. Each pixel's distance from the analytic stripe is reported.

ChArUco and marker-board ids are wrong when the board is turned more than 45° in the image (a
calib-targets 0.15 bug). Keep such views within ±45° of upright: the sphere pose generator keeps images upright, so keep its rolls within ±45°.

## `scenario from-poses`

Builds a stop-and-shoot scenario from tool poses: for each pose a point-to-point move, then a
capture. The poses file is a JSON array of `base_se3_tool` poses, either in the SE3 format
(`{"rotation": [qx, qy, qz, qw], "translation": [x, y, z]}`) or as `robot_poses.json` rows
(`tx, ty, tz, qx, qy, qz, qw`, with an optional `capture` id that names the capture).

| Option | Default | Meaning |
|---|---|---|
| `--robot <id>` | — | The robot in the scene that moves. |
| `--speed-scale` | 0.5 | Fraction of the joint velocity and acceleration limits, in (0, 1]. |
| `--dt` | 0.01 | Baking sample period, seconds. |
| `--scene <file>` | — | Check the scenario against this scene (reachability, limits). |

Feeding a dataset's `robot_poses.json` back in reproduces its viewpoints.

## `scenario sphere`

```bash
etendue scenario sphere <scene.json> --spec generator.json -o scenario.json [--candidates candidates.json] [--dt 0.01]
```

Calibration poses on spheres around a target, for an eye-in-hand sensor. The spec is a
scenario's `generator`:

```json
{"type": "sphere", "robot": "ur5e", "look": "rig", "target": "board",
 "radii_m": [0.45, 0.55],
 "theta": {"min_rad": 0.17, "max_rad": 0.6, "steps": 3},
 "phi": {"min_rad": 0, "max_rad": 6.283185307179586, "steps": 8},
 "roll_rad": [0, 0.3]}
```

The sensor — `look`, a rig, camera or frame the robot's tool carries (optionally offset by
`look_se3_sensor`) — is placed at every radius, polar angle `θ` from the sphere's pole (default the
board's normal) and azimuth `φ`, its +Z at the sphere's centre (default the target's centre) and its
image upright, then rolled by each `roll_rad`. A placement is kept when the robot reaches it with
every joint at least `joint_margin_rad` (0.05) from its limits and the tool Jacobian's smallest
singular value at least `min_sigma` (0.02), and every camera in `require.cameras` (default all)
sees at least `require.min_visible_fraction` (default 1) of the target, `require.margin_px` (2)
inside the image. The kept poses are ordered into a short tour and written as `ptp_pose` moves,
each with its joint configuration stored, and captures; `speed_scale` (0.5) and `return_home`
apply. `--candidates` writes every placement and why it was kept or not. The studio's Motion
workspace runs the same generator (**Generate poses**).
