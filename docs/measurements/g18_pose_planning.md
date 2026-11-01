# G18.1: planned calibration poses

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

## G18.2 (eye-to-hand): not yet

The sphere generator still refuses eye-to-hand cells (the target on the robot), and so does the planner. That is the next step of P18.

## Procedure

```bash
cargo test --release -p etendue-cli --test g18_plan -- --ignored --nocapture   # ETENDUE_G18_TRIALS=100
etendue scenario plan examples/closed_loop_ur5e/scene.json --spec examples/closed_loop_ur5e/scenario.json \
    --budget 8 -o plan.json --report plan_report.json
```
