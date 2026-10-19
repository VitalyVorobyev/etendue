# G13.1, G13.2, G13.3: native calibration

Gates of P13 (`docs/pivot/PLAN.md`), measured 2026-10-19 on branch `p13a/observe` (on
`3c367bc`), Apple M4 Pro.
- `etendue calibrate` runs calibration-rs 0.9.0's workflows (`vision-calibration-pipeline`) in
  Rust, built against tag `v0.9.0`, the one CI clones.
- The wheel is `vision-calibration==0.9.0`, from `tools/closed-loop`.

## G13.1: the native rows equal the wheel's: PASS

```bash
etendue gt examples/<ex>/scene.json examples/<ex>/scenario.json -o target/<ex>
etendue calibrate target/<ex> -o target/<ex>/report.json
uv run --locked --project tools/closed-loop tools/closed-loop/parity.py target/<ex>/report.json target/<ex>
```

`parity.py` runs `closed_loop.py` with its comparison rows captured, not printed. Both sides
must have the same labels in the same order, and each value must agree within
max(1e-9, 1e-6·|value|).

| Example | Workflow | Rows | max \|Δ\| | Gate (native) |
|---|---|---|---|---|
| closed_loop_ur5e | rig hand-eye | 25 | 2.2e-12 | G5.1 PASS |
| closed_loop_ur5e `--camera cam_left` | single-camera hand-eye | 14 | 1.5e-12 | G5.1 PASS |
| eye_in_hand_ur5e | rig hand-eye | 25 | 1.1e-12 | G5.1 PASS |
| laser_ur5e | rig hand-eye laserline | 29 | 4.2e-12 | G5.3 PASS |
| rig3_laser_ur5e | rig hand-eye laserline, 3 lasers | 42 | 1.4e-12 | G5.3b PASS |
| laser_scheimpflug_ur5e | laserline, Scheimpflug, seeded | 33 | 1.9e-8 | G5.3 FAIL (as the wheel: a characterisation) |

Two cases are new, beyond the script:
- **eye_to_hand_ur5e** (rig hand-eye, eye-to-hand) passes G5.1: hand-eye 1.6e-5°, 2.7e-4 mm.
- **A single head**: laser_ur5e with one camera carrying its laser, the laserline-device
  workflow. It passes G5.3: plane normal 1.0e-6°, distance 1.5e-7 mm, fx 1.0e-9 relative.

  This only works with the workflow's `optimize.fix_poses` emptied. Its default, `[0]`, fixes
  the first target pose at its linear estimate, which left a 0.058 px misfit and a 0.73° plane
  error on exact data. A single camera has no gauge for a fixed pose to remove. This is noted
  for calibration-rs; no upstream change was made.

CI now runs the closed loops natively and checks parity against the wheel on closed_loop_ur5e
(the rig and `cam_left`) and laser_ur5e.

## G13.2: the same seed gives the same report: PASS

`etendue calibrate target/closed_loop_ur5e --noise noise.json -o n.json` with
`{"pixel_sigma_px": 0.5, "seed": 1}`, run three times: the three reports have the same
SHA-1, `32e003e9…`. `tests/calibrate.rs` also checks it in one process.

## G13.3: the residuals match the noise: PASS

The run is closed_loop_ur5e with `pixel_sigma_px` 0.5, 8 trials (seeds 1–8), n = 2592 points
(2n = 5184 residuals).

The expected per-axis RMS is σ·√(1 − p/2n). How many parameters p are free depends on
whether the robot-pose refinements count. calibration-rs refines each view's robot pose under
a prior, so the truth lies between the two:

| p | Expected (px) | Measured mean (px) | Deviation | Limit |
|---|---|---|---|---|
| 34: intrinsics 2 × 8, cam_se3_rig 6, hand-eye 6, target 6 | 0.4984 | 0.4908 | **−1.5 %** | 5 % |
| 178: plus 24 robot-pose refinements × 6 | 0.4913 | 0.4908 | −0.1 % | 5 % |

Across the 8 seeds the RMS ranges from 0.4825 to 0.4986 px.

At this noise, the errors that matter downstream (mean over the 8 seeds):

| Error | Mean |
|---|---|
| fx (relative) | 2.5e-3 (cam_left), 1.2e-3 (cam_right) |
| hand-eye rotation | 0.21° |

## P13c: the estimate beside the truth; observations in wasm

Measured 2026-10-19 on branch `p13c/compare` (on `c812423`).

- **Offsets** (`etendue-cli/tests/calibrate.rs`):
  - On exact data, laser_ur5e's camera and target offsets are identities to 1e-6 rad and 1e-6 m,
    and the true and estimated planes agree to 1e-10 in the normal and 1e-6 m in distance.
  - With 0.5 px noise, the reference camera's offset reproduces the hand-eye rows to 1e-9.
    Eye-in-hand (closed_loop_ur5e) matches both the angle and the translation. Eye-to-hand
    (eye_to_hand_ur5e) matches the angle, since there the translation is measured in another
    frame.
- **Observations in wasm:** G0.1 extended (see `g0_1_wasm_parity.md`), with 0 mismatches.
  Timed in Node with `pixel_sigma_px` 0.5:

  | Example | `observe` | JSON |
  |---|---|---|
  | closed_loop_ur5e | 2.7 ms | 0.29 MB |
  | laser_ur5e | 3.4 ms | 0.57 MB |
  | rig3_laser_ur5e | 11.7 ms | 2.9 MB |

  The studio recomputes 150 ms after the noise settles.
- **G2.1:** `etendue_wasm_bg.wasm` is **768,001 B** gzipped (limit 1,572,864). It was 689,423 B;
  ground-truth emission and the noise stream add 78,578 B.
