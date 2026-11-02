# G18.1 and G18.2: planned calibration poses

Gate of P18 (`docs/pivot/PLAN.md`; the objective was a user decision on 2026-11-01).

- Measured 2026-11-02, Apple M4 Pro.
- etendue commit `365ff87` (branch `p18a/pose-planner`).
- calibration-rs `v0.11.0` (`c1880f64`), a clean clone.

## What is predicted

**The system σ.** The planner (`etendue-synth::plan`, `etendue scenario plan`) predicts it as follows:
- **The quantity:** how far, in mm, the calibrated cell would place a point it knows in the robot's base frame. It is measured across the line of sight, per image axis.
- **What it averages over:** every sighting of the target's points by every feasible candidate of the sphere generator.
- **The calibration it models:** the rig hand-eye with the rig refined (`refine_rig: true`) and exact robot poses, as G17.1 found necessary for a complete covariance.
- **The information:** calibration-rs's projection, differentiated numerically, over each camera's intrinsics, distortion (and tilt), the rig, the hand-eye and the target.

**Check of the information model.** On the 8 planned poses with 0.5 px noise:
- The planner's predicted σ of all 34 parameters match calibration-rs's own covariance of a calibration of those poses, within 0.89–1.10 (most within ±5 %).
- So does each camera's `fx`…`p2`, the rig, the hand-eye and the target.

## G18.1: PASS

**Pose sets compared:**
- **Planned:** the 8 the planner picks.
- **Grid:** the generator's 24 feasible poses thinned evenly to 8, in tour order.

**Measured values:** 100 calibrations of each set, with 0.5 px noise and seeds 1000–1099, using the planner's model of the calibration. Each estimate's system error comes from `system_error`, and the table gives the RMS over the trials.

| Pose set | Predicted system σ | Measured system error | Measured / predicted |
|---|---|---|---|
| planned (8) | 0.0362 mm | 0.0377 mm | 1.04 |
| grid (8) | 0.0462 mm | 0.0461 mm | 1.00 |

- **The criterion holds.** The planned poses predict, and achieve, a smaller system σ than the grid of the same size (−22 % predicted, −18 % measured), and the prediction holds to 4 % (the limit is 15 %).
- **Other budgets (predicted):**
  - 6 poses: 0.0439 against 0.0697 mm.
  - 12 poses: 0.0282 against 0.0348 mm.
  - 16 poses: 0.0255 against 0.0297 mm.
  - Every feasible pose (24): 0.0243 mm.
- **A normalisation bug was found here.** The first run measured 0.72× the prediction for both sets. It was a per-point (both axes) mean on the predicted side against a per-axis mean on the measured side. Both are now per axis.

## G18.2 (eye-to-hand): PASS

Measured 2026-11-02, Apple M4 Pro, branch `p18b/eye-to-hand` (commit in the PR), calibration-rs
`v0.11.0`, a clean clone.

**The feasible tour.** `examples/eye_to_hand_ur5e` (a static stereo rig on a stand, the board on
the UR5e's flange) with its generator spec `sphere.json`: radii 0.6 and 0.7 m, θ 0.14 and
0.44 rad, 6 azimuths, rolls ±0.35 rad.
- All 48 candidates are feasible: reachable, with joint margin, away from singularities, and
  both cameras see all 54 corners.
- The tour bakes, and its ground truth (`etendue gt`) sees the whole board in both cameras at
  every capture.
- Exact data calibrates: the hand-eye to 5.5e-6° and 9.8e-5 mm.
- Radii of 0.5 m lose up to 3 corners at the image edge, so the spec starts at 0.6 m.

**The planner, eye-to-hand.** The hand-eye is now `rig_se3_base` and the target's pose
`gripper_se3_target`: every view's chain is `cam_se3_rig · rig_se3_base · base_se3_tcp ·
gripper_se3_target`. The G18.1 check, run on this cell (100 calibrations per set, 0.5 px, seeds
1000–1099):

| Pose set | Predicted system σ | Measured system error | Measured / predicted |
|---|---|---|---|
| planned (8) | 0.0407 mm | 0.0428 mm | 1.05 |
| grid (8) | 0.0499 mm | 0.0485 mm | 0.97 |

- The planned poses beat the grid (−18 % predicted, −12 % measured), and the prediction holds to
  5 %.
- The planner's σ of all 34 parameters match calibration-rs's covariance within 0.93–1.14.
- On eye-in-hand, the planner's σ match within 0.94–1.10, and G18.1 is unchanged (0.0377 mm, 1.04).

**Ties.** Mirror-symmetric candidates (rolls ±0.35, symmetric azimuths) give system σ equal to
about 1e-9 relative. The greedy pick now keeps the earlier candidate unless a later one is better
by 1e-7 relative, so rounding (a reordered product, another platform's libm) does not swap a pick
for its mirror. Before that rule, a reordered product picked the mirror set on
`closed_loop_ur5e`; it measured 0.0393 mm (1.08), within the gate but a different noise draw.

## Procedure

```bash
cargo test --release -p etendue-cli --test g18_plan -- --include-ignored --nocapture   # ETENDUE_G18_TRIALS=100
etendue scenario plan examples/closed_loop_ur5e/scene.json --spec examples/closed_loop_ur5e/scenario.json \
    --budget 8 -o plan.json --report plan_report.json
etendue scenario plan examples/eye_to_hand_ur5e/scene.json --spec examples/eye_to_hand_ur5e/sphere.json \
    --budget 8 -o plan.json --report plan_report.json
```
