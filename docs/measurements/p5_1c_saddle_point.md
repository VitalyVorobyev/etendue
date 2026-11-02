# P5-1c: the saddle point as `etendue detect`'s chessboard corners

The decision of 2026-11-01 (user): `etendue detect` may default to ChESS with the saddle-point
refiner if the image-level loop (P5-1b) confirms what G4.2 showed. This is that loop, run with
both corner detectors on the same images.

**Every number here is from etendue's own renders.** Their clean, box-filtered corners are close
to the ideal model the saddle-point refiner fits. They say which detector is better on synthetic
images, not on a real camera's.

## When and where

- Measured 2026-11-02, Apple M4 Pro, Blender 5.2.2 (Cycles, Metal).
- etendue commit `4c06f0f`, branch `p5-1c/detect-corners`.
- calibration-rs `v0.12.0`, a clean clone of the tag.
- calib-targets 0.16.1, chess-corners 2.1.

## Setup

P5-1b's procedure (`closed_loop_images.md`):
- `examples/closed_loop_ur5e`: 24 poses, two cameras, so 48 images, and a 9 × 6 chessboard;
- rendered linear at s = 4 with 16 samples (`sensor_linear.json`);
- `etendue detect --corners radon` and `--corners saddle_point` on the same images;
- `etendue calibrate --features` from each.

## Detection

Both detectors kept 48/48 views and labelled 2592 corners: 0 mislabelled, 0 unmatched, 0 off the
board.

| Corners | cam_left RMS | cam_right RMS | max |
|---|---|---|---|
| Radon (`DetectorConfig::radon`) | 0.0578 px | 0.0583 px | 0.137 px |
| ChESS + saddle point | **0.0114 px** | **0.0130 px** | 0.166 px |

## Calibration from the detected corners

Rig hand-eye, the closed-loop limits for reference:

| Quantity | Radon | Saddle point |
|---|---|---|
| mean reprojection | 0.0527 px | **0.0087 px** |
| cam_left fx (rel) | 1.8e-4 | **3.2e-5** |
| cam_left cx | 0.344 px | **0.006 px** |
| cam_left cy | **0.019 px** | 0.072 px |
| cam_right fx (rel) | 7.4e-5 | **6.7e-6** |
| cam_right cx | 0.133 px | **0.091 px** |
| cam_right cy | **0.013 px** | 0.060 px |
| hand-eye rotation | 0.0116° | **0.0017°** |
| hand-eye translation | 0.122 mm | **0.019 mm** |

**White-noise reference.** For comparison, exact observations were calibrated with white noise
at each detector's RMS (`--noise`, 20 trials, seeds 1–20). Each cell is mean ± std:

| Quantity | 0.0122 px | 0.058 px |
|---|---|---|
| cam_left cx | 0.094 ± 0.065 px | 0.45 ± 0.30 px |
| cam_left cy | 0.092 ± 0.055 px | 0.43 ± 0.27 px |
| cam_right cx | 0.120 ± 0.099 px | 0.57 ± 0.47 px |
| cam_right cy | 0.076 ± 0.056 px | 0.36 ± 0.26 px |
| hand-eye rotation | 0.0037 ± 0.0015° | 0.017 ± 0.007° |
| hand-eye translation | 0.027 ± 0.018 mm | 0.12 ± 0.09 mm |

## Reading

- **Detection.** The saddle point locates a chessboard's corners about five times more
  accurately than Radon.
- **Calibration.** Its calibration is better wherever the errors are well determined: the
  reprojection, the focal lengths and the hand-eye (7 times).
- **The principal point.** It is weakly determined by these 24 views. Its cy errors (0.06–0.07 px)
  are larger than Radon's, but they lie within the white-noise spread at the saddle point's own
  RMS. They are noise, not a bias, and Radon's small cy errors are within its own spread too.
- **Every saddle-point error is within or below its white-noise spread.** The mean reprojection
  (0.0087 px) is below white noise's (0.0151 px), because part of a detector's error is
  systematic and the solve absorbs it.

**Other kinds.** The detector tests (`crates/etendue-cli/tests/detect_targets.rs`) run every
kind with the saddle point too:
- the chessboard tests pass;
- a ChArUco board turned 100° loses 1 of 54 corners;
- the PuzzlePole mis-places 3 of 101 (`unmatched`).

Their marker bits and edges near a corner disturb the saddle fit.

## Decision

`etendue detect` uses ChESS with the saddle-point refiner on a chessboard, and keeps Radon (ChESS
located by Radon) on the other kinds. `--corners radon|saddle_point` chooses for every kind. The
studio's dataset generation follows the same default.

## Procedure

```bash
etendue gt examples/closed_loop_ur5e/scene.json examples/closed_loop_ur5e/scenario.json -o ds
etendue render examples/closed_loop_ur5e/scene.json examples/closed_loop_ur5e/scenario.json -o ds \
  --supersample 4 --samples 16 --sensor examples/closed_loop_ur5e/sensor_linear.json
etendue detect ds --corners radon        && cp ds/features.json radon.json
etendue detect ds --corners saddle_point && cp ds/features.json saddle.json
etendue calibrate ds --features radon.json
etendue calibrate ds --features saddle.json
echo '{"pixel_sigma_px": 0.0122, "seed": 1}' > wn.json && etendue calibrate ds --noise wn.json --trials 20
cargo test --release -p etendue-cli --test detect_targets   # with Corners::default_for returning SaddlePoint for every kind
```
