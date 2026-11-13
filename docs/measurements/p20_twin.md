# P20b: the twin of a real calibration (gate T1, characterisation T2)

The twin of a real calibration is the same cell, simulated: the real result (cameras, rig,
hand-eye) as its truth, the real robot poses, and in each view exactly the points the real detector
found, observed with the noise the real residuals imply. It is calibrated with the real run's
workflow and configuration. Where the real run and its twin differ, etendue's simulation (or the
models calibration-rs fits) is wrong.

- **Gate T1:** the twin recovers its truth on exact data (G5.1's limits), and its scatter over
  trials matches its predicted σ (std/σ within 15 %, G17.1's parameters: each camera's fx and cx,
  and the hand-eye).
- **Characterisation T2:** the real run against its twin, per camera.

**Verdict.** T1 exact: **PASS** on both. T1 scatter: **PASS** on `kuka_1`; **FAIL** on `DS8`, on
the three cameras' cx (std/σ 1.21, 1.18, 1.92; the hand-eye and fx pass). The gate is not relaxed;
the cause is below. T2: the twin reproduces `kuka_1`'s residual level; both real datasets have
spatial structure the twin does not, `DS8` much more, and their σ is inflated by calibration-rs's
view scatter accordingly.

## When and where

- Measured 2026-11-13, Apple M4 Pro, rustc 1.95.0, release builds.
- etendue commit `06c66a2`, branch `p20b/twin` (the numbers); the diagnostic run below on
  `86e3a64`, which adds only the procedure override and the view scatter line.
- calibration-rs `v0.18.0`, a clean clone of the tag.

## Method

`tests/t1.rs`, run with
`cargo test --release -p etendue-cli --test t1 -- --ignored --nocapture`. For each dataset it:
1. calibrates the real dataset (`etendue real calibrate`'s library call) and keeps the run;
2. builds its twin (`etendue real twin`'s library call, `etendue_cli::real::twin`):
   - truth: the run's export (`result.json`);
   - views: calibration-rs's dataset runner's input rebuilt from the run's detection cache, so the
     robot poses are those the calibration took and each view has the detector's points in its
     order;
   - noise: calibration-rs's reprojection σ (its `noise`, from the median absolute residual), and
     on the robot poses the pose error the real run weighed its pose corrections with (estimated
     by calibration-rs, `robot_pose_sigma`);
3. calibrates the twin's exact observations once (T1, exact), then 200 noisy trials, seeds
   100–299, with `auto`'s procedure: the workflow the manifest's topology picks, at calibration-rs's
   defaults, which is the real run's (`calibrate_dataset` with no config);
4. compares the trials' std with the mean predicted σ (`param_stats`, G17.1's convention: the
   hand-eye error in calibration-rs's tangent), and the real run with its twin
   (`etendue_cli::real::versus`).

Whiteness (`versus`) is `Σ n |mean|² / σ²` per degree of freedom over bins with at least five
points, `σ` being the camera's own RMS per axis: across the image on an 8 × 6 grid, and within each
view on a 4 × 3 grid of its own. It is about 1 for white residuals; the twin's score is the
reference (below 1, since the fit absorbs part of the noise). A view's own mean is not scored: the
calibration refines each view's robot pose, which absorbs it. The radial column is the mean radial
residual (projected − observed, positive outward) in four rings out to the farthest corner.

The datasets are public; this file gives aggregate numbers only, as for private data.

## Data

As in `p20a_r1.md`: `kuka_1` (one camera, 17 × 28 chessboard, 30 views, eye-in-hand), and `DS8`
with its image globs adapted to the local PNG copy (three cameras, 10 × 14 chessboard, 33 of 42
views used, eye-in-hand).

## T1

**Exact observations.** Every G5.1 row passes on both twins, by 3 orders of magnitude or more:
mean reprojection 1.7e-6 px (`kuka_1`) and 1.3e-7 px (`DS8`); fx within 3.4e-8 relative; cx, cy
within 7e-5 px; hand-eye within 1.5e-6° and 7.3e-5 mm.

**Scatter, `kuka_1`** (0.108 px; robot 7.6e-4 rad, 0.45 mm): **PASS**.

| Parameter | Unit | std | predicted σ | std / σ | gated |
|---|---|---|---|---|---|
| cam0 fx | px | 3.227e-1 | 2.925e-1 | 1.10 | yes |
| cam0 fy | px | 3.176e-1 | 2.900e-1 | 1.10 |  |
| cam0 cx | px | 2.192e-1 | 2.335e-1 | 0.94 | yes |
| cam0 cy | px | 1.931e-1 | 1.975e-1 | 0.98 |  |
| cam0 k1 |  | 2.027e-4 | 2.087e-4 | 0.97 |  |
| cam0 k2 |  | 8.776e-4 | 8.856e-4 | 0.99 |  |
| cam0 p1 |  | 1.901e-5 | 1.933e-5 | 0.98 |  |
| cam0 p2 |  | 2.373e-5 | 2.466e-5 | 0.96 |  |
| hand-eye rx | deg | 2.033e-2 | 1.872e-2 | 1.09 | yes |
| hand-eye ry | deg | 1.821e-2 | 1.785e-2 | 1.02 | yes |
| hand-eye rz | deg | 1.348e-2 | 1.329e-2 | 1.01 | yes |
| hand-eye tx | mm | 3.446e-1 | 3.112e-1 | 1.11 | yes |
| hand-eye ty | mm | 3.404e-1 | 3.446e-1 | 0.99 | yes |
| hand-eye tz | mm | 3.585e-1 | 3.532e-1 | 1.01 | yes |

**Scatter, `DS8`** (0.185 px; robot 1.65e-3 rad, 2.05 mm): **FAIL** on cx.

| Parameter | Unit | std | predicted σ | std / σ | gated |
|---|---|---|---|---|---|
| camera0 fx | px | 1.191e0 | 1.240e0 | 0.96 | yes |
| camera0 cx | px | 4.442e-1 | 3.679e-1 | **1.21** | yes |
| camera0 cy | px | 4.367e-1 | 3.706e-1 | 1.18 |  |
| camera1 fx | px | 1.192e0 | 1.234e0 | 0.97 | yes |
| camera1 cx | px | 4.319e-1 | 3.654e-1 | **1.18** | yes |
| camera1 cy | px | 4.414e-1 | 3.677e-1 | 1.20 |  |
| camera2 fx | px | 6.964e-1 | 6.166e-1 | 1.13 | yes |
| camera2 cx | px | 3.516e-1 | 1.830e-1 | **1.92** | yes |
| camera2 cy | px | 2.569e-1 | 1.847e-1 | 1.39 |  |
| camera2 p1 |  | 1.113e-4 | 8.633e-5 | 1.29 |  |
| camera2 p2 |  | 1.750e-4 | 9.226e-5 | 1.90 |  |
| hand-eye rx | deg | 1.185e-1 | 1.040e-1 | 1.14 | yes |
| hand-eye ry | deg | 8.442e-2 | 8.110e-2 | 1.04 | yes |
| hand-eye rz | deg | 5.227e-2 | 5.551e-2 | 0.94 | yes |
| hand-eye tx | mm | 4.070e0 | 3.944e0 | 1.03 | yes |
| hand-eye ty | mm | 5.187e0 | 5.146e0 | 1.01 | yes |
| hand-eye tz | mm | 3.997e0 | 3.786e0 | 1.06 | yes |

(The other parameters, fy and the distortion of cameras 0 and 1, and camera 2's k1 and k2, are within
0.96–1.15.)

**Why `DS8` fails: the covariance holds the rig.** calibration-rs's rig hand-eye solve holds each
camera's `cam_se3_rig` at its default (`refine_cam_se3_rig: false`; its `fixed` list names them), so
its covariance is conditional on the rig, and the principal point, which trades off against a
camera's pose on the rig, is understated most. This is G17.1's finding on `closed_loop_ur5e`
(`g17_sim_lab.md`: up to 2.4× there), now on a real cell's pose set. A diagnostic run (the same 200
trials, seeds and noise, `ETENDUE_T1_PROCEDURE` with one `rig_handeye` stage and
`"free": {"refine_rig": true}`) passes: every gated ratio is within 0.93–1.14 (camera0, 1, 2 cx:
1.01, 1.00, 1.03; hand-eye 0.94–1.14), and every parameter within 0.89–1.14. T1 is the real run's
configuration, so it stays **FAIL** on `DS8` as measured; it would pass with the rig refined, which
is not what calibration-rs's runner does. The real run's σ (below) has the same conditioning.

## T2: the real run against its twin

### Residuals, per camera (real / twin)

| Dataset | Camera | Points | RMS real (px) | RMS twin (px) | Ratio | Across the image | Within views | Largest bin mean (px) | Radial, centre → edge (px) |
|---|---|---|---|---|---|---|---|---|---|
| `kuka_1` | cam0 | 14280 | 0.1077 | 0.1070 | **1.01** | 2.33 / 0.67 | 1.28 / 0.60 | 0.115 / 0.056 | −0.006 +0.003 −0.003 +0.005 / +0.001 −0.001 −0.001 +0.004 |
| `DS8` | camera0 | 4480 | 0.2109 | 0.1843 | **1.14** | 2.97 / 1.03 | 21.0 / 0.78 | 0.161 / 0.122 | −0.005 +0.004 −0.006 +0.105 / −0.003 +0.001 −0.002 −0.038 |
| `DS8` | camera1 | 4480 | 0.2204 | 0.1835 | **1.20** | 8.78 / 0.99 | 21.0 / 0.80 | 0.262 / 0.123 | −0.011 +0.007 −0.013 +0.109 / +0.002 −0.001 −0.008 −0.012 |
| `DS8` | camera2 | 4480 | 0.2329 | 0.1829 | **1.27** | 3.17 / 0.93 | 12.7 / 0.90 | 0.215 / 0.121 | +0.002 −0.004 +0.006 −0.018 / +0.003 −0.001 −0.008 −0.023 |

### σ and distance, per parameter

Real σ is calibration-rs's on the real data. calibration-rs scales it by its **view scatter**: how
many times the variance of the parameters across views exceeds what the residual noise predicts
(leave one view out). `kuka_1`'s is 2.79 (√ = 1.67), `DS8`'s 25.2 (√ = 5.02). The twin's views
scatter no more than its noise predicts, so its σ is not scaled. Distance is
`(real − twin mean) / twin std`.

| Dataset | Parameter | real σ / twin std | distance (σ) |
|---|---|---|---|
| `kuka_1` | cam0 fx, fy | 1.52, 1.53 | +0.03, +0.02 |
| `kuka_1` | cam0 cx, cy | 1.78, 1.71 | −0.04, −0.06 |
| `kuka_1` | cam0 k1, k2, p1, p2 | 1.69–1.74 | −0.06 to +0.01 |
| `kuka_1` | hand-eye rx, ry, rz | 1.54, 1.64, 1.65 | −0.13, −0.08, −0.07 |
| `kuka_1` | hand-eye tx, ty, tz | 1.52, 1.70, 1.65 | +0.02, +0.08, +0.06 |
| `DS8` | camera0 fx, cx | 6.09, 4.99 | −0.01, +0.00 |
| `DS8` | camera1 fx, cx | 6.05, 5.10 | +0.06, −0.09 |
| `DS8` | camera2 fx, cx | 5.18, 3.14 | +0.04, −0.03 |
| `DS8` | distortion, every camera | 3.18–6.29 | −0.18 to +0.08 |
| `DS8` | hand-eye rx, ry, rz | 4.46, 4.91, 5.48 | +0.01, −0.03, +0.06 |
| `DS8` | hand-eye tx, ty, tz | 4.92, 5.05, 5.18 | −0.08, −0.02, +0.03 |

### Reading it

- **`kuka_1`: the noise model holds in level, not in shape.** The RMS ratio is 1.01: the real
  residuals are as large as Gaussian noise at calibration-rs's σ, without heavy tails. But they are
  not white across the image (2.33 against the twin's 0.67; bins up to 0.115 px against 0.056), and
  mildly not within views (1.28 against 0.60). No radial trend (every ring within 0.006 px), so it
  is not the radial distortion model. A smooth field that is neither radial nor absorbed by each
  view's pose points at the target (a board that is not flat, or printed off scale) or at a
  position-dependent detector bias. It makes the views disagree a little (view scatter 2.79), and
  the real σ is 1.5–1.8 times what white noise of the same level gives.
- **`DS8`: a model error.** The RMS ratio is 1.14–1.27: the real residuals have heavier tails than
  the median-based σ implies (the quality report's outlier views, P20a). Within views they are far
  from white (12.7–21, against 0.8–0.9): each view has a residual pattern its pose cannot absorb,
  and it differs between views, which is what view scatter 25 measures: the real σ is 5 times the
  twin's. On the two JAI cameras (0 and 1) the outer ring's mean radial residual is +0.11 px
  (outward) against the twin's −0.04 and −0.01: the projected points sit outside the observed ones
  at the image edge, where the five-parameter Brown-Conrady model with k3 held (calibration-rs's
  default) does not bend far enough. That suggests freeing k3 or a richer radial model for those
  lenses. The rest of the structure is not radial (camera 2 has none, and the within-view score
  dwarfs the across-image one): view-dependent, as a target that is not flat or not rigid on the
  board, or a detector whose bias depends on the board's pose, would make it.
- **The real estimates are where their twins land** (every distance within 0.18 σ). The twin's
  truth is the real estimate, so this distance is the twin's bias: the estimator has none worth
  naming at these noise levels and pose sets, and the real-versus-twin differences above are in the
  residuals and the σ, not in where the estimates fall.
