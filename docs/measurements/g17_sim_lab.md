# G17.1, G17.2: the simulation report

Gates of P17 (`docs/pivot/PLAN.md`). Measured 2026-11-02, Apple M4 Pro, on etendue branch
`p17a/sim-report` (commit `dd423de`), calibration-rs `v0.11.0` (`c1880f64`, a clean clone).

## G17.1: the spread over trials matches calibration-rs's predicted σ: PASS (with the rig refined)

**Setup**
- Dataset: closed_loop_ur5e (24 views, 2 cameras), `pixel_sigma_px` 0.5, seeds 100–299 (200 trials).
- For each trial, `param_stats` compares the estimate's spread with the mean predicted σ.
- The predicted σ is the diagonal of calibration-rs's covariance.
- The hand-eye error is expressed in calibration-rs's tangent: `estimate = truth · Exp(δ)`.
  - Rotation: `log(R_trueᵀ R)`.
  - Translation: `R_trueᵀ (t − t_true)`, in the hand-eye's own frame.

On 20 trials, the global translation error and the left perturbation did not match (`target/g17/conv.py`). This convention did.

**Result**
- **With the rig refined in the hand-eye solve, every ratio is within 0.92–1.04 (limit: within 15 %).** The procedure is one `rig_handeye` stage with `"free": {"refine_rig": true}`.
- **With calibration-rs's defaults, the predicted σ is too small, by up to 2.4×.** Its covariance is *conditional on the parameters held fixed*, and by default the rig hand-eye solve holds the cameras' rig poses (`refine_cam_se3_rig: false`). Its `fixed` list says so, and calibration-rs's own documentation of `RigConfig.refine_intrinsics` warns of the same effect.
- **This is a finding, not a gate change.** The predicted σ of the default workflow understates the real uncertainty of the rig. The book's Calibration page (Reading the report) says this, and how to get a complete covariance.

**Rig refined (`refine_rig: true`)**

| Parameter | Unit | std | predicted σ | std / σ |
|---|---|---|---|---|
| cam_left fx | px | 4.979e0 | 5.066e0 | 0.98 |
| cam_left fy | px | 4.990e0 | 5.058e0 | 0.99 |
| cam_left cx | px | 6.563e0 | 6.497e0 | 1.01 |
| cam_left cy | px | 4.646e0 | 4.467e0 | 1.04 |
| cam_left k1 |  | 1.808e-2 | 1.751e-2 | 1.03 |
| cam_left k2 |  | 2.157e-1 | 2.172e-1 | 0.99 |
| cam_left p1 |  | 5.147e-4 | 5.005e-4 | 1.03 |
| cam_left p2 |  | 9.841e-4 | 9.807e-4 | 1.00 |
| cam_right fx | px | 4.633e0 | 5.061e0 | 0.92 |
| cam_right fy | px | 4.630e0 | 5.053e0 | 0.92 |
| cam_right cx | px | 6.093e0 | 6.489e0 | 0.94 |
| cam_right cy | px | 4.568e0 | 4.464e0 | 1.02 |
| cam_right k1 |  | 1.731e-2 | 1.751e-2 | 0.99 |
| cam_right k2 |  | 2.104e-1 | 2.178e-1 | 0.97 |
| cam_right p1 |  | 4.848e-4 | 5.184e-4 | 0.94 |
| cam_right p2 |  | 9.194e-4 | 1.004e-3 | 0.92 |
| hand-eye rx | deg | 1.130e-1 | 1.091e-1 | 1.04 |
| hand-eye ry | deg | 1.615e-1 | 1.592e-1 | 1.01 |
| hand-eye rz | deg | 2.874e-2 | 3.005e-2 | 0.96 |
| hand-eye tx | mm | 1.581e-1 | 1.707e-1 | 0.93 |
| hand-eye ty | mm | 1.420e-1 | 1.394e-1 | 1.02 |
| hand-eye tz | mm | 1.161e0 | 1.188e0 | 0.98 |

**Defaults (`auto`, the rig held in the hand-eye solve)**

| Parameter | Unit | std | predicted σ | std / σ |
|---|---|---|---|---|
| cam_left fx | px | 5.177e0 | 3.285e0 | 1.58 |
| cam_left fy | px | 5.183e0 | 3.276e0 | 1.58 |
| cam_left cx | px | 6.574e0 | 4.622e0 | 1.42 |
| cam_left cy | px | 4.683e0 | 1.960e0 | 2.39 |
| cam_left k1 |  | 1.809e-2 | 1.744e-2 | 1.04 |
| cam_left k2 |  | 2.161e-1 | 2.158e-1 | 1.00 |
| cam_left p1 |  | 5.158e-4 | 2.988e-4 | 1.73 |
| cam_left p2 |  | 9.870e-4 | 8.266e-4 | 1.19 |
| cam_right fx | px | 4.848e0 | 3.286e0 | 1.48 |
| cam_right fy | px | 4.847e0 | 3.276e0 | 1.48 |
| cam_right cx | px | 6.114e0 | 4.621e0 | 1.32 |
| cam_right cy | px | 4.672e0 | 1.960e0 | 2.38 |
| cam_right k1 |  | 1.730e-2 | 1.744e-2 | 0.99 |
| cam_right k2 |  | 2.103e-1 | 2.162e-1 | 0.97 |
| cam_right p1 |  | 4.928e-4 | 3.036e-4 | 1.62 |
| cam_right p2 |  | 9.216e-4 | 8.407e-4 | 1.10 |
| hand-eye rx | deg | 1.139e-1 | 4.784e-2 | 2.38 |
| hand-eye ry | deg | 1.618e-1 | 1.129e-1 | 1.43 |
| hand-eye rz | deg | 2.875e-2 | 2.969e-2 | 0.97 |
| hand-eye tx | mm | 1.626e-1 | 1.156e-1 | 1.41 |
| hand-eye ty | mm | 1.422e-1 | 1.030e-1 | 1.38 |
| hand-eye tz | mm | 1.209e0 | 7.620e-1 | 1.59 |
## G17.2: the per-feature residuals reproduce the report's RMS exactly: PASS

`crates/etendue-cli/tests/calibrate.rs`, `g17_2_…` (closed_loop_ur5e, 0.5 px, seed 7):
- **2592 features.** That is the report's `residuals.count`.
- **Exact RMS.** The per-axis RMS recomputed from `observed_px` and `projected_px` equals `rms_axis_px` exactly (`==` on `f64`).
- **Correct naming.** Each sampled feature's `observed_px` is the observation of the capture, camera and point it names.
- **Extras.** The report also has calibration-rs's covariance (`uncertainty`) and quality verdict (`quality`).

## Procedure

```bash
etendue gt examples/closed_loop_ur5e/scene.json examples/closed_loop_ur5e/scenario.json -o <ds>
echo '{"pixel_sigma_px": 0.5, "seed": 100}' > noise.json
etendue calibrate <ds> --noise noise.json --trials 200 --procedure rig_free.json   # rig refined
etendue calibrate <ds> --noise noise.json --trials 200                             # defaults
cargo test -p etendue-cli --test calibrate g17_2 -- --nocapture
```

`rig_free.json` holds:

```json
{ "version": 2, "id": "rig_free", "name": "rig hand-eye, rig refined",
  "procedure": { "stages": [ { "id": "rig_handeye", "problem": "rig_handeye", "free": { "refine_rig": true } } ] } }
```
