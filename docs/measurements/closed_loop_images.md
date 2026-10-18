# P5-1b: calibration closed loop on rendered images

> Re-run 2026-10-18 on the 24 generated viewpoints of `examples/closed_loop_ur5e` (P7-6) with
> Blender 5.2.2: see [Re-run on Blender 5.2](#re-run-on-blender-52-2026-10-18). The sections
> after it record the first run, on the former 20-viewpoint scenario with Blender 5.1.1.

- Measures: what the rendered dataset, detected with the G4.2 default detector, gives
  calibration-rs. The pipeline is render → detect → calibrate (`docs/pivot/PLAN.md` P5-1).
- Reference limits: **G5.1**'s (mean reprojection ≤ 0.05 px, fx/fy ≤ 1e-4 relative, cx/cy ≤
  0.05 px, hand-eye ≤ 0.01° / 0.05 mm). G5.1 is defined on noise-free input and passes there
  (`closed_loop.md`). This run is **not** a G5.1 re-run: a detector adds noise, and the limits
  are reported against, not applied as, a gate.
- Result: **the loop closes, and it is limited by noise, not bias.**
  - The detector finds the board in **40/40 views** (2160 corners). Every label agrees with the
    analytic ground truth: 0 mislabelled, 0 unmatched.
  - Detected corners are **0.051 px RMS** from their analytic pixels (max 0.14 px).
  - Mean reprojection is **0.044 px**, inside its limit.
  - Intrinsics and hand-eye are **outside the noise-free limits**. The worst errors are cy
    0.56 px, fx 2.1e-4 and hand-eye 0.014° / 0.12 mm, all for the rig.
  - The same analytic points with **white noise of the same RMS** and no bias give errors of
    the same size, over 12 seeds. The detected result sits inside that spread for every
    quantity. On these 20 views a 0.05 px corner floor alone costs about that much. Neither the
    renderer nor the detector adds a measurable bias.
- **Decision needed**: which limits, if any, should gate the image-level loop. See the end of
  this file.
- Measured: 2026-09-29, on etendue commit `dc2b3ff` (branch `pivot/p5-1b-image-loop`),
  `vision-calibration` 0.8.3 wheel, calibration-rs 0.8.3, calib-targets 0.15.3,
  chess-corners 1.2.0, Blender 5.1.1 (Cycles, Metal, Apple M4 Pro).
- Changed after this run (PR #13 review): `etendue detect` decides the half-turn by comparing
  the two square classes' mean intensities instead of a global midpoint threshold, which
  survives an illumination gradient. It also reads `render`'s default sRGB PNGs, as linear
  luminance, and rounds 16-bit images to 8 bits instead of truncating. On this run's input
  (8-bit linear, uniform light, a full board in every view) the decision is not expected to
  change. The 2026-10-18 re-run uses it.

## Re-run on Blender 5.2 (2026-10-18)

- Measured: 2026-10-18, on etendue commit `95f1947` (branch `blender-5.2`), Blender 5.2.2
  (Cycles, Metal, Apple M4 Pro), `vision-calibration` wheel and calibration-rs 0.9.0,
  calib-targets 0.15.5, chess-corners 1.2. The procedure below, on the example's current
  scenario: 24 viewpoints from the sphere pose generator, 48 images. Ground truth, render and
  detection took 2 min 35 s.
- **Detection:** every view of both cameras found, 2592 corners, **0 mislabelled**, 0 unmatched.
  0.0578 px (cam_left) and 0.0582 px (cam_right) RMS from the analytic pixels, max 0.137 px.
- **Calibration**, against white noise of the same RMS (σ = 0.041 px per axis), min / median /
  max over 12 seeds:

| quantity | limit (G5.1) | rig, detected | white noise min / median / max |
|---|---|---|---|
| mean reprojection px | 0.05 | 0.0521 | 0.0502 / 0.0505 / 0.0516 |
| cam_left fx rel | 1e-4 | 1.9e-4 | 2.6e-5 / 1.3e-4 / 5.2e-4 |
| cam_left cx px | 0.05 | 0.28 | 0.099 / 0.61 / 0.91 |
| cam_left cy px | 0.05 | 0.017 | 0.086 / 0.34 / 0.78 |
| cam_right fx rel | 1e-4 | 1.2e-4 | 1.6e-5 / 1.8e-4 / 3.8e-4 |
| cam_right cx px | 0.05 | 0.13 | 0.036 / 0.31 / 1.12 |
| cam_right cy px | 0.05 | 0.027 | 0.0078 / 0.26 / 0.64 |
| hand-eye rotation deg | 0.01 | 0.012 | 0.0048 / 0.020 / 0.029 |
| hand-eye translation mm | 0.05 | 0.14 | 0.013 / 0.078 / 0.30 |

  One camera alone (cam_left): mean reprojection 0.051 px, fx 2.2e-4, cx 0.37 px, cy 0.036 px,
  hand-eye 0.012° / 0.14 mm.
- **Still noise-limited, as on 5.1.1.** Every intrinsic and hand-eye quantity lies within the
  white-noise spread, except cam_left cy, which is better than every seed.
- **The mean reprojection error is 0.0005 px above the white-noise max** (0.0521 against
  0.0516). The detector's error is not quite white: it depends on each corner's sub-pixel phase
  and perspective (G4.2), so it is correlated across views in a way independent noise is not.
  This is the one place the detected corners differ measurably from white noise of the same
  RMS. Option 2 below would fail on it by that margin; the rest of option 2 holds.

## Procedure

```bash
cargo run --release -p etendue-cli -- gt examples/closed_loop_ur5e/scene.json examples/closed_loop_ur5e/scenario.json -o target/p5_1b
cargo run --release -p etendue-cli -- render examples/closed_loop_ur5e/scene.json examples/closed_loop_ur5e/scenario.json -o target/p5_1b \
  --supersample 4 --samples 16 --sensor examples/closed_loop_ur5e/sensor_linear.json
cargo run --release -p etendue-cli -- detect target/p5_1b
uv run --locked --project tools/closed-loop tools/closed-loop/closed_loop.py target/p5_1b --features target/p5_1b/features.json                   # rig
uv run --locked --project tools/closed-loop tools/closed-loop/closed_loop.py target/p5_1b --features target/p5_1b/features.json --camera cam_left  # 1 camera
uv run --locked --project tools/closed-loop tools/closed-loop/closed_loop.py target/p5_1b --noise-px 0.036 --seed <k>                            # white-noise reference
```

The render takes 2 min 14 s for 40 images: 5120 × 4096 canonical pixels each, 16 samples. The
EXRs take 4.5 GB.

- **Scene and scenario:** `examples/closed_loop_ur5e`, as in `closed_loop.md`: a UR5e, a
  two-camera rig on `tool0`, a 9 × 6 chessboard of 20 mm squares, and 20 viewpoints at
  0.45–0.55 m.
- **Rendering:** the G4.2 defaults.
  - Supersampling s = 4, box-filtered through the remap LUT.
  - Linear output: `sensor_linear.json` is an 8-bit sensor with radiance 1 at 240 DN, as G4.2
    scaled it. Its shot noise is about 0.008 DN, and it has no other noise.
  - Uniform white environment (`--ambient 1`), 16 Cycles samples per canonical pixel. That is
    256 per camera pixel, as in G4.2.
- **Detection:** `etendue detect`.
  - Corners come from chess-corners' `DetectorConfig::radon()`.
  - They are labelled by calib-targets' chessboard detector (`ChessboardParams::default()`).
  - A view is kept when its labels span the whole board. The half-turn ambiguity is settled by
    the colour of the squares between the corners, read from the print: the darker of the two
    square classes (alternate squares) is the ink.
  - Every label is then checked against the analytic ground truth: the corner's nearest
    analytic pixel within 1.5 px must be its own point's. The check reports; it never corrects.
- **Calibration:** `tools/closed-loop/closed_loop.py --features`, which runs calibration-rs's
  solver through the `vision-calibration` wheel. Only the source of the observations changes
  from the analytic loop.
  - It does not go through calibration-rs's `dataset_runner`, as P5-1 first proposed. That
    runner's detector config (`DetectorSpec`) offers only ChESS with its default refiner (about
    0.18 px in G4.2), and no Radon.

## Results

**Detection** (`features.json`):

| camera | views ok | corners | mislabelled | unmatched | RMS vs GT px | max px |
|---|---|---|---|---|---|---|
| cam_left | 20/20 | 1080 | 0 | 0 | 0.0505 | 0.142 |
| cam_right | 20/20 | 1080 | 0 | 0 | 0.0512 | 0.132 |

The detection error is 0.051 px here and 0.032 px in G4.2, on different poses. Here the RMS
per view ranges from 0.030 to 0.072 px. The difference is not isolated further.

**Calibration**, detected corners against the white-noise reference. The reference is the
analytic pixels plus Gaussian noise of σ = 0.036 px per axis (0.051 px RMS), median and max
over 12 seeds:

| quantity | limit (G5.1) | rig, detected | rig, white noise median / max | cam_left, detected | cam_left, white noise median / max |
|---|---|---|---|---|---|
| mean reprojection px | 0.05 | **0.044** | 0.044 / 0.045 | **0.043** | 0.044 / 0.045 |
| cam_left fx rel | 1e-4 | 2.1e-4 | 9.5e-5 / 4.0e-4 | 6.0e-5 | 5.9e-5 / 3.8e-4 |
| cam_left cx px | 0.05 | 0.090 | 0.24 / 0.64 | 0.090 | 0.23 / 0.57 |
| cam_left cy px | 0.05 | 0.56 | 0.44 / 0.68 | 0.39 | 0.27 / 0.77 |
| cam_right fx rel | 1e-4 | 8.8e-5 | 1.7e-4 / 2.8e-4 | | |
| cam_right cx px | 0.05 | 0.25 | 0.22 / 0.76 | | |
| cam_right cy px | 0.05 | 0.12 | 0.17 / 0.78 | | |
| hand-eye rotation deg | 0.01 | 0.014 | 0.012 / 0.024 | 0.0095 | 0.012 / 0.022 |
| hand-eye translation mm | 0.05 | 0.12 | 0.057 / 0.18 | 0.040 | 0.043 / 0.18 |

What the table shows:
- **The mean reprojection error is the corner noise.** The mean length of a 2-D Gaussian error
  of 0.051 px RMS is 0.051 · √(π/4) = 0.045 px. It is the same with detected and with
  white-noise corners.
- **The rest is what 0.05 px of noise costs on these 20 views.** Each detected value is inside
  the white-noise spread. The principal point is the weakest quantity (up to about 0.7 px): the
  views tilt only 8–25° off the board normal, which constrains cx/cy poorly.
- **No bias shows.** A systematic detector or renderer error would move the detected result
  outside the white-noise spread, and it does not.

## Findings

- **The IK picked a branch in which the arm hides the board.** The first render of the scenario
  used `initial_q` at shoulder pan 0. From there the IK put captures 0–4 in an arm-over branch
  (pan ≈ 30°, shoulder lift ≈ −170°) whose upper arm and forearm fill the view. The detector
  found no board in 3 of those views and part of it in 2. It then jumped to the pan ≈ 180°
  branch at capture 5.
  - `examples/closed_loop_ur5e/scene.json` now starts at pan 180°, and all 20 captures stay in
    that branch.
  - The tool poses are unchanged, so the ground truth is too: pixels move by ≤ 1.7e-7 px (IK
    tolerance), and the analytic G5.1 results are identical.
- **The analytic ground truth has no robot self-occlusion.** `etendue gt` marked every corner of
  those five views visible. ADR 0006 takes render occlusion from the Blender object-index pass
  (the photometric tier), which is not built yet. Until it is, `etendue detect`'s check against
  the ground truth is what shows such views.
- **calibration-rs's runner** (for an upstream issue; not filed):
  - it has no Radon option in `DetectorSpec`;
  - it keeps chessboard corners only while `grid.u < rows` (`vision-calibration-detect`
    `chessboard.rs`), which drops corners of a landscape board whose u runs along the columns.

## Decision needed

These are the options for the image-level loop:
1. **Keep it as a characterisation** (like G4.2): record the numbers, no gate.
2. **Gate it against the white-noise reference**: pass when every quantity is within the
   white-noise max over N seeds. That tests "no bias" directly. It held on the 2026-09-29 run;
   on the 2026-10-18 run every intrinsic and hand-eye quantity passes, and the mean
   reprojection error misses by 0.0005 px.
3. **Gate it against G5.1's limits with a stronger view set**: more views and more tilt, until
   0.05 px noise costs less than the limits. That tests the dataset design as well as the
   pipeline.
