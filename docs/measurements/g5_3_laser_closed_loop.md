# G5.3 — laser closed loop (analytic correspondences)

- Gate: **G5.3** (`docs/pivot/PLAN.md` P5-3)
- Criterion, on noise-free input: laser plane normal error ≤ 0.01° and distance error ≤ 0.05 mm.
- Result:
  - **PASS on `examples/laser_ur5e`** (untilted sensors). The plane is recovered to 1.3e-7° and
    1e-8 mm in both cameras, 5–8 orders of magnitude inside the gate. Every G5.1 limit passes
    too.
  - **The Scheimpflug variant does not pass** (`examples/laser_scheimpflug_ur5e`, tilts of about
    3° and 2°). PLAN's G5.3 names no sensor model, so it is recorded here as a characterisation
    of calibration-rs. **Open question for the user:** should G5.3 also gate Scheimpflug rigs?
    If so, G5.3 does not pass yet. Seeded as
    calibration-rs's ADR 0022 prescribes, the plane normal is 0.0080° / 0.0102° off, and the
    principal point misses the G5.1 limits by 0.3–2 px. Unseeded, it is worse. The cause is a flat
    valley in calibration-rs's Scheimpflug model; the details are below.
- Scope: calibration-rs solves from etendue's **analytic ground truth**: board corners and
  laser-stripe points, with their exact pixels. There is no rendering, no detector and no laser
  line extraction in the loop.
- Measured: 2026-09-28, on etendue commit `e213946` plus the P5-3 change (branch
  `pivot/p5-3-laser-loop`), calibration-rs `76353753` (0.8.3), and the `vision-calibration`
  0.8.3 wheel from PyPI.
- Re-run 2026-10-17 after the review fixes (both laser examples now start the arm at shoulder pan
  π, as `closed_loop_ur5e` does since P5-1b): identical results, numbers below. The analytic
  loop ignores robot links, so the starting branch only matters once lasers are rendered.
- Re-run 2026-10-17 after merging P5-1b and P2-7 (the CLI pipeline as a library), on branch
  `pivot/p5-3-laser-loop` (merge commit `30468b3`), calibration-rs 0.8.3: identical
  results (plane normal 1.32e-7° / 1.21e-7°, distance 1e-8 mm; Scheimpflug variant 0.0080° /
  0.0102°).
- Re-run 2026-10-17 on calibration-rs 0.9.0: unchanged (`calibration_rs_0_9.md`).

## Procedure

```bash
cargo run -p etendue-cli -- gt examples/laser_ur5e/scene.json examples/laser_ur5e/scenario.json -o target/laser_loop
uv run --locked --project tools/closed-loop tools/closed-loop/closed_loop.py target/laser_loop
# The Scheimpflug characterisation (exits 1: it does not pass):
cargo run -p etendue-cli -- gt examples/laser_scheimpflug_ur5e/scene.json examples/laser_scheimpflug_ur5e/scenario.json -o target/laser_scheimpflug
uv run --locked --project tools/closed-loop tools/closed-loop/closed_loop.py target/laser_scheimpflug
```

CI job `checks` runs the first pair.

- **Scene** (`examples/laser_ur5e`): `examples/closed_loop_ur5e` plus a line laser on the rig.
  - Robot, rig mount, cameras and board are those of `closed_loop_ur5e`: UR5e; 2 cameras,
    1280 × 1024, fx 2318.8, ±40 mm; the rig 15°/10° off square on `tool0`; a 9 × 6 inner-corner
    chessboard with 20 mm squares.
  - The laser sits at the rig origin, 15 mm along the rig's y. It is turned 3° about y, so its
    fan plane nearly contains the rig's viewing axis and the stripe crosses the board in every
    view. Fan half-angle 20°, reach 1 m, 660 nm, waist 0.5 mm.
  - `examples/laser_scheimpflug_ur5e` is the same scene with Scheimpflug sensors: tilt
    (x, y) = (0.05, −0.01) rad and (0.035, 0.012) rad, i.e. about 2.9° and 2.0° about x.
- **Scenario:** the 20 P5-1 viewpoints (`tools/closed-loop/make_scenario.py --example …`), at
  0.45–0.55 m.
- **Ground truth:** `etendue gt` writes a `rig_handeye_laserline` dataset
  (`etendue-synth::dataset::emit`, `gt.json` version 2). The laser part comes from
  `etendue-synth::laser`:
  - The stripe is `etendue-core::laser::stripe_on_target`: the fan plane intersected with the
    board plane, clipped to the fan and to the board's printed extent. It is sampled every 1 mm,
    which gives 202–217 points per capture, in the target frame.
  - The stripe points are projected by the same `gt::project_points` as the corners, with the
    same visibility tests. Over 40 views that is 8,229 visible laser pixels.
  - The true plane is stored per camera and in the rig frame, as `n̂ · p + d = 0` in metres
    (calibration-rs's optimizer form, ADR 0006).
  - A test checks that every stripe point lies on the true camera-frame plane to 1e-12 m and
    re-projects to its pixel to 1e-9 px (`crates/etendue-synth/tests/dataset.rs`).
- **Solve:** `tools/closed-loop/closed_loop.py`:
  - Each view's visible corners and visible stripe pixels become a
    `RigHandeyeLaserlineView`.
  - It runs `vision_calibration.run_rig_handeye_laserline`: a rig hand-eye warm start, then a
    laser-plane init on the frozen geometry, then the joint bundle adjustment. It uses the
    default configuration, eye-in-hand.
  - It compares each camera's `laser_planes_cam` with the truth in that camera's frame, with the
    sign of `(n̂, d)` taken out. Camera frames are the same in etendue and calibration-rs, so
    this comparison needs no rig-frame convention.

## Result: `examples/laser_ur5e`

```text
rig hand-eye laserline (2 cameras, 20 views, 8229 laser pixels)
  mean reprojection px            1.721e-08  ≤ 0.05     PASS
  cam_left fx rel                 3.151e-11  ≤ 0.0001   PASS
  cam_left fy rel                 3.317e-11  ≤ 0.0001   PASS
  cam_left cx px                  5.368e-06  ≤ 0.05     PASS
  cam_left cy px                  1.664e-07  ≤ 0.05     PASS
  cam_left laser normal deg       1.320e-07  ≤ 0.01     PASS
  cam_left laser distance mm      9.751e-09  ≤ 0.05     PASS
  cam_right fx rel                8.966e-11  ≤ 0.0001   PASS
  cam_right fy rel                2.673e-11  ≤ 0.0001   PASS
  cam_right cx px                 4.908e-06  ≤ 0.05     PASS
  cam_right cy px                 5.439e-07  ≤ 0.05     PASS
  cam_right cam_se3_rig deg       2.519e-07
  cam_right cam_se3_rig mm        1.408e-07
  cam_right laser normal deg      1.209e-07  ≤ 0.01     PASS
  cam_right laser distance mm     9.743e-09  ≤ 0.05     PASS
  hand-eye rotation deg           1.321e-07  ≤ 0.01     PASS
  hand-eye translation mm         1.914e-08  ≤ 0.05     PASS
  base_se3_target rot deg         6.617e-10
  base_se3_target trans mm        8.307e-09
  G5.3: PASS
```

(Distortion rows omitted; every coefficient is within 1.4e-9 of the truth.)

## Characterisation: Scheimpflug sensors

Same loop, on `examples/laser_scheimpflug_ur5e`. All three runs are on noise-free data.

| | unseeded (default) | seeded, per-camera stage only | **seeded + intrinsics in rig BA** (the script's choice) |
|---|---|---|---|
| mean reprojection px | 3.4e-4 | 6.0e-2 (rig stage) | 6.8e-6 |
| tilt error, cam_left / cam_right (°) | 0.45 / 0.61 | 0.033 / 0.012 | 0.050 / 0.009 |
| cy error, cam_left / cam_right (px) | 18.1 / 24.7 | 1.39 / 0.97 | 2.05 / 0.37 |
| laser normal error, cam_left / cam_right (°) | 0.163 / 0.032 | — | **0.0080 / 0.0102** |
| laser distance error (mm) | 1.3e-4 / 8.2e-5 | — | 1.4e-6 / 3.9e-7 |
| hand-eye rotation error (°) | 0.47 | — | 0.051 |

Notes on the runs:
- **Unseeded:** `ScheimpflugSensorMode(refine_scheimpflug_in_rig_ba=True)`, the default
  otherwise. The result does not change with `init_tilt_x/y` (true, negated or halved) or with
  50 vs 500 solver iterations. The tilt from `init_tilt_*` is only one candidate in the linear
  initialiser's tilt grid.
- **Seeded:** `handeye.manual_init` holds what a user would enter from datasheets. That is fx
  rounded to 100 px (2300), the principal point at the image centre, and the mount tilt about x
  rounded to a whole degree (3° and 2°), with no tilt about y. Distortion is left to the
  auto-fit.
- **The wheel does not expose `manual_init`** (calibration-rs 0.8.3 says "still Rust-only").
  The Rust config it is deserialised into accepts the field, so `closed_loop.py` adds it to the
  payload (`SeededLaserlineConfig`).

What the table shows:

- **Unseeded init is in the wrong basin**, as calibration-rs's ADR 0022 says it can be
  ("from-scratch auto-init is experimental").
  - On the single-camera `run_scheimpflug_intrinsics` path, it is worse: 0.094 / 0.131 px residual,
    tilt 4.7° / 6.4° off and cy 226 / 306 px off.
- **Seeded, the solve stops in a flat valley.** The residual is 6.8e-6 px, yet tilt is 0.05° off
  and cy 2 px off.
  - Changing the sensor tilt looks almost the same as rotating the camera and shifting its
    principal point.
  - Each view's pose and the rig/hand-eye rotation take up the difference. The hand-eye rotation
    is off by 0.051°, while `base_se3_target` stays within 1.6e-7°.
  - The joint laserline stage does not leave the warm start. Its result is the same with the
    tilt fixed or free.
  - Because the camera frame is rotated, the laser plane is off by the same order in camera
    coordinates: 0.008–0.010°.
- **The laser plane's distance is exact in every run** (≤ 1.3e-4 mm). The error is in the
  camera's orientation, not in the laser geometry.

## Findings

1. **etendue's laser ground truth is exact.** The pinhole run recovers the plane to 1e-7°, and
   the laser GT uses the same frames and projection as the G5.1 ground truth.
2. **calibration-rs, Scheimpflug rigs: flat tilt / principal-point valley.** Found on noise-free
   data. The solver stops 0.05° / 2 px from the truth at 6.8e-6 px residual.
   - A draft issue for calibration-rs is prepared with this reproduction, for the user's review
     (not filed).
3. **The Python wheel lacks `manual_init`**, which is calibration-rs's own supported
   Scheimpflug route (ADR 0022). A draft request to expose it is prepared for the user's review
   (not filed). Until then, `closed_loop.py` passes it in the payload.

## Other gates re-run

`etendue-synth` now depends on `etendue-core`, with `etendue-core`'s argmin solver behind a
`solver` feature that `etendue-synth` does not enable (ADR 0001, amended). On the P5-3 branch:

- **G0.1:** 0 bit mismatches.
- **G2.1:** 319,191 B gzipped (was 319,533 B).
- The wasm32 graph has no `getrandom`.
- **G5.1:** unchanged on both examples (1.841e-8 px and 4.332e-7 px mean reprojection).
