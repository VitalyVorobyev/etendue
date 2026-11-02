# A head's laser plane and hand-eye from one camera (G5.3c)

`auto` on a single triangulation head, and the `laser_head` procedure, took the laser plane from
one camera solution and the hand-eye from another. This record measures what that cost, with a
check of the calibration as a measuring device, and the fix.

**Setup**
- Measured 2026-11-02 on an Apple M4 Pro.
- etendue `7bad250` with this change (branch `feat/head-triangulation-check`).
- calibration-rs `v0.12.0` (`8d5d1f08`), a clean worktree of the tag, built with `--config`
  patches. The rows marked *joint* use calibration-rs `98ef7517` with the one-camera change of
  `rig_handeye_laserline` (calibration-rs#268).

**Datasets** (`etendue gt`, `laser_ur5e`'s scenario: 24 views, 3614 stripe points):
- `head`: `laser_ur5e` keeping `cam_left`, with the laser on it. Untilted, f = 2318.8 px.
- `head_s`: the same from `laser_scheimpflug_ur5e`, tilt (0.05, −0.01) rad, k1 = −0.08
  (`scheimpflug_observability.md`).
- The rigs: `laser_scheimpflug_ur5e` and `rig3_laser_ur5e` as they are.

## The check

For each camera with a laser, every true stripe pixel (the ground truth's, noise-free) is:
1. back-projected with the estimated camera;
2. met with the estimated laser plane (camera frame);
3. carried into the robot base through the estimated hand-eye and the true robot pose.

The rows are the rms and the largest distance from the true point, in mm. The largest is gated at
0.05 mm on exact data (G5.3c), the limit of the hand-eye translation and the plane offset.

This is what a measurement inherits, and nothing else: it does not depend on how a solve splits
the projection among tilt, principal point and camera frame
(`docs/derivations/scheimpflug_observability.md` §5). The camera-frame rows do depend on that
split, and none of them sees a camera, a plane and a hand-eye that do not belong together.

`scheimpflug_observability.md` §3 measured only the distance from the estimated plane in the
gripper frame. That misses every error within the plane, and understated the problem 100-fold.

## Why the head is sensitive

`head` triangulates at a narrow angle: the laser is about 40 mm from the camera, and the stripe is
0.52–0.59 m away. A stripe pixel moved by 1 px moves its triangulated point by 0.22–3.8 mm (median
1.6 mm), against 0.24 mm across the line of sight. A camera that disagrees with the plane's camera
by a pixel anywhere in the image puts points millimetres off.

## Before

Exact data:

| Head | Procedure | rms (mm) | max (mm) | report |
|---|---|---|---|---|
| `head` | `auto` | 1.3e-5 | 1.4e-5 | PASS |
| `head` | `laser_head` | 1.0e-3 | 1.2e-3 | PASS |
| `head_s` | `auto` | **29.2** | **31.3** | FAIL |
| `head_s` | `laser_head` | 1.2e-3 | 3.4e-3 | FAIL (camera-frame rows) |

0.1 px of corner and laser noise, seed 1, 10 trials. The mean of the trials' rms, and the largest
distance over all trials:

| Head | Procedure | rms (mm) | max (mm) | per-trial rms (mm) |
|---|---|---|---|---|
| `head` | `auto` | **0.25** | 0.59 | 0.09–0.54 |
| `head` | `laser_head` | **3.0** | 8.5 | 0.06–7.4 |
| `head_s` | `auto` | **29.2** | 31.4 | 29.1–29.3 |
| `head_s` | `laser_head` | **4.7** | 15.1 | 0.24–13.1 |

What went wrong, by procedure:
- **`auto`.** Its device stage gives the camera and the plane, and a separate pinhole hand-eye
  solve gives the hand-eye. The two camera frames differ:
  - on `head`, by noise (about 0.4 mrad, so 0.2 mm at 0.55 m);
  - on `head_s`, by the tilt the pinhole model absorbs (2.95° of hand-eye rotation), so 29 mm.
- **`laser_head`.** Its result takes the camera and the hand-eye from the hand-eye stage, and the
  plane from the device stage (`procedure::run` carries planes forward). Re-estimated from target
  views alone, the camera moves by about a pixel: in trial 4 on `head`, cx moves 1.5 px and k2
  moves from 0.022 to 0.064. Rays from that camera meet a plane fitted with the other one, at a
  sensitivity of up to 3.8 mm/px.

## After: the hand-eye holds the device stage's camera

`auto`'s head hand-eye (`calibrate::head`) and `laser_head`'s second stage now hold the device
stage's camera: intrinsics, distortion and tilt, seeded from it. The plane and the hand-eye then
share one camera. On a head the two give the same numbers.

| Head | Data | rms (mm) | max (mm) | report |
|---|---|---|---|---|
| `head` | exact | 1.3e-4 | 1.3e-4 | PASS |
| `head` | 0.1 px, 10 trials | **0.013** | 0.061 | — |
| `head_s` | exact | 5.9e-4 | 9.3e-4 | FAIL (camera-frame rows) |
| `head_s` | 0.1 px, 10 trials | **0.013** | 0.061 | — |

At 0.1 px of noise:
- **The rms improves 19–2200-fold.** The noise's own floor is about 0.013 mm; the rigs below
  reach the same.
- **The hand-eye's own error improves a little:** on `head` it goes from 0.033° and 0.12 mm to
  0.026° and 0.11 mm, the mean over the trials.

`head_s` still fails its camera-frame rows. Its tilt seed is 0.57° off, and the principal point
and the camera frame absorb it. That error is not a measurement error (the triangulated rows
pass), and it is described in `scheimpflug_observability.md`.

## The joint solve, with one camera

With calibration-rs's joint laser hand-eye (`rig_handeye_laserline`) opened to one camera, a
procedure of the device stage then the joint stage (seeded from it, intrinsics and tilt refined)
gives:

| Head | exact: rms / max (mm) | 0.1 px, 10 trials: rms / max (mm) |
|---|---|---|
| `head` | 3.0e-6 / 7.9e-6 | 0.016 / 0.075 |
| `head_s` | 8.5e-4 / 2.5e-3 | 0.014 / 0.064 |

The joint solve is right too, and on this head no more precise than holding the device's camera
(0.013 mm). Its camera-frame rows split the tilt the same way.

## Rigs

`auto` (the joint solve), exact data:
- `laser_scheimpflug_ur5e`: 9.0e-7 and 9.0e-7 mm rms, PASS.
- `rig3_laser_ur5e`: 2.6e-6 to 3.1e-6 mm rms, PASS.

`laser_scheimpflug_ur5e` at 0.1 px over 10 trials: 0.014 and 0.015 mm rms (max 0.061 and 0.087
mm).

## G16.1

G16.1 (P16a) pinned `auto`'s reports to the reports before procedures, bit for bit. That still
holds where nothing here changed: datasets without lasers. Two things differ from it now:
- **Every laser report** has the two triangulated rows per laser camera.
- **`auto_head`'s hand-eye** comes from the held device camera, not a pinhole solve. On `head`,
  exact, the hand-eye rows go from 1.3e-7° and 1.1e-7 mm to 4.5e-7° and 1.2e-4 mm.

G13.1's parity with the wheel's script leaves the triangulated rows out (`parity.py`): the script
has no counterpart. Its other rows are unchanged (`laser_ur5e`: 29 rows, max |Δ| 5.5e-13, PASS).

## Procedure

```bash
etendue gt <scene> <scenario> -o m/gt
etendue calibrate m/gt [--procedure laser_head] -o exact.json
etendue calibrate m/gt [--procedure laser_head] --noise noise.json --trials 10 -o noise.json
# noise.json: {"pixel_sigma_px": 0.1, "laser_sigma_px": 0.1, "seed": 1}
```

The "before" rows were measured with the triangulated rows added and the procedures unchanged.
The regression test `a_heads_plane_and_hand_eye_come_from_one_camera`
(`crates/etendue-cli/tests/calibrate.rs`) runs `auto` and `laser_head` on a noisy `head`.
