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
sample. `--pretty` indents the JSON.

## `gt`

Writes the analytic ground truth into `<dir>`: `dataset.json` (a calibration-rs `DatasetSpec`),
`robot_poses.json` and `gt.json`. The dataset type follows from the scene: one camera without a
robot is an intrinsics dataset, several cameras a rig, a robot makes it hand-eye (eye-in-hand if
the cameras ride on the robot, eye-to-hand if the board does), and a laser on the rig a laser-line
dataset. The image paths it names are where `render` writes. See [Datasets](datasets.md).

## `render`

Renders every capture with Blender and writes `images/<camera>/<capture>.png` into `<dir>` (plus
the canonical renders in `exr/` and the render job, `job.json`). See
[Rendering](rendering.md).

| Option | Default | Meaning |
|---|---|---|
| `--samples` | 64 | Cycles samples per canonical pixel. |
| `--supersample` | 1 | Canonical pixels per camera pixel at the image centre. 4 is converged for corner accuracy. |
| `--sensor <file>` | — | A sensor model (JSON): raw mono PNGs with exposure, noise and quantisation. Without it, sRGB PNGs. |
| `--exposure` | 1 | Linear scale before sRGB encoding (without `--sensor`). |
| `--ambient` | 1 | Radiance of a uniform white environment; 0 lights the scene with its lights only. |
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

Detects the chessboard in every image of a rendered dataset (`gt` and `render` into the same
directory) and writes `features.json`: per view, the detected pixel of each board point. Corners
come from chess-corners' Radon detector and are labelled by calib-targets' chessboard detector; a
view is kept only when the whole board is found, and the square colours settle the board's
orientation. Every label is then checked against the ground truth — reported, never corrected — and
a summary table is printed. Chessboards only, for now.

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
