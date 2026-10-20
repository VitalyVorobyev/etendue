# Ground truth and closed-loop calibration

The question behind every calibration cell is: *how well will this setup calibrate?* etendue
answers it by closing the loop. It knows the truth (the scene), it produces what a calibration
would observe (exactly, with controlled noise, or from rendered images), it runs calibration-rs on
those observations, and it compares every estimated quantity with the truth. On exact
observations the loop is a test of conventions and of the solver: everything must come back to
within numerical noise. With noise, it measures what your design can achieve.

## Ground truth

`etendue gt <scene> <scenario> -o <dir>` bakes the scenario and writes three files:

- `dataset.json`, a calibration-rs dataset: the image list of each camera, the board, the
  calibration problem the scene poses, and for robot scenes the robot poses with their explicit
  convention (`base_se3_<tcp_link>`, quaternion `xyzw`, metres);
- `robot_poses.json`, the robot's reported TCP pose at every capture;
- `gt.json`, the truth: each camera's model (and, for a physical camera, where its projection
  frame sits in its body), the rig's `cam_se3_rig`, the hand-eye transforms, the target, the true
  laser planes, and, for every capture and camera, every target point's exact pixel and whether it
  is visible.

Pixels come from the camera model itself, never from images. A point is visible when it is in
front of the camera, at least 2 px inside the image, and on the side of the target facing the
camera.

**The calibration problem** follows from where things hang in the [frame tree](frames.md):

| Scene | Problem |
|---|---|
| one fixed camera | intrinsics (with a tilt for a Scheimpflug camera) |
| several fixed cameras on a rig | rig extrinsics |
| cameras on the robot, board fixed | eye-in-hand (one camera, or a rig) |
| board on the robot, cameras fixed | eye-to-hand |
| lasers on a hand-eye rig | rig hand-eye with laser planes: one laser all cameras see, or one per camera |
| one camera carrying its laser | a laser-line device (a triangulation head): intrinsics and plane, plus the hand-eye |

`etendue calibrate` (below) solves the problems with a robot. The datasets of fixed cameras are
written for calibration-rs's own tools.

## Observations

A calibration is run on **observations**: the target points each camera found in each view, the
laser-stripe pixels, and the robot poses. etendue makes them in three ways.

**Exact.** The ground truth's pixels as they are.

**With noise.** A noise model perturbs the truth the way a detector and a controller would:

| Field | Effect, per seen target point or pose |
|---|---|
| `pixel_sigma_px` | Gaussian pixel noise, per axis |
| `outlier_fraction`, `outlier_sigma_px` | this share of points is displaced by a much larger Gaussian (a misdetected corner) |
| `dropout_fraction` | this share of points is missed |
| `laser_sigma_px` | Gaussian noise on laser-stripe pixels, per axis |
| `robot_rot_sigma_rad`, `robot_trans_sigma_m` | the reported TCP pose is off by a random rotation vector and translation, per component |
| `seed` | the random stream |

Every field defaults to zero. A view keeps its target points only if at least four remain, and
its stripe only with at least two. One seeded stream (ChaCha8) is drawn in a fixed order, so the
same truth and noise give the same observations on every platform, in the studio as on the
command line.

Try it: raise the pixel noise, the outliers and the dropouts, and compare the observed corners
with the true ones in one camera's view.

<div class="etendue-demo" data-demo="noise"></div>

**From images.** `etendue detect` finds the board (and the laser stripe) in rendered images, and
the calibration runs on what it found (see [Synthetic images](synthetic_images.md)).

## Calibration and comparison

`etendue calibrate <dir>` runs calibration-rs's own workflows natively: single-camera hand-eye,
rig hand-eye, rig hand-eye with laser planes, and the laser-line device; eye-in-hand and
eye-to-hand. It then compares every estimate with the truth, after re-expressing the truth in
calibration-rs's frames (its rig frame is the first camera's):

| Quantity | Compared as |
|---|---|
| reprojection | mean distance between observed and reprojected points (px) |
| `fx`, `fy` | relative error |
| `cx`, `cy` | error in pixels |
| distortion `k1 … p2`, sensor tilt | absolute error |
| `cam_se3_rig` of each other camera | rotation (°) and translation (mm) |
| hand-eye, `base_se3_target` | rotation (°) and translation (mm) |
| laser plane | angle between the normals (°) and difference in distance (mm) |

### Pass on exact observations

On exact observations the loop **passes** when:

| Quantity | Limit |
|---|---|
| mean reprojection error | ≤ 0.05 px |
| focal lengths | ≤ 1e-4 relative |
| principal point | ≤ 0.05 px |
| hand-eye rotation and translation | ≤ 0.01° and ≤ 0.05 mm |
| laser plane normal and distance | ≤ 0.01° and ≤ 0.05 mm |

These limits apply only to exact input; with noise or detected corners the same rows are reported
for comparison, not judged. The examples pass by orders of magnitude:

| Example | Problem | Results |
|---|---|---|
| `closed_loop_ur5e` | rig hand-eye, 2 cameras, 24 views | reprojection 7.7e-7 px, fx 2.0e-8, principal point 7.2e-4 px, hand-eye 1.5e-7° / 1.3e-6 mm |
| `eye_to_hand_ur5e` | eye-to-hand rig | hand-eye 1.6e-5° / 2.7e-4 mm |
| `laser_ur5e` | rig hand-eye with a laser plane | plane normal 2.1e-7° and 1.7e-5°, distance 6.6e-8 mm |
| `rig3_laser_ur5e` | three cameras, one laser each, 70 views | plane normals ≤ 1.2e-5°, distances ≤ 2.8e-7 mm |
| `laser_ur5e`, one camera | laser-line device | plane normal 1.0e-6°, distance 1.5e-7 mm, fx 1.0e-9 |

The residues are the solver's convergence tolerance, not a disagreement: any mismatch of a frame
or pose convention between etendue and calibration-rs would show up as millimetres and degrees.
The native results agree with those of calibration-rs's Python package row for row, to within
$5 \times 10^{-12}$ on every passing example.

### With noise

Noise turns the loop into a prediction. With `--trials N` the calibration is repeated with seeds
`seed, seed + 1, …` and each row's mean, spread and maximum are reported. On `closed_loop_ur5e`
with 0.5 px of pixel noise, eight trials (2592 points each):

| Quantity | Mean error over 8 seeds |
|---|---|
| residual, RMS per axis | 0.491 px |
| fx | 2.5e-3 relative (cam_left), 1.2e-3 (cam_right) |
| principal point | 5.8 / 5.0 px (cam_left), 3.0 / 3.3 px (cam_right) |
| hand-eye | 0.21° / 1.4 mm |

The residuals are *smaller* than the noise, as they must be: a fit with $p$ free parameters to
$2n$ measured coordinates absorbs part of the noise, and the expected RMS per axis is

$$
\sigma\sqrt{1 - \frac{p}{2n}} .
$$

With $\sigma = 0.5$ px and $2n = 5184$ that is 0.498 px for the 34 parameters of the cameras and
the hand-eye, and 0.491 px if each view's robot-pose refinement counts too ($p = 178$); the
measured 0.491 px agrees. The intrinsics are the weak point here: the views tilt only 8–25° from
the board's normal, so the principal point is poorly constrained. A wider spread of views is the
fix, which the loop lets you verify before building anything.

### From rendered images

On the same example rendered and detected (24 views, 2592 corners found, none mislabelled, 0.058
px RMS from the truth), the rig calibrates to a mean reprojection error of 0.052 px, fx within
1.9e-4, and a hand-eye error of 0.012° / 0.14 mm. The focal lengths, principal points and
hand-eye each lie within the spread of the same calibration from exact corners with white noise of
the same size, so the detector adds noise but no measurable bias. The detector's error is not
quite white (it depends on each corner's sub-pixel position), which shows only in the mean
reprojection error, 0.0005 px above the white-noise runs.

### A Scheimpflug rig

`laser_scheimpflug_ur5e` is `laser_ur5e` with tilted sensors (about 2.9° and 2.0° about x). It
does **not** pass, even on exact observations: the plane normals are off by 0.0095° and 0.024°
and the hand-eye rotation by 0.034°, against 0.01° limits, and the principal points miss by
0.3–2 px. The tilt, the focal length and the principal point trade off along a nearly flat valley
of calibration-rs's Scheimpflug model, so the solver stops short of the truth. Seeding it with
datasheet values (focal length, principal point at the centre, the tilt to a whole degree) helps
but does not close the gap. If you design a Scheimpflug head, expect its calibration to need more,
and more varied, views than an untilted one.

## Where it is

- **Command line**: `etendue gt`, `etendue calibrate` (`--noise`, `--trials`, `--camera`,
  `--features`); see [Calibration](../calibration.md).
- **Studio**: the Calibrate workspace previews the observations (exact or noisy) in the browser;
  the desktop app also generates the dataset and calibrates it, showing each estimate beside the
  truth.

## Assumptions and limits

- **Noise is independent.** Pixel noise is white and identical for every point; a real detector's
  error depends on blur, contrast and the corner's sub-pixel position.
- **Robot pose noise is random**, not systematic: a robot whose kinematics are off by a fixed
  amount is not modelled.
- **One board per dataset**, and a dataset is written for chessboard and ChArUco targets.
- **The comparison is only as good as the truth's frames**: the hand-eye is compared in
  calibration-rs's frames, and a physical camera's estimate is its projection frame, not its body
  (`mech_se3_projection` links the two).

API: `etendue-synth` `dataset`, `observe`; `etendue-cli` `calibrate`; `@etendue/wasm`
`EtendueScene.observe`.
