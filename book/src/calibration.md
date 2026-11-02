# Calibration

etendue calibrates its own datasets with
[calibration-rs](https://github.com/VitalyVorobyev/calibration-rs) and compares every estimated
quantity with the ground truth. Because the truth is known exactly, the comparison answers
questions a real calibration cannot: is the calibration correct, how much does pixel noise or a
robot's pose error cost, and how close do rendered and detected images come to the ideal?

Two tools do it:
- **`etendue calibrate <dataset>`** on the command line ([Command line](cli.md#calibrate));
- the studio's **Calibrate** workspace, which generates the dataset of the open cell, calibrates
  it the same way, and draws the estimate beside the truth
  ([The studio](studio.md#calibrate)).

The theory — what a closed loop proves and why — is in
[Ground truth and closed-loop calibration](models/closed_loop.md).

## The problems

The dataset decides the calibration problem. It must have a robot: `etendue calibrate`
calibrates hand-eye cells, eye-in-hand (the cameras on the robot) or eye-to-hand (the target on
the robot). A dataset without a robot is refused with "the dataset has no robot"; calibrate its
`dataset.json` with calibration-rs directly.

| Dataset | Problem (as the report names it) | Estimated |
|---|---|---|
| One camera | single-camera hand-eye (`SingleCamHandeye`) | intrinsics, distortion, hand-eye, target pose |
| A rig of cameras | rig hand-eye (`RigHandeye`) | each camera's intrinsics and distortion, each camera's pose on the rig, hand-eye, target pose |
| A rig with lasers (one shared, or one per camera) | rig hand-eye laser-line (`RigHandeyeLaserline`) | as rig hand-eye, plus each camera's laser plane and a Scheimpflug sensor's tilt |
| One camera carrying its laser (a triangulation head) | laser-line device (`LaserlineDevice`) | intrinsics, distortion, tilt and laser plane; then the hand-eye from the single-camera problem on the same views, with that camera held |

- **One camera of a rig.** `--camera <id>` (or **Cameras › _id_ alone** in the studio)
  calibrates one camera of a rig without lasers as a single-camera problem.
- **Scheimpflug rigs** are started from the values you would read off datasheets: the focal
  length rounded to 100 px, the principal point at the image centre, and the tilt rounded to a
  whole degree.

This choice by the dataset's shape is a procedure too, `auto`, and it is what runs without
`--procedure`. The report names it (`auto_single`, `auto_rig`, `auto_head`, `auto_rig_laser`,
`auto_rig_laser_scheimpflug`).

## Procedures

A **procedure** says how to calibrate, in stages. Each stage is one calibration-rs workflow with:
- the camera model and the parameters it holds **fixed**, in calibration-rs's names (`fx`, `cx`,
  `k3`, `tilt_y`);
- what else is **free**: intrinsics refined with the rig, the robot poses refined under a prior
  (and its σ), the hand-eye or the target held, a laser plane held;
- how it solves: the iteration limit and the robust loss;
- what it **starts from**: an earlier stage's result, the design (datasheet values), or the
  truth (synthetic data only).

The last stage's result is the procedure's. Its laser planes come from the latest stage that
estimated them, and the report lists every stage (`stages`).

```bash
etendue calibrate out --procedure rig_handeye        # a built-in
etendue calibrate out --procedure my_procedure.json  # a library item of kind procedure
```

| Built-in | Stages |
|---|---|
| `intrinsics` | each camera's intrinsics from its planar views (no robot needed) |
| `stereo` | each camera's intrinsics, then the rig's extrinsics seeded from them (no robot needed) |
| `handeye` | the camera's intrinsics, then its hand-eye with the robot poses refined; on a rig, each camera on its own |
| `rig_handeye` | intrinsics per camera, the rig's extrinsics, then the rig's hand-eye, each seeded from the last |
| `laser_head` | the device workflow (intrinsics, tilt, plane; datasheet tilt), then the hand-eye with that camera held |
| `laser_rig` | one joint solve of cameras, rig, planes and hand-eye, seeded from datasheet values |

The built-ins are also files, `examples/library/procedures/<id>.json`: copy one into your bank
or project and edit it, in the studio's procedure editor ([The studio](studio.md#calibrate-and-compare))
or by hand. A procedure file is a library item:

```json
{ "version": 2, "id": "my_handeye", "name": "Hand-eye, principal point held",
  "procedure": { "stages": [
    { "id": "intrinsics", "problem": "planar_intrinsics",
      "model": { "fix": { "intrinsics": ["cx", "cy"] } } },
    { "id": "handeye", "problem": "single_cam_handeye",
      "model": { "fix": { "intrinsics": ["cx", "cy"] } },
      "seed": { "intrinsics": { "stage": "intrinsics" } },
      "free": { "refine_robot_poses": true, "robot_rot_sigma_rad": 0.005 } } ] } }
```

| Stage field | Values |
|---|---|
| `problem` | `planar_intrinsics`, `rig_extrinsics`, `single_cam_handeye`, `rig_handeye`, `laserline_device`, `rig_handeye_laserline` |
| `camera` | for a single-camera workflow, the camera; absent, each camera in turn |
| `model.sensor` | `design` (Scheimpflug when the camera tilts), `identity`, `scheimpflug` |
| `model.fix` | `intrinsics`, `distortion`, `sensor`: lists of parameter names held fixed |
| `free` | `refine_intrinsics`, `refine_sensor`, `refine_rig`, `refine_robot_poses`, `robot_rot_sigma_rad`, `robot_trans_sigma_m`, `min_motion_angle_deg`, `fix_handeye`, `fix_target`, `fix_plane`, `fix_poses` |
| `solver` | `max_iters`, `robust_loss` (`{"kind": "huber", "scale": 1.0}`, `cauchy`, `arctan`, `none`) |
| `seed` | `intrinsics`, `sensor`, `handeye`: `"nominal"`, `"truth"` or `{"stage": "<id>"}`; `rig`: `{"stage": "<id>"}` |

A procedure is checked against the dataset before anything runs: every workflow must fit
(a robot for a hand-eye, lasers for a laser workflow, one camera for `laserline_device`), every
seed must name an earlier stage, and an option must apply to its stage's workflow.

### Scheimpflug cameras and laser heads

**A calibration hardly sees a sensor tilt, and measurement does not need it.** Seen through a
planar target, a tilted sensor projects almost exactly like an untilted one with a shifted
principal point, in a slightly rotated camera:
- **The principal point** moves by f·τ. A tilt of 0.57° (0.01 rad) about y moves it 23 px on a
  2300 px lens.
- **The camera frame** turns by the tilt.

Only the lens distortion tells the two apart. It acts about the optical axis, which the shift moves
away from the principal point. The tangential terms p1 and p2 describe exactly such an off-centre
distortion, so they can absorb the difference too:

| What the procedure estimates | What happens to the tilt |
|---|---|
| p1, p2 and the tilt | The tilt stays near its starting value, the datasheet's. The principal point, p1, p2 and the camera frame make up the difference. More or different views do not change this. |
| The tilt, with p1 and p2 held at zero | The tilt is measured through the radial distortion, assuming a centred lens. On the example head, at 0.1 px of noise, it comes out within about 0.1° (k1 = −0.08). Stronger distortion measures it better; with none, it is not measured at all. |

The model is `{"sensor": "scheimpflug", "fix": {"distortion": ["k3", "p1", "p2"], "sensor": []}}`.
`laser_rig` already holds p1 and p2 at zero.

Either way the calibrated camera projects correctly. Every pixel's ray, the laser plane in the
robot's frame and the target's pose come out right, whichever split the solve found. What depends
on the split:
- **Quantities that read the tilt itself:** the plane of best focus, or a check against the
  head's drawing.
- **Quantities in the camera's frame.** The report's tilt, principal point, laser-normal and
  hand-eye rows can therefore fail on a calibration that measures correctly.

**The laser plane and the hand-eye share one camera.** The plane is estimated in the camera's
frame, so it only composes with a hand-eye found for the same camera. `laser_head` and `auto`
on a head estimate the camera, its tilt and the plane in the device stage, then the hand-eye with
that camera held. Earlier, the hand-eye stage estimated the camera again (in etendue 0.2, `auto`
with an untilted model), so the result combined two camera solutions. Under 0.1 px of noise a
stripe point then triangulated 0.1–13 mm off on the example head, against about 0.013 mm now. A procedure of your own that estimates the plane in one
stage and the hand-eye in a later one must hold the camera in the later one, as `laser_head` does
(`examples/library/procedures/laser_head.json`). The stripe triangulation row catches one that
does not.

## Observations

A calibration is given **observations**: the target points' pixels in every view, the
laser-stripe pixels, and the robot's reported tool poses. There are three sources.

| Source | Command line | Studio |
|---|---|---|
| The ground truth's exact pixels and poses | (default) | **Exact** |
| The same, with seeded noise | `--noise noise.json` | **With noise** |
| The detector's corners and stripes, in rendered images | `--features features.json` | **Detected** (after a render) |

### The noise model

| Field | Unit | Studio field | Meaning |
|---|---|---|---|
| `pixel_sigma_px` | px | Pixel σ | Gaussian noise on every target point, per axis. |
| `outlier_fraction` | 0–1 | Outliers (%) | The share of seen target points that are outliers. |
| `outlier_sigma_px` | px | Outlier σ | An outlier's displacement, per axis (a misdetected or mislabelled corner). |
| `dropout_fraction` | 0–1 | Dropouts (%) | The share of seen target points the detector misses. |
| `laser_sigma_px` | px | Laser σ | Gaussian noise on every laser-stripe pixel, per axis. |
| `robot_rot_sigma_rad` | rad | Robot rotation σ (°) | The robot's reported tool rotation error, per rotation-vector component. |
| `robot_trans_sigma_m` | m | Robot translation σ (mm) | The robot's reported tool position error, per axis. |
| `seed` | — | Seed | The random stream's seed. |

Every field is optional and defaults to 0. Each seen target point is, in turn, dropped (with
probability `dropout_fraction`), made an outlier (with probability `outlier_fraction`), or
given pixel noise. A view keeps its target points only if at least four remain, and its stripe
only if at least two pixels remain.

The noise is drawn from one seeded stream, the same on every platform, so a seed always gives
the same observations and the same report. In the studio, the camera view of the Calibrate
workspace shows the very points a seed calibrates from.

### Trials

`--trials N` (or **Trials** in the studio) calibrates `N` times with seeds `seed`, `seed + 1`, …,
and reports each quantity's mean, standard deviation and maximum over the trials. One seed shows
one outcome; a few trials show how much it varies. Trials apply to noise only, not to detections.

## Reading the report

`etendue calibrate` prints the problem, the views and points used, one row per compared
quantity, and the reprojection error. On the `closed_loop_ur5e` example with exact observations:

```text
$ etendue gt examples/closed_loop_ur5e/scene.json examples/closed_loop_ur5e/scenario.json -o out
$ etendue calibrate out
out: rig hand-eye (2 camera(s): cam_left, cam_right; 24 views, 2592 points, 0 laser pixels)
  mean reprojection px             7.676e-7  ≤ 0.05     PASS
  cam_left fx rel                 1.489e-10  ≤ 0.0001   PASS
  cam_left fy rel                 7.540e-11  ≤ 0.0001   PASS
  cam_left cx px                   5.505e-6  ≤ 0.05     PASS
  cam_left cy px                  5.230e-12  ≤ 0.05     PASS
  cam_left k1 abs                  1.528e-9
  cam_left k2 abs                  2.165e-8
  cam_left k3 abs                   0.000e0
  cam_left p1 abs                 3.010e-16
  cam_left p2 abs                  6.027e-10
  cam_right fx rel                 1.963e-8  ≤ 0.0001   PASS
  …
  cam_right cam_se3_rig deg        1.746e-5
  cam_right cam_se3_rig mm         1.698e-5
  hand-eye rotation deg            1.528e-7  ≤ 0.01     PASS
  hand-eye translation mm          1.330e-6  ≤ 0.05     PASS
  base_se3_target rot deg          1.929e-8
  base_se3_target trans mm         1.094e-6
  closed-loop check (target): PASS
  reprojection: mean 0.0000 px, rms per axis 0.0000 px over 2592 points
```

Every row is an **error**: the estimate's distance from the truth.

| Row | Meaning |
|---|---|
| `mean reprojection px` | The mean distance, in pixels, between the observed points and the points the estimate projects. |
| `<camera> fx rel`, `fy rel` | The focal length's relative error, \|estimate − truth\| / truth. |
| `<camera> cx px`, `cy px` | The principal point's error, in pixels. |
| `<camera> k1 abs` … `p2 abs` | Each Brown–Conrady distortion coefficient's absolute error. |
| `<camera> tilt_x deg`, `tilt_y deg` | A Scheimpflug sensor's tilt error, in degrees. |
| `<camera> cam_se3_rig deg`, `mm` | The camera's pose on the rig, relative to the first camera: the rotation error in degrees and the translation error in millimetres. |
| `<camera> laser normal deg`, `laser distance mm` | The laser plane, in that camera's frame: the angle between the estimated and true normals, and the offset error. |
| `hand-eye rotation deg`, `translation mm` | The hand-eye transform: the camera rig on the robot's flange (eye-in-hand), or the rig in the robot's base (eye-to-hand). |
| `base_se3_target …`, `gripper_se3_target …` | The target's pose: in the robot's base (eye-in-hand), or on the flange (eye-to-hand). |

The last line gives the reprojection error again: its mean, its RMS per axis, and the number of
points. The studio shows the same rows in a table, and every point's error as a histogram.

### The tolerances

Some rows carry a tolerance:

| Quantity | Tolerance |
|---|---|
| Mean reprojection error | 0.05 px |
| Focal lengths `fx`, `fy` | 1e-4 relative (0.01 %) |
| Principal point `cx`, `cy` | 0.05 px |
| Hand-eye rotation | 0.01° |
| Hand-eye translation | 0.05 mm |
| Laser plane normal | 0.01° |
| Laser plane offset | 0.05 mm |
| Stripe triangulation, largest error | 0.05 mm |

**The stripe triangulation row** checks the calibration as a measuring device. It takes every
true stripe pixel, back-projects it with the calibrated camera, meets the ray with the calibrated
laser plane, and carries the point into the robot's base through the calibrated hand-eye and the
robot pose. It reports the distance from the true point, rms and largest, for each camera with a
laser. Unlike the other rows it does not depend on how the solve split the projection between
tilt, principal point and camera frame, and it judges the result as a whole: a camera, a plane
and a hand-eye that do not belong together fail it even when each row passes.

**On exact observations, the tolerances are a pass/fail check.** A correct calibration of exact
data recovers the truth almost perfectly, well inside these numbers (in the example above, at
least 70 times inside, and most rows by many orders of magnitude). A row outside its tolerance means something is wrong:
a frame or pose convention, a model mismatch, or a problem the solver cannot resolve. Each such
row prints `FAIL`, the verdict line `FAIL`, and the command exits with status 1.

**With noise or detections, the tolerances are shown for reference only.** Noise moves every
estimate, so rows outside a tolerance print `outside` and the command still succeeds. The
tolerances are not accuracy specifications for a real cell: they are a yardstick for how far the
noise moved the result.

The verdict line names what was checked: `closed-loop check (target)` for cameras alone, and
`closed-loop check (target and laser plane)` for a dataset with lasers. The studio's badge reads
**Closed-loop check: PASS** or **FAIL**. In the report file the check keeps its development id
(`"gate": "G5.1"` or `"G5.3"`; see the [Glossary](glossary.md#gate)).

**Scheimpflug laser rigs.** These rigs are seeded from datasheet values (focal length, a
whole-degree tilt). The `laser_scheimpflug_ur5e` example passes on exact data, every row within
1e-6. calibration-rs holds p1 and p2 at zero there, so the radial distortion reveals the tilt (see
[Scheimpflug cameras and laser heads](#scheimpflug-cameras-and-laser-heads)). Under noise the tilt
stays the weak parameter: at 0.1 px it comes out within about 0.1°, and the principal points within
4–7 px.

### With noise

The same dataset with `pixel_sigma_px` 0.5 and seed 1, over 8 trials (excerpt):

```text
$ echo '{"pixel_sigma_px": 0.5, "seed": 1}' > noise.json
$ etendue calibrate out --noise noise.json --trials 8
…
  closed-loop check (target) not applied (noisy or detected observations); limits not all within
  reprojection: mean 0.6066 px, rms per axis 0.4862 px over 2592 points
8 trials:
  mean reprojection px         mean   6.140e-1  std   8.454e-3  max   6.292e-1
  cam_left fx rel              mean   2.474e-3  std   1.408e-3  max   5.041e-3
  cam_left cx px               mean    5.776e0  std    3.122e0  max    1.080e1
  …
  hand-eye rotation deg        mean   2.082e-1  std   9.462e-2  max   3.707e-1
  hand-eye translation mm      mean    1.362e0  std   7.162e-1  max    2.771e0
```

Two things to read here:
- **The residuals match the noise.** With Gaussian pixel noise σ, the RMS per axis approaches σ
  (here 0.486 px for σ = 0.5 px; slightly less, because the solver absorbs some of the noise in
  its free parameters), and the mean 2-D distance approaches σ√(π/2) ≈ 1.25σ (here 0.61 px). A
  residual far from this means the noise is not what you think, or the model does not fit.
- **What the noise costs.** Half a pixel of corner noise on this cell costs about 0.25 % in focal
  length, several pixels in the principal point, 0.2° and 1.4 mm in hand-eye. Rerun with more
  views, a larger target or a wider spread of viewpoints and compare.

### From detections

After a render, `--features` calibrates from what the detector found. On the rendered
`laser_ur5e` example (supersampling 4, 16 samples, the linear sensor), the detected corners are
0.058 px RMS from the truth and the stripes 0.08 px. The calibration's reprojection error is
then 0.052 px; the laser planes are within 0.01° and 0.02 mm, and the hand-eye is 0.014° and
0.15 mm from the truth. As with noise, the tolerances are shown for reference. This is the
whole image chain — rendering, sensor, detection — at work.

## The JSON report

`-o report.json` writes the report:

- `reports`: one per trial, each with the procedure (`procedure`) and its `stages` (each stage's
  workflow, cameras, views, points and mean reprojection error), the problem (`workflow`), the check's id (`gate`), whether
  it applied (`gated`) and passed (`pass`), the `cameras`, `views`, `points` and `laser_pixels`
  used, the `noise`, the `rows` (`label`, `value`, `limit` when there is one), the `residuals`
  (`count`, `mean_px`, `rms_axis_px`, `max_px`, `per_camera_mean_px` and every point's error
  `errors_px`), and the `estimate`;
- `row_stats`: with several trials, each row's `mean`, `std` and `max`;
- `param_stats`: with several trials, each estimated parameter (`cam_left fx`, `hand-eye ty`, …)
  with its `truth`, `mean`, `std`, `bias`, `rmse` and calibration-rs's `predicted_sigma`.

Each report also holds:
- `residuals.features`: every target point's residual, with its `capture`, `camera` and `point`
  (its index in the target) and the `observed_px` and `projected_px` pixels. In a run of several
  trials, only the first trial keeps them.
- `uncertainty`: calibration-rs's predicted σ of every estimated parameter (`labels`, `sigma`,
  in calibration-rs's units: px, radians, metres; `camN` is the report's `cameras[N]`), the
  parameters held `fixed`, and the reprojection noise the residuals imply (`noise_px`).
- `quality`: calibration-rs's quality verdict on the result (`verdict`: pass, warn or fail; and
  its `checks`).

**Predicted against observed.** The σ are calibration-rs's covariance, which is *conditional on
the parameters held fixed*. The rig workflow holds the cameras' poses in the rig fixed while it
solves the hand-eye (`refine_rig` is off by default). Its σ for the intrinsics and the hand-eye
are then smaller than the spread you see over trials: 1.2–2.6 times smaller on
`closed_loop_ur5e` at 0.5 px of noise. With `"free": {"refine_rig": true}` (a procedure), the
covariance holds the rig too, and the predicted σ match the spread. The hand-eye's σ are in
calibration-rs's tangent space, `estimate = truth · Exp(δ)`, and `param_stats` reports the
hand-eye error that way: the rotation `log(R_trueᵀ R)` and the translation `R_trueᵀ (t − t_true)`,
both in the hand-eye's own frame.

The estimate's `offsets` place it beside the truth: each estimated camera in the true camera's
frame, the estimated target in the true target's frame, and each laser plane, true and
estimated, in the true camera's frame. The studio draws them this way.

## Runs

`--run <folder>` keeps a calibration as a **run**, and the studio keeps every calibration this
way:

| File | Holds |
|---|---|
| `config.json` | The dataset and the options: the noise, the trials, the camera, whether the observations were detected, and the procedure (by value) when one was named. |
| `report.json` | The report, as above. |
| `result.json` | calibration-rs's own export of the first trial, to use downstream. |

The studio names a run after the dataset, the time and the observations, for example
`closed_loop_ur5e-20261019-181522-noise-s1x8` (noise with seed 1, 8 trials), and keeps it in the
open project's `runs/` folder, or else in the dataset's own `runs/`.

**Comparing runs.** The studio lists a dataset's kept runs below the result, newest first, with
what each calibrated from (exact, detected, or the noise) and how it came out (PASS or FAIL on
exact data, the reprojection RMS otherwise); **Show** brings one back, its 3-D estimate included.
On the command line, compare the `rows` or `row_stats` of the runs' `report.json` files: for
example two noise levels, or detections against exact observations of the same dataset.
