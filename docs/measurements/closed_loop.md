# G5.1 — calibration closed loop (analytic correspondences)

- Gate: **G5.1** (`docs/pivot/PLAN.md` P5-1)
- Criterion, on noise-free input at a working distance of about 0.5 m:
  - mean reprojection ≤ 0.05 px
  - fx/fy relative error ≤ 1e-4
  - cx/cy ≤ 0.05 px
  - hand-eye rotation ≤ 0.01° and translation ≤ 0.05 mm
- Result: **PASS for one camera and for the two-camera rig, on analytic correspondences.** Every
  gated quantity is 5–8 orders of magnitude inside its limit: mean reprojection 1.8e-8 px, fx
  1e-10, hand-eye 1.4e-7° / 7e-8 mm.
- Scope: calibration-rs solves from etendue's **analytic ground truth** (target points and
  their exact pixels). There is no rendering and no detector in the loop. This is the baseline
  the PLAN asks for, and it validates every frame and pose convention between the two
  projects.
  - The image-level loop (render → detect → calibrate through calibration-rs's
    `dataset_runner`) inherits the detector's 0.07–0.19 px per-corner error
    (`g4_2_corner_bias.md`). It waits on the G4.2 decision.
- Measured: 2026-09-27, on etendue commit `f8e7ac1` (branch `pivot/p5-closed-loop`), with the
  `vision-calibration` 0.8.2 wheel from PyPI.

## Procedure

```bash
cargo run -p etendue-cli -- gt examples/closed_loop_ur5e/scene.json examples/closed_loop_ur5e/scenario.json -o target/closed_loop
uv run --locked --project tools/closed-loop tools/closed-loop/closed_loop.py target/closed_loop                   # rig
uv run --locked --project tools/closed-loop tools/closed-loop/closed_loop.py target/closed_loop --camera cam_left  # 1 camera
```

CI job `checks` runs both.

- **Scene** (`examples/closed_loop_ur5e`):
  - Robot: UR5e.
  - Rig: two cameras (1280 × 1024, fx 2318.8, Brown–Conrady k1 −0.08 / −0.07, k2 0.02, ±40 mm
    apart). The rig is on `tool0`, mounted 15° about x and 10° about z off square, 30 mm / 50 mm
    offset.
  - Board: a 9 × 6 inner-corner chessboard with 20 mm squares, at (0.42, 0.10, 0) in the base
    frame.
- **Scenario:** 20 viewpoints from `tools/closed-loop/make_scenario.py` (deterministic).
  - The rig looks at the board centre from 0.45–0.55 m.
  - Tilt is 8–25° from vertical, azimuths are golden-angle spaced, and roll is within ±25°.
  - Each viewpoint is a PTP move, baked by `etendue-kinematics` (IK, trapezoidal profiles).
- **Ground truth:** `etendue gt` writes `dataset.json`, `robot_poses.json` and `gt.json`
  (`etendue-synth::dataset::emit`). The board points use the interim etendue layout
  (`gt::board_points`, P3-3 pending). 2160 corners are visible over 40 views; the border
  margin is 2 px.
- **Solve:** `tools/closed-loop/closed_loop.py` does the following.
  - It reads the robot poses from `robot_poses.json` through the dataset's own column map
    and pose convention.
  - Each view's visible ground-truth corners become an `Observation` (target point, pixel).
  - It runs `vision_calibration.run_rig_handeye` (or `run_single_cam_handeye`) with default
    configuration, eye-in-hand.
  - It compares the estimate with `gt.json`, re-expressed in calibration-rs's rig frame (the
    reference camera's).

## Result

```text
target/closed_loop: 20 captures, cameras ['cam_left', 'cam_right'], 2160 visible points
rig hand-eye (2 cameras, 20 views)
  mean reprojection px            1.841e-08  ≤ 0.05     PASS
  cam_left fx rel                 1.252e-10  ≤ 0.0001   PASS
  cam_left fy rel                 6.216e-11  ≤ 0.0001   PASS
  cam_left cx px                  5.540e-06  ≤ 0.05     PASS
  cam_left cy px                  1.686e-07  ≤ 0.05     PASS
  cam_left k1 abs                 1.345e-10             
  cam_left k2 abs                 9.705e-10             
  cam_left k3 abs                 0.000e+00             
  cam_left p1 abs                 1.960e-11             
  cam_left p2 abs                 5.766e-10             
  cam_right fx rel                4.337e-10  ≤ 0.0001   PASS
  cam_right fy rel                3.649e-10  ≤ 0.0001   PASS
  cam_right cx px                 5.238e-06  ≤ 0.05     PASS
  cam_right cy px                 5.572e-07  ≤ 0.05     PASS
  cam_right k1 abs                2.781e-10             
  cam_right k2 abs                2.277e-09             
  cam_right k3 abs                0.000e+00             
  cam_right p1 abs                6.456e-11             
  cam_right p2 abs                5.666e-10             
  cam_right cam_se3_rig deg       2.609e-07             
  cam_right cam_se3_rig mm        6.131e-08             
  hand-eye rotation deg           1.350e-07  ≤ 0.01     PASS
  hand-eye translation mm         7.188e-08  ≤ 0.05     PASS
  base_se3_target rot deg         2.398e-10             
  base_se3_target trans mm        1.247e-08             
  G5.1: PASS
```

```text
target/closed_loop: 20 captures, cameras ['cam_left', 'cam_right'], 2160 visible points
single-camera hand-eye (cam_left, 20 views)
  mean reprojection px            1.648e-08  ≤ 0.05     PASS
  cam_left fx rel                 9.829e-11  ≤ 0.0001   PASS
  cam_left fy rel                 3.022e-11  ≤ 0.0001   PASS
  cam_left cx px                  5.119e-06  ≤ 0.05     PASS
  cam_left cy px                  1.536e-07  ≤ 0.05     PASS
  cam_left k1 abs                 2.175e-10             
  cam_left k2 abs                 2.525e-09             
  cam_left k3 abs                 0.000e+00             
  cam_left p1 abs                 1.655e-11             
  cam_left p2 abs                 5.518e-10             
  hand-eye rotation deg           1.233e-07  ≤ 0.01     PASS
  hand-eye translation mm         6.191e-08  ≤ 0.05     PASS
  base_se3_target rot deg         3.050e-09             
  base_se3_target trans mm        2.234e-08             
  G5.1: PASS
```

k3 is not estimated by the default configuration (it stays 0, as does the truth).

## Findings

1. **etendue and calibration-rs agree on every convention.** That covers `a_se3_b` naming,
   the robot pose stream (`t_base_tcp`, `quat_xyzw`, m), CV camera axes, the target frame and
   rig frames. Any mismatch would show up as a gross error, not a 1e-8 px residual.
2. **calibration-rs bug found: hand-eye initialisation rejected identity-rotation mounts**
   ([calibration-rs#124](https://github.com/VitalyVorobyev/calibration-rs/issues/124)).
   `examples/eye_in_hand_ur5e` mounts its rig square to the flange. Every motion pair's robot
   and camera rotation axes are then parallel, and the Tsai–Lenz initialiser dropped all of
   them (`NoValidMotionPairs`). The fix is calibration-rs#125. This scene's mount is
   deliberately off square, which is also the more general case.

   *Re-run with calibration-rs 0.8.3 (which ships #125), 2026-09-28, etendue commit
   `2199784`:* `examples/eye_in_hand_ur5e` now passes too. Over 10 views, the rig gives
   4.3e-7 px mean reprojection and hand-eye 2.5e-6° / 1.9e-7 mm; `cam_left` alone gives
   3.8e-7 px, fx 1.7e-9, and 2.3e-6° / 5.2e-7 mm. It is smaller than this scene's 20 views and
   has the square mount, so CI now runs it as well.
