# Command line

The `etendue` command runs each step of the pipeline on its own, for scripts and batch runs.
`etendue <command> --help` lists every option, and `etendue --version` prints the version. A
command that fails prints `error: …` and exits with status 1.

```text
etendue validate  <scene> [scenario]
etendue bake      <scene> <scenario> -o <baked.json> [--pretty] [--derivatives]
etendue export    <scene> <scenario> [--robot <id>] [--captures <file>] [--trajectory <file>]
                  [--report <file>] [--resolved <file>]
etendue gt        <scene> <scenario> -o <dir>
etendue render    <scene> <scenario> -o <dir> [options]
etendue detect    <dir>
etendue calibrate <dir> [--noise <noise.json>] [--trials N] [--camera <id>]
                  [--features <features.json>] [--laser-features <file>] [-o <report.json>] [--run <dir>]
etendue scenario from-poses <poses> --robot <id> -o <scenario.json> [--speed-scale S] [--dt T] [--scene <scene>]
etendue scenario sphere <scene> --spec <spec.json> -o <scenario.json> [--candidates <file>] [--dt T]
etendue project check <dir>
```

A typical run, from scene to calibration:

```bash
etendue validate  scene.json scenario.json
etendue gt        scene.json scenario.json -o out
etendue render    scene.json scenario.json -o out --supersample 4 --samples 16 --sensor sensor.json
etendue detect    out
etendue calibrate out --features out/features.json
```

The [command-line tutorial](tutorials/cli_pipeline.md) walks through it.

## `validate`

```text
etendue validate <scene> [scenario]
```

Checks a scene — its robot models, the frame tree, every item — and, if given, compiles the
scenario in full: joint limits, the reachability of every pose, and the timing. It prints a
summary or the problems:

```text
$ etendue validate examples/closed_loop_ur5e/scene.json examples/closed_loop_ur5e/scenario.json
scene ok: 1 robot(s), 1 rig(s), 2 camera(s), 0 laser(s), 0 light(s), 1 target(s), 0 part(s)
scenario ok: 48 step(s), 22.440 s, 2245 sample(s) at dt = 0.01 s, 24 capture(s)
```

## `bake`

```text
etendue bake <scene> <scenario> -o <baked.json> [--pretty] [--derivatives]
```

Compiles the scenario and writes the baked scenario: the pose `world_se3_frame` of every frame
at every sample, with the captures marked.

| Option | Meaning |
|---|---|
| `-o`, `--output` | The output file. |
| `--pretty` | Indent the JSON. |
| `--derivatives` | Also record the joint velocities and accelerations at every sample. |

## `export`

```text
etendue export <scene> <scenario> [--robot <id>] [--captures poses.csv|poses.json]
               [--trajectory trajectory.csv] [--report report.json] [--resolved scenario.json]
```

Exports a scenario's motion, for a robot controller or a spreadsheet, in SI units:

| Option | Writes |
|---|---|
| `--captures` | The tool pose in the robot's base frame and the joint positions at every capture. CSV columns: `capture, t, x, y, z`, the rotation as a quaternion `qx, qy, qz, qw` and as a rotation vector `rx, ry, rz` (axis × angle, as Universal Robots controllers take it), then one column per joint. A `.json` file is a [captures document](formats.md). |
| `--trajectory` | The timed joint trajectory (CSV): `t`, the capture id on capture samples, then every joint's position, velocity and acceleration. |
| `--report` | The motion report (JSON): per step, its duration, each joint's peak velocity, acceleration and jerk as fractions of its limits, the limit that sets the speed, the smallest distance from a joint limit, the smallest singular value of the tool Jacobian, how a `ptp_pose` found its joint configuration, and warnings: the pose was found only from a restart, a joint comes within 2° of a limit, the move passes near a singularity (singular value below 0.02), the joints jump more than 0.05 rad between straight-line waypoints. |
| `--resolved` | The scenario with every `ptp_pose`'s joint configuration stored, so it can never be compiled onto another configuration. |
| `--robot` | The robot to export, when the scene has several. |

```text
$ etendue export examples/eye_in_hand_ur5e/scene.json examples/eye_in_hand_ur5e/scenario.json \
      --captures out/captures.csv --report out/report.json
10 capture(s) of `ur5e` → out/captures.csv
motion report (18.61 s, 3 warning(s)) → out/report.json
```

## `gt`

```text
etendue gt <scene> <scenario> -o <dir>
```

Writes the analytic ground truth into `<dir>`: `gt.json`, `dataset.json` (the calibration-rs
dataset manifest) and, for a robot cell, `robot_poses.json`. The kind of calibration problem
follows from the scene; the image paths `dataset.json` names are where `render` writes. See
[Datasets](datasets.md).

```text
$ etendue gt examples/closed_loop_ur5e/scene.json examples/closed_loop_ur5e/scenario.json -o out
ground truth: 24 capture(s), 48 view(s), 2592 visible point(s) of 54 → out
```

## `render`

```text
etendue render <scene> <scenario> -o <dir> [options]
```

Renders every capture with Blender and writes `images/<camera>/<capture>.png` into `<dir>`, plus
`<capture>_laser.png` for a camera that images a laser. Blender's own renders go to `exr/`, and
the render job to `job.json`. See [Rendering](rendering.md).

| Option | Default | Meaning |
|---|---|---|
| `-o`, `--output` | — | The output directory: use the one `gt` wrote. |
| `--samples` | 64 | Render samples per pixel of Blender's render. |
| `--supersample` | 1 | Blender's render pixels per camera pixel, along each axis, at the image centre. 4 gives converged corner positions. |
| `--sensor <file>` | — | A [sensor model](rendering.md#sensor-model): raw mono PNGs with exposure, noise and quantisation. Without it, sRGB PNGs. |
| `--exposure` | 1 | Linear scale before sRGB encoding (without `--sensor`). |
| `--ambient` | 1 | Radiance of a uniform white environment; 0 lights the scene with its own lights only. |
| `--laser-ambient` | 0.1 | In laser images, the fraction of the scene's light left on with the laser. |
| `--camera <id>` | all | Render only this camera (repeatable). |
| `--cpu` | GPU | Render on the CPU: bit-identical images between runs. |
| `--seed` | 0 | Blender's render seed. |
| `--blender <path>` | see [Installation](installation.md#blender) | The Blender executable. |
| `--allow-blender-version` | off | Render with a Blender other than the pinned version, with a warning. |
| `--backend` | `blender` | The render backend; Blender is the only one. |

For corner-accurate linear images:

```bash
etendue gt     scene.json scenario.json -o out
etendue render scene.json scenario.json -o out --supersample 4 --samples 16 --sensor sensor.json
```

## `detect`

```text
etendue detect <dir>
```

Detects the target in every image of a rendered dataset (`gt` and `render` into the same
directory) and writes `features.json`: per view, the detected pixel of each target point, checked
against the ground truth. The target's kind comes from `gt.json`:

| Kind | Detected by | Labels |
|---|---|---|
| chessboard | chess-corners' Radon corners and calib-targets' chessboard detector | grid position; the whole board must be found, and the square colours decide its half-turn |
| ChArUco | calib-targets' ChArUco detector | ChArUco corner id |
| marker board | calib-targets' marker-board detector (the three circles fix the grid) | grid position |
| PuzzleBoard | calib-targets' PuzzleBoard detector | corner id |
| PuzzlePole | calib-targets' PuzzlePole detector | corner id on the cylinder |

For the last four, calib-targets' own corner finder proposes the corners and each moves to the
Radon corner within 1 px, so every kind is located as accurately as the chessboard. Every label
is then checked against the ground truth — reported, never corrected — and a table per camera is
printed: views kept, views with no board, a partial board, an ambiguous or undecoded board,
points, mislabelled and unmatched points, and the RMS and largest distance from the true pixels.
A plain rectangle has nothing to detect.

**Laser stripes.** With laser images, `detect` also extracts each stripe's centre line into
`laser_features.json`:
1. The board image is subtracted at its estimated dimming and divided out, so ink and paper do
   not bias the stripe.
2. A Gaussian is fitted across the stripe in every column (or row), giving its centre and its
   width.
3. Each pixel's distance from the true stripe is reported (farther than 3 px is an outlier), and
   its width against the laser beam's width carried into the image. The table's `width/beam`
   column is the median ratio, then the extremes in brackets. A ratio a little above 1 is the
   pixel's own blur.

The [Datasets](datasets.md) page shows an example.

## `calibrate`

```text
etendue calibrate <dir> [--noise noise.json] [--trials N] [--camera <id>]
                  [--features features.json] [--laser-features laser_features.json]
                  [-o report.json] [--run <folder>]
```

Calibrates a dataset (the directory `gt` wrote) with calibration-rs, natively, and compares
every estimated quantity with the ground truth. The dataset needs a robot (a hand-eye cell).

| Option | Meaning |
|---|---|
| `--noise <file>` | Observation noise, a JSON object (below). Without it, the observations are the ground truth's exact pixels. |
| `--trials N` | Repeat with seeds `seed, seed + 1, …` and print each row's mean, spread and maximum (default 1). |
| `--camera <id>` | Calibrate one camera of a rig alone, as a single-camera hand-eye problem. Not for laser datasets. |
| `--features <file>` | Calibrate from `etendue detect`'s detections instead of the ground truth. Cannot be combined with `--noise` or `--trials`. |
| `--laser-features <file>` | The detected laser stripes (default: `laser_features.json` next to `--features`). |
| `-o`, `--output` | Write the report (JSON). |
| `--run <folder>` | Keep the run in this folder: `config.json`, `report.json` and `result.json` (calibration-rs's own export), as the studio does. |

The noise file (every field optional, default 0):

```json
{"pixel_sigma_px": 0.5, "outlier_fraction": 0.02, "outlier_sigma_px": 5,
 "dropout_fraction": 0.05, "laser_sigma_px": 0.3,
 "robot_rot_sigma_rad": 0.0001, "robot_trans_sigma_m": 0.0001, "seed": 1}
```

On exact observations the command fails (exit status 1) when any checked quantity is outside its
tolerance. With noise or detections the tolerances are printed for reference only.
[Calibration](calibration.md) explains the problems, the noise, the rows and the tolerances, with
a full example.

## `scenario from-poses`

```text
etendue scenario from-poses <poses> --robot <id> -o <scenario.json> [--speed-scale S] [--dt T] [--scene <scene>]
```

Builds a stop-and-shoot scenario from tool poses: for each pose a point-to-point move, then a
capture. The poses file is a JSON array of `base_se3_tool` poses, either in the pose format
(`{"rotation": [qx, qy, qz, qw], "translation": [x, y, z]}`) or as `robot_poses.json` rows
(`tx, ty, tz, qx, qy, qz, qw`, with an optional `capture` id that names the capture).

| Option | Default | Meaning |
|---|---|---|
| `--robot <id>` | — | The robot in the scene that moves. |
| `-o`, `--output` | — | The scenario file to write. |
| `--speed-scale` | 0.5 | Fraction of the joint limits the moves use, in (0, 1]. |
| `--dt` | 0.01 | Baking sample period, seconds. |
| `--scene <file>` | — | Check the scenario against this scene: every pose reachable, every move within the limits. |

Feeding a dataset's `robot_poses.json` back in reproduces its viewpoints. The studio's **File ›
Import tool poses…** does the same.

## `scenario sphere`

```text
etendue scenario sphere <scene> --spec <spec.json> -o <scenario.json> [--candidates candidates.json] [--dt 0.01]
```

Calibration viewpoints on spheres around a target, for a sensor the robot carries. The spec is
the JSON a scenario keeps in its `generator`, so `--spec` also takes a generated scenario, to
generate it again:

```json
{"type": "sphere", "robot": "ur5e", "look": "rig", "target": "board",
 "radii_m": [0.45, 0.55],
 "theta": {"min_rad": 0.17, "max_rad": 0.6, "steps": 3},
 "phi": {"min_rad": 0, "max_rad": 6.283185307179586, "steps": 8},
 "roll_rad": [0, 0.3]}
```

| Field | Default | Meaning |
|---|---|---|
| `robot` | — | The robot that carries the sensor. |
| `look` | — | The sensor: a rig, camera or frame the robot's tool carries. |
| `look_se3_sensor` | identity | The sensor's axis relative to `look`. |
| `target` | — | The target looked at. |
| `centre_m` | the target's centre | The spheres' centre, in the target's frame. |
| `pole` | +Z (a PuzzlePole: +X) | The direction θ = 0, in the target's frame. |
| `radii_m` | — | The sphere radii. |
| `theta`, `phi` | — | The polar angle from the pole and the azimuth: `steps` angles from `min_rad` to `max_rad`, both included (a full turn counts its start once). |
| `roll_rad` | `[0]` | Rolls of the sensor about its axis. |
| `joint_margin_rad` | 0.05 | Smallest distance of any joint from its limits. |
| `min_sigma` | 0.02 | Smallest singular value of the tool Jacobian (distance from a singularity). |
| `require` | all cameras, 100 %, 2 px | `cameras` that must see the target, the share `min_visible_fraction` of its points each must see, at least `margin_px` inside the image. |
| `speed_scale` | 0.5 | Fraction of the joint limits the moves use. |
| `return_home` | false | Go back to the start configuration at the end. |

The sensor is placed at every radius, polar angle and azimuth, its +Z at the centre and its image
upright, then rolled by each roll angle. A placement is kept when the robot reaches it within the
joint margin and away from singularities, and every required camera sees enough of the target.
The kept poses are ordered into a short tour and written as `ptp_pose` moves, each with its joint
configuration stored, and captures. `--candidates` writes every placement and why it was kept or
not.

```text
$ etendue scenario sphere examples/closed_loop_ur5e/scene.json \
      --spec examples/closed_loop_ur5e/scenario.json -o out/scenario.json
24 of 48 candidate poses kept, 22.44 s of motion → out/scenario.json
```

The studio's **Generate poses** panel (Motion workspace) runs the same generator. How it works is
in [Pose generation](models/poses.md).

## `project check`

```text
etendue project check <dir>
```

Checks a project folder: its manifest, every cell (the scene, and the scenario compiled in full),
every library item, the defaults and the project's robots. See
[Projects and the library](library.md#checking-a-project).

```text
$ etendue project check examples
project ok: etendue examples: 6 cell(s), library 2 cameras, 3 lasers, 3 lenses, 1 rigs, 3 sensors, 2 targets, 0 robot(s)
```
