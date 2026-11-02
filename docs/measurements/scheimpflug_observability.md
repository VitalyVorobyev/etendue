# A Scheimpflug head's tilt, and what its calibration determines

This record investigates the Scheimpflug head that no procedure calibrated in G16
(`g16_procedures.md`, "Characterisation: one Scheimpflug head"). The model behind it is derived in
[`docs/derivations/scheimpflug_observability.md`](../derivations/scheimpflug_observability.md).

**Setup**
- Measured 2026-11-02 on an Apple M4 Pro.
- etendue commit `4512ddd`.
- calibration-rs `v0.12.0` (`8d5d1f08`), a clean worktree of the tag, built with
  `--config` patches.

**Datasets.** Each is the ground truth from `etendue gt`. The heads use `laser_ur5e`'s scenario:
24 views, 1296 corners and 3614 stripe points.

| Name | Built from | Camera |
|---|---|---|
| `head_s` | `laser_scheimpflug_ur5e`, keeping `cam_left` (the laser stays on the rig) | f = 2318.8 px, k1 = −0.08, tilt (0.05, −0.01) rad, p1 = p2 = 0 |
| `head_s` with k1 = −0.3, and with k1 = 0 | the same scene with only k1 changed | as `head_s` |
| `head` | `laser_ur5e`, keeping `cam_left` | untilted |
| the rig | `laser_scheimpflug_ur5e` as it is | two tilted cameras |

**Procedures.** `auto`, `laser_head` and `laser_rig` are the built-ins. The others are procedure
files that differ from `laser_head` only in each stage's `model`. Every stage is seeded as
`laser_head` seeds it: the device stage from the datasheet tilt, the hand-eye stage from the device
stage.

| Procedure | Device stage's model | Hand-eye stage's model |
|---|---|---|
| `p held` | `{"sensor": "scheimpflug", "fix": {"distortion": ["k3", "p1", "p2"], "sensor": []}}` | the same |
| `p free` | `{"sensor": "scheimpflug", "fix": {"distortion": ["k3"], "sensor": []}}` | the same |
| `held camera` | `laser_head`'s | `{"sensor": "design", "fix": {"intrinsics": ["fx","fy","cx","cy"], "distortion": ["k1","k2","k3","p1","p2"], "sensor": ["tilt_x","tilt_y"]}}`; for `head`, without the `sensor` list |

**The stripe check.** This check is frame-invariant. The 3614 true stripe points of every capture
are mapped into the gripper frame by the true poses. Their distance from the estimated laser plane
in the gripper frame (the estimated hand-eye composed with the estimated camera-frame plane) is then
measured. That distance is what a triangulation in the robot's frame inherits. The report's rows
(laser normal and hand-eye rotation) compare quantities in the camera frame.

## 1. On exact data, the tilt stays where it was seeded, and the fit is still right

`head_s`:

| Procedure | reprojection (px) | cx, cy (px) | tilt_x, tilt_y error (°) | p1, p2 | laser normal, camera frame (°) | hand-eye rotation (°) | stripe check, rms / max (µm) | report |
|---|---|---|---|---|---|---|---|---|
| `auto` | 3.3e-4 | 23.3, 5.52 | 0.135, 0.573 | 1.8e-4, 7.8e-4 | 0.576 | 2.95 | **169 / 376** | FAIL |
| `laser_head` | 3.5e-4 | 23.3, 5.52 | 0.135, 0.572 | 1.8e-4, 7.8e-4 | 0.576 | 0.591 | 0.29 / 0.36 | FAIL |
| `p held` | 1.5e-6 | 0.004, 0.0005 | 1e-5, 8e-5 | 0, 0 (held) | 9e-5 | 9e-5 | 0.00 / 0.00 | PASS |
| `p free` | 2.1e-4 | 0.07, 54.7 | 1.35, 0.001 | 1.9e-3, 2e-6 | 0.071 | 1.35 | 0.40 / 0.46 | FAIL |
| `held camera` | 3.0e-3 | 23.3, 5.52 | 0.135, 0.572 | 1.8e-4, 7.8e-4 | 0.576 | 0.591 | 0.19 / 0.30 | FAIL |

**`laser_head`.** Its device stage holds the tilt at the datasheet seed: tilt_x rounded to 3°
(0.05236 rad against 0.05) and tilt_y = 0 (against −0.01). The derivation's equivalence predicts
what absorbs the seed's error:

| Quantity | Predicted | Measured |
|---|---|---|
| cx | −f·δτ_y = −23.2 px | −23.3 px |
| cy | +f·δτ_x = +5.47 px | +5.52 px |
| p2 | −k1·δτ_y = +8.0e-4 | +7.8e-4 |
| p1 | +k1·δτ_x = −1.9e-4 | −1.8e-4 |
| camera frame rotation | (−0.135°, −0.573°, 0) | (−0.137°, −0.575°, −0.03°) |

The hand-eye stage frees the tilt but holds p1 and p2 at those values, so nothing moves it. Every
camera-frame row is off by the frame's rotation, while the stripe check is at 0.3 µm.

**`p free`.** With the tilt and p1, p2 all free, the solve wanders along the valley: a different
split, equally right for measurement (0.4 µm). **`p held`** finds the truth.

**`auto` is genuinely wrong.** Its hand-eye stage uses a pinhole camera, whose frame is not the
device stage's, while the laser plane comes from the device stage. That leaves the stripe check at
169 µm rms and 376 µm max on exact data.

## 2. With p1 and p2 free, no data determines the tilt; with them held, distortion does

Corner and laser noise 0.1 px, seed 1, 10 trials. For each quantity: the mean absolute error, then
calibration-rs's σ (first trial, final stage) in brackets.

| Dataset | Procedure | cx (px) | tilt_x (°) | tilt_y (°) |
|---|---|---|---|---|
| `head_s` (k1 −0.08) | `laser_head` | 24.3 [5.9] | 0.126 [0.087] | 0.597 [0.13] |
| `head_s` (k1 −0.08) | `p held` | 2.6 [5.6] | 0.060 [0.087] | 0.052 [0.121] |
| `head_s` (k1 −0.08) | `p free` | 193 [729] | 0.090 [0.35] | 4.75 [17.8] |
| `head_s`, k1 −0.3 | `p held` | 0.72 [1.5] | 0.011 [0.012] | 0.007 [0.025] |
| `head_s`, k1 −0.3 | `p free` | 226 [84] | 0.32 [0.016] | 5.53 [2.0] |
| `head_s`, k1 0 | `p held` | 162 [158] | 1.42 [2.45] | 4.0 [3.89] |
| `head_s`, k1 0 | `p free` | 163 [257] | 1.43 [2.93] | 4.02 [6.32] |

- **p1 and p2 free:** the tilt is undetermined for every lens (σ(tilt_y) 2–18°).
- **p1 and p2 held:** the tilt is determined through radial distortion, roughly as 1/|k1|. It is
  0.12° at k1 = −0.08 and 0.025° at k1 = −0.3. Without distortion it is undetermined again (3.9°).
- **`laser_head`'s σ is conditional.** It assumes p1 and p2 are known, but they were fixed at the
  device stage's values. Its tilt_y error, 0.6°, is 4.6 times the σ it reports.

## 3. The head procedures mix two cameras' frames

Same noise, 10 trials. The stripe check's rms per trial, averaged, then the largest single
distance:

| Head | Procedure | stripe rms, mean (µm) | max (µm) |
|---|---|---|---|
| `head_s` | `auto` | 325 | 874 |
| `head_s` | `laser_head` | 344 | 1028 |
| `head_s` | `p held` | 2276 | 3442 |
| `head_s` | `p free` | 808 | 4952 |
| `head_s` | `held camera` | **5.4** | **29** |
| `head` (untilted) | `laser_head` | 218 | 583 |
| `head` (untilted) | `held camera` | **5.4** | **29** |

Every head procedure takes the laser plane from the device stage and the hand-eye from the hand-eye
stage. When the hand-eye stage re-estimates the camera, it settles on another split of tilt,
principal point and frame: under noise, and through the valley of §2 even on exact data. The plane
and the hand-eye then sit in two different camera frames. Holding the device stage's camera in the
hand-eye stage keeps one frame and makes the stripe check 40–60 times smaller. This happens with an
untilted camera too: there the frames differ only by noise, which already puts the stripe check at
0.2 mm.

The joint laser hand-eye (`rig_handeye_laserline`) would avoid it, but calibration-rs needs at least
two cameras for it ("insufficient data: need 2, got 1").

## 4. The rig is observable because calibration-rs holds p1 and p2 there

`laser_rig` on `laser_scheimpflug_ur5e`, seeded from the datasheet values:
- **Exact data:** every row is within 1e-6, and the report passes. p1 and p2 come back exactly
  zero, so they are held, and the tilt is recovered through k1.
- **At 0.1 px over 10 trials:** the two cameras' errors are

  | Quantity | First camera | Second camera |
  |---|---|---|
  | cx (px) | 3.7 | 6.7 |
  | tilt_x (°) | 0.066 | 0.065 |
  | tilt_y (°) | 0.088 | 0.144 |
  | laser normal (°) | 0.094 | 0.17 |

  and the hand-eye rotation's error is 0.149°.

The second camera, the robot and the shared laser add nothing (derivation, §4).

## Correction to G16

`g16_procedures.md` closes its Scheimpflug characterisation with "the pose set itself has to fix the
observability, which is what P18's planner is for". That is wrong:
- With p1 and p2 free, no pose set determines the tilt.
- With them held, more views help only as 1/√N.
- The tilt is distinguished only by radial distortion.

The camera-frame failures in that table are a different split of the same projection. The stripe
check shows `laser_head`'s result to be right on exact data and `auto`'s to be wrong.

## Procedure

```bash
etendue gt <scene> <scenario> -o m/gt                                          # each dataset
etendue calibrate m/gt --procedure <built-in | file> -o <p>.exact.json        # exact
etendue calibrate m/gt --procedure <...> --noise noise.json --trials 10 -o <p>.noise.json
# noise.json: {"pixel_sigma_px": 0.1, "laser_sigma_px": 0.1, "seed": 1}
```

The stripe check composes the report's `estimate.handeye` with `estimate.laser_planes[0]` and
measures each true stripe point's distance from the plane. Each point is `base_se3_tcp⁻¹ ·
world_se3_target · (position_m, 0)`, from `gt.json`'s captures.
