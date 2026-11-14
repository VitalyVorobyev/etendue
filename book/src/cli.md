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
etendue calibrate <dir> [--noise <noise.json>] [--trials N] [--camera <id>] [--procedure <id | file>]
                  [--features <features.json>] [--laser-features <file>] [-o <report.json>] [--run <dir>]
etendue real calibrate <id | dataset.toml> -o <dir>
etendue real twin <run> [--trials N] [--seed S] [--exact] -o <dir>
etendue real split-half <run> [--random K] [--seed S] [--procedure <id | file>] -o <report.json>
etendue scenario from-poses <poses> --robot <id> -o <scenario.json> [--speed-scale S] [--dt T] [--scene <scene>]
etendue scenario sphere <scene> --spec <spec.json> -o <scenario.json> [--candidates <file>] [--dt T]
etendue scenario plan <scene> --spec <spec.json> --budget N -o <scenario.json> [--report plan.json] [--noise-px 0.5]
etendue project check <dir> [--bank <dir>]
etendue library list [--project <dir>] [--bank <dir>] [--builtin <dir>]
etendue library check <dir>
etendue library import <item.json | robot folder> --into <dir> [--force]
etendue library init <dir> [--name <name>]
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
etendue detect <dir> [--corners saddle_point|radon]
```

Detects the target in every image of a rendered dataset (`gt` and `render` into the same
directory) and writes `features.json`: per view, the detected pixel of each target point, checked
against the ground truth. The target's kind comes from `gt.json`:

| Kind | Detected by | Labels |
|---|---|---|
| chessboard | chess-corners' ChESS corners with the saddle-point refiner, and calib-targets' chessboard detector | grid position; the whole board must be found, and the square colours decide its half-turn |
| ChArUco | calib-targets' ChArUco detector | ChArUco corner id |
| marker board | calib-targets' marker-board detector (the three circles fix the grid) | grid position |
| PuzzleBoard | calib-targets' PuzzleBoard detector | corner id |
| PuzzlePole | calib-targets' PuzzlePole detector | corner id on the cylinder |

For the last four, calib-targets' own corner finder proposes the corners and each moves to
chess-corners' Radon corner within 1 px. The saddle-point refiner is not used for them: their
marker bits and edges near a corner disturb its fit.

`--corners` sets the corner detector for every kind: `saddle_point` (ChESS with the saddle-point
refiner) or `radon` (chess-corners' Radon detector, the chessboard's default before). On etendue's
closed-loop renders the saddle point is about five times more accurate on a chessboard (0.012
against 0.058 px RMS). These are synthetic images, whose clean corners suit the saddle-point fit:
the comparison does not carry over to real cameras unmeasured.

Every label
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
etendue calibrate <dir> [--noise noise.json] [--trials N] [--camera <id>] [--procedure <id | file>]
                  [--features features.json] [--laser-features laser_features.json]
                  [-o report.json] [--run <folder>]
```

Calibrates a dataset (the directory `gt` wrote) with calibration-rs, natively, and compares
every estimated quantity with the ground truth. Without `--procedure`, the dataset's shape picks
the workflow, and the dataset needs a robot (a hand-eye cell).

| Option | Meaning |
|---|---|
| `--noise <file>` | Observation noise, a JSON object (below). Without it, the observations are the ground truth's exact pixels. |
| `--trials N` | Repeat with seeds `seed, seed + 1, …` and print each row's mean, spread and maximum (default 1). |
| `--camera <id>` | Calibrate one camera of a rig alone, as a single-camera hand-eye problem. Not for laser datasets. |
| `--procedure <id \| file>` | Calibrate in stages: a built-in (`intrinsics`, `stereo`, `handeye`, `rig_handeye`, `laser_head`, `laser_rig`) or a procedure file. See [Procedures](calibration.md#procedures). The `intrinsics` and `stereo` built-ins need no robot. |
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

## `real calibrate`

```text
etendue real calibrate <id | dataset.toml | folder> -o <folder>
```

Calibrates a **real** dataset: images and robot poses described by a calibration-rs
`dataset.toml` manifest (its topology, cameras, image globs, target, robot poses and how they
pair). It runs calibration-rs's dataset runner, the same code as calibration-rs's own `vcal run`:
the target is detected in every image, the views are screened and paired with the robot poses, and
the manifest's workflow (`single_cam_handeye`, `rig_handeye`, `planar_intrinsics`,
`rig_extrinsics`, and the laser topologies `laserline_device`, `rig_laserline_device` and
`rig_handeye_laserline`) runs with its default configuration.

A laser topology's laser images are read by etendue's stripe detector, the one `detect` runs on
rendered laser images, in place of calibration-rs's own extractor (which is not on crates.io). A
real laser image has no board image beside it, so the stripe is taken as it is, one centre per
column (`scan_axis = "cols"`, the default) or per row (`"rows"`), and only the centres on long
continuous chains as thin as the stripe are kept: beyond the stripe's ends a column's brightest
pixel is the board's dim light. The manifest's `[laser]` `min_points` applies; its `sigma` and
thresholds belong to calibration-rs's extractor and are not read. A `rig_laserline_device`
manifest names its rig hand-eye calibration in `upstream_calibration`: a calibration-rs export of
the same version.

The dataset is named by:
- a manifest path, or a folder holding `dataset.toml`;
- an id listed in `local/datasets.toml` (in the folder `etendue` runs from; one line per dataset,
  `my_cell = "/path/to/dataset.toml"`, a relative path read from `local/`). Keep this file out of
  version control: it is how a private dataset is named without its path;
- the public ids `kuka_1` and `DS8`, found in a calibration-rs checkout beside etendue
  (`../calibration-rs/data/<id>/dataset.toml`) when `local/datasets.toml` does not name them.

The output folder is kept as a run, like `calibrate --run`: `config.json` (the manifest and id),
`report.json` and `result.json` (calibration-rs's export, identical to `vcal run`'s
`calibration.json`), with the detection cache in `.detections/`, so a second run into the same
folder detects nothing again. Nothing is written to the dataset's folder.

There is no truth, so there are no compared rows and no closed-loop check. The report has the
residuals of every feature and every view, calibration-rs's σ of each parameter and the noise the
residuals imply, the quality verdict with its checks, and the views the runner left out with the
reason. With lasers it also has each camera's laser plane (in the camera's frame) and its stripe
residuals: the views and stripe pixels, the pixels' RMS distance from the calibrated laser line in
the image, and their RMS distance from the plane in space. The command prints the views used, each
camera's intrinsics with σ, the reprojection error, the worst views, each laser's residuals and the
verdict.

## `real twin`

```text
etendue real twin <run> [--trials N] [--seed S] [--exact] -o <folder>
```

Builds the **twin** of a real calibration and calibrates it: the same cell, the same robot poses
and the same seen points, simulated. `<run>` is the folder `real calibrate` kept; the dataset's
images must still be where it found them, since its detection cache is keyed by them.

- **The truth** is the real calibration's result: the cameras, their poses on the rig and the
  hand-eye. No scene and no robot model are needed.
- **The views** are the real calibration's: each view's robot pose as the calibration took it, and
  in each view exactly the points the detector found, in its order, projected through the truth.
- **The noise** is what the real run implies: calibration-rs's reprojection noise σ on every point,
  and on the robot poses the pose error the real calibration weighed its pose corrections with.
- **The calibration** is the real run's: the workflow the manifest's topology picks, with its
  default configuration. Only hand-eye datasets (`single_cam_handeye`, `rig_handeye`) have a twin.

With `--trials N` the twin is observed and calibrated `N` times, with seeds `S, S + 1, …`; with
`--exact` it is calibrated once on noise-free observations, and the closed-loop check (G5.1) says
whether it recovers its truth. The output folder is kept as a run: `config.json` names the dataset
and the real run (`twin`), `report.json` holds every trial, `result.json` the first trial's
calibration-rs export, and `gt.json` the twin's ground truth.

It then sets the real run against its twin, per camera:
- the residuals' RMS, real over twin: about 1 when the noise model holds;
- their whiteness across the image (binned on an 8 × 6 grid) and within each view (a 4 × 3 grid
  per view): `Σ n |mean|² / σ²` per degree of freedom, about 1 for white residuals and several
  times 1 where the residuals have a shape a model does not explain;

and, with several trials, per parameter: the real σ, the twin's predicted σ and its scatter over
the trials, and how far the real estimate is from the twin's mean, in the twin's σ.

## `real split-half`

```text
etendue real split-half <run> [--random K] [--seed S] [--procedure <id | file>] -o <report.json>
```

Checks the pose planner's σ (`scenario plan`) on a real calibration's own poses. `<run>` is the
folder `real calibrate` kept, as for `real twin`. Three σ are set side by side:

- **The planner's**, predicted on the twin: the twin's views (the real robot poses and the points
  the detector found) and the noise learnt from the real run. That is its pixel noise σ; its
  view scatter as a per-view term (each view counts for `1 / k` of what white noise would give
  it, so every σ grows by `√k`, as calibration-rs scales its own covariance); and its robot pose
  error, as the prior the calibration weighs its pose corrections with, without the part of the
  corrections that is view-correlated pixel error (calibration-rs's estimate counts that part at
  its white size, so with view-correlated residuals it overstates the robot's error). The
  planner's σ with white views and calibration-rs's robot error, and with exact robot poses, are
  given beside it.
- **The real calibration's**: calibration-rs's covariance of a calibration of every real view,
  which it scales by its view scatter (how much more the views disagree than the residual noise
  predicts).
- **The split-half σ**: the real views are split into two halves, and each half is calibrated on
  its own. The difference of the two halves' estimates is compared with what the planner predicts
  for it, `√(σ_A² + σ_B²)` (about `√2 σ_half`, or twice the whole set's σ). Several splits are
  pooled: the even views against the odd, the first half against the second (in capture order),
  and `K` random halves (seeds `S, S + 1, …`; 8 by default). The pooled ratio of the measured to
  the predicted difference, times the planner's σ, is the split-half σ of the whole set.

Each is given for the **system σ** (how far, in millimetres across the line of sight, the
calibrated cell would place a point it knows in the robot's base frame, over every sighting of the
real views) and for each parameter, grouped. Every calibration runs the planner's model by
default: a rig's hand-eye with the rig refined (calibration-rs's default holds the rig, and its σ
is then conditional on it); a single camera's own workflow. `--procedure` runs another, a built-in
id or a procedure file, as `calibrate --procedure` takes it. The report (`-o`) holds every split,
the three system σ and every parameter's.

Gate P1 asks that the planner's system σ be within 25 % of the split-half σ. With white views
(the planner's model for a synthetic cell, and `scenario plan`'s), the planner's σ is a lower bound
wherever the residuals vary from view to view more than white noise would (a real board, a real
detector): the halves disagree by more than it predicts. Where the robot pose error dominates the
system σ, the planner is only as good as its prior. The learnt noise model addresses both, and on
`kuka_1` and `DS8` it predicts the split-half σ within 25 %. Its robot error rests on the view
scatter describing every direction alike (the assumption calibration-rs makes when it scales its
covariance by it), so treat it as an estimate: on `DS8` it is sensitive to which calibration's view
scatter it takes.

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

Calibration viewpoints on spheres around a target, for a sensor the robot carries
(eye-in-hand) or a fixed sensor that sees a target the robot carries (eye-to-hand). The spec is
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
| `robot` | — | The robot that carries the sensor, or the target. |
| `look` | — | The sensor: a rig, camera or frame, on the robot's tool or fixed. |
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
upright, then rolled by each roll angle. Eye-to-hand, the placements are relative to the target:
the robot puts the target where the fixed sensor sees it from there
(`examples/eye_to_hand_ur5e/sphere.json` is such a spec). A placement is kept when the robot reaches it within the
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

## `scenario plan`

```text
etendue scenario plan <scene> --spec <spec.json> --budget N -o <scenario.json> [--report plan.json] [--noise-px 0.5] [--dt 0.01]
```

Chooses, from the sphere generator's feasible poses, the `N` that calibrate the cell best. It
predicts, from the calibration's information, the **system σ**: how far across the line of sight
(mm, per image axis) the calibrated cell would place a point it knows in the robot's base frame,
averaged over the target's points as every feasible pose sees them. It adds one pose at a time,
the one that lowers the system σ most, and writes the chosen poses in the generator's tour order.

```text
$ etendue scenario plan examples/closed_loop_ur5e/scene.json \
      --spec examples/closed_loop_ur5e/scenario.json --budget 8 -o out/plan.json
8 of 24 feasible poses: system σ 0.0362 mm (the grid thinned to 8: 0.0462 mm; every feasible pose: 0.0243 mm) → out/plan.json
```

- **What it assumes.** The prediction is of a rig hand-eye calibration that refines the rig
  (`"free": {"refine_rig": true}`) and takes the robot's poses as exact, at `--noise-px` of pixel
  noise per axis. The per-parameter σ it predicts match calibration-rs's own covariance to about
  5 %, and calibrating noisy observations from the chosen poses gives the predicted system
  error ([measurement](https://github.com/VitalyVorobyev/etendue/blob/main/docs/measurements/g18_pose_planning.md)).
- **`--report`** writes the picks in order, the system σ after each (the curve to choose a budget
  from), the evenly thinned grid's σ for comparison (`grid_sigma_mm`, and `grid_sigma_curve_mm`
  for every smaller number), and every parameter's predicted σ. The picks for a budget are the
  first picks of any larger one.
- **The studio's Suggest poses** (Motion → Generate poses) runs the same planner.
- **Eye-to-hand** cells are planned too: the hand-eye is then `rig_se3_base` and the target's
  pose `gripper_se3_target`, and the same check holds (planned 8 poses: 0.0407 mm predicted,
  0.0428 mm measured, against the grid's 0.0485 mm).

## `project check`

```text
etendue project check <dir>
```

Checks a project folder: its manifest, every cell (the scene, and the scenario compiled in full),
every library item, the defaults and the project's robots. See
[Projects and the library](library.md#checking-a-project).

```text
$ etendue project check examples
project ok: etendue examples: 6 cell(s), library 3 cameras, 3 lasers, 3 lenses, 2 mounts, 2 rigs, 3 sensors, 2 targets, 0 robot(s)
```

With `--bank <dir>` it also notes each copy whose source in that bank has changed since it was
copied.

## `library`

```text
etendue library list [--project <dir>] [--bank <dir>] [--builtin <dir>]
etendue library check <dir>
etendue library import <item.json | robot folder> --into <dir> [--force]
etendue library init <dir> [--name <name>]
```

Works with a user bank (or any folder with `library/` and `robots/`). See
[The bank](library.md#the-bank).

```text
$ etendue library list --builtin examples
builtin  cameras/imx174_lm25hc                    IMX174 + Kowa LM25HC, f/4, focused at 0.5 m
…
14 item(s) and robot(s), 0 problem(s)
```
