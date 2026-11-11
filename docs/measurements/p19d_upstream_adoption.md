# P19d: what calibration-rs #256, #261 and #268 change for etendue

Measured 2026-11-11, Apple M4 Pro, etendue branch `p19d/joint-seeds`, calibration-rs `v0.18.0`
(`865e86d`, a clean clone). Noise: 0.5 px on the target corners and the laser stripe pixels,
seeds 500–519 (20 trials), for the rigs; 0.1 px, seeds 700–719, for the head.

## #256: the joint laser solve seeded from a rig hand-eye result: adopted as seeds, no built-in change

- **What etendue can now do.** A `rig_handeye_laserline` stage's `rig` seed takes an earlier
  `rig_handeye` stage's result as a whole, and a new `planes` seed takes the design planes or an
  earlier stage's.
- **The procedure compared:** intrinsics → rig → rig hand-eye → joint, the joint stage seeded
  from the rig hand-eye result and the design planes ("staged"). Against it, the `laser_rig`
  built-in: one joint solve from datasheet focal lengths and tilts.

| Example | Procedure | triangulated rms (worst camera, mean) | laser normal (worst, mean) | hand-eye (mean) | std / σ |
|---|---|---|---|---|---|
| laser_ur5e | laser_rig | 0.0797 mm | 0.180° | 0.943 mm | 0.28–1.07 |
| laser_ur5e | staged | 0.0797 mm | 0.180° | 0.943 mm | 0.28–1.06 |
| laser_scheimpflug_ur5e | laser_rig | 0.0664 mm | 0.815° | 0.949 mm | 0.47–1.12 |
| laser_scheimpflug_ur5e | staged | 0.0665 mm | 0.820° | 0.949 mm | 0.54–1.34 |
| rig3_laser_ur5e | laser_rig | 0.0363 mm | 0.071° | 0.151 mm | 0.60–3.22 |
| rig3_laser_ur5e | staged | 0.0362 mm | 0.072° | 0.151 mm | 0.60–3.24 |

- Both reach the same minimum on every example, and both pass the closed-loop check on exact data.
- The staged procedure is not faster overall (243 s against 267 s, 231 s against 212 s, 1470 s
  against 1097 s for 20 trials).
- On simulated data, starting the joint solve from a rig hand-eye result buys nothing over
  datasheet values. The built-ins stay as they are. The seeds are for cells where a rig hand-eye
  result already exists, and for real data (P20), where datasheet values may start the joint
  solve in the wrong basin.
- **A finding, not a gate:** on the laser rigs, the spread over trials is 0.3–3.2 times
  calibration-rs's predicted σ (same with either procedure). G17.1 (±15 %) covers the camera rig,
  not the laser rigs. This belongs to P20's twin (T2) and to calibration-rs.

## #268: one camera through the joint laser solve: not adopted

A one-camera head (`laser_ur5e`'s `cam_left` carrying its laser, untilted), as in
`docs/measurements/head_triangulation.md`:

| Procedure | exact: triangulated max | 0.1 px: triangulated rms (mean, worst) | laser normal (mean) | hand-eye (mean) |
|---|---|---|---|---|
| auto (`auto_head`: device, then hand-eye with the camera held) | 1.3e-4 mm | 0.0161 mm, 0.0239 mm | 0.016° | 0.205 mm |
| `laser_head` built-in | 1.3e-4 mm | 0.0161 mm, 0.0239 mm | 0.016° | 0.205 mm |
| one joint solve (`rig_handeye_laserline`, one camera) | 1.2e-7 mm | 0.0164 mm, 0.0350 mm | 0.029° | 0.181 mm |

- The joint solve is a thousand times closer on exact data. That is the two-step route's
  convergence floor, not a difference a measurement would see.
- Under noise it is no better: the same rms, a worse worst case and plane, a slightly better
  hand-eye.
- `auto` and the built-ins keep the two-step route. A one-camera joint stage is a valid
  procedure for anyone who wants it.

## #261: next-view prediction for rig hand-eye: not adopted in the planner

The P18 planner runs in wasm (the studio's Suggest poses), and calibration-rs's prediction
(`analysis::predict_with_robot_poses`) is in `vision-calibration-pipeline`, which the wasm build
does not take: the pipeline brings the detectors and their nalgebra 0.35. The planner keeps its
own information model. G18's check of it against calibration-rs's covariance of a real solve
(34 parameters within 0.93–1.14) is unchanged on 0.18.

## Procedure

```bash
etendue calibrate ds/<laser example> --procedure laser_rig --noise n500.json --trials 20 -o a.json
etendue calibrate ds/<laser example> --procedure staged.json --noise n500.json --trials 20 -o b.json
cargo test -p etendue-cli --test calibrate head   # makes target/calibrate_test/head/gt
etendue calibrate target/calibrate_test/head/gt [--procedure laser_head | joint1.json] --noise n700.json --trials 20
```

`staged.json`:

```json
{ "version": 2, "id": "laser_rig_staged", "name": "Rig with lasers, staged",
  "procedure": { "stages": [
    { "id": "intrinsics", "problem": "planar_intrinsics" },
    { "id": "rig", "problem": "rig_extrinsics", "seed": { "intrinsics": { "stage": "intrinsics" } } },
    { "id": "handeye", "problem": "rig_handeye", "seed": { "intrinsics": { "stage": "rig" }, "rig": { "stage": "rig" } } },
    { "id": "joint", "problem": "rig_handeye_laserline", "seed": { "rig": { "stage": "handeye" }, "planes": "nominal" } } ] } }
```

`joint1.json`: one stage, `rig_handeye_laserline`, with `"seed": { "sensor": "nominal" }`.
