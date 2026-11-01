# calibration-rs 0.11.0: gate re-runs

## The change

| Dependency | From | To | Source |
|---|---|---|---|
| `vision-calibration-*` | 0.9.0 | **0.11.0** | crates.io; CI's `CALIBRATION_RS_REF` is `v0.11.0` in every workflow |
| `vision-calibration` wheel | 0.9.0 | 0.11.0 | PyPI, used by `tools/closed-loop` |
| `calib-targets-*` | 0.15.5 | **0.16.1** | crates.io |
| `chess-corners` | 1.2 | **2.1** | crates.io; npm `@vitavision/chess-corners` is 2.1.0 too |
| `toml` | 0.9 | 1 | crates.io; the version the dataset crate uses |

calibration-rs's detectors now use calib-targets 0.16.1, the copy etendue uses, so the
calib-targets 0.12 copy is gone from both locks. The second nalgebra (0.35) stays, only for the
detector stack, as before (ADR 0001).

## When and where

- Measured 2026-11-01, Apple M4 Pro, Blender 5.2.2 (Cycles, Metal).
- etendue commits `f23dbf8` (crates) and `7cac6bf` (npm), branch `chore/calibration-rs-0.11`.
- Built against calibration-rs `v0.11.0` (`c1880f64`) exactly: a clean clone of the tag, passed
  with `cargo --config <file>` / `ETENDUE_CARGO_CONFIG`. The sibling checkout is on another
  branch.

## What changed for etendue

**Intrinsics lose skew.**
- calibration-rs 0.10 replaced `IntrinsicsParams::FxFyCxCySkew` with a plain `FxFyCxCy`.
- The code, examples, library, schemas, typings and web apps all use `{fx, fy, cx, cy}`.
- Old JSON still loads, because the extra keys are ignored.

**Compound tilts behind a pupil off H are refused.** This was the user's decision on 2026-11-01; ADR 0008 is amended.
- A sensor tilted about both axes behind a lens with `m_p ≠ 1` needs a skew in its equivalent
  pinhole. The 0.9 model had one.
- Without the skew the pinhole misplaces pixels by 0.2 px (tilt 0.03 / 0.02 rad) to 14 px
  (−0.15 / 0.1 rad) over `tests/camera_frames.rs`' cases.
- `PhysicalCamera::new` now refuses that case, and P has no roll.
- One-axis tilts, and any tilt with `m_p = 1`, are unchanged and exact.
- The head optimiser frees only the tilt axes the Scheimpflug tilt uses when `m_p ≠ 1`.
- The fixtures of G9.1/G9.4 and G0.1 that used a compound tilt behind such a lens now use a
  one-axis tilt.
- A test checks the refusal.
- No shipped example or library item was affected.

**`etendue calibrate` takes calibration-rs 0.11's defaults.** The gates are judged against the
truth, not against the 0.9 numbers, so nothing is pinned to 0.9 behaviour:
- rigs refine intrinsics in the rig and hand-eye bundle adjustments;
- robot-pose refinement uses 0.11's default prior;
- the laserline device's losses (Huber 1.0) are the defaults;
- no target pose is held.

**The corner-bias study loses one setup.** chess-corners 2 removed the Förstner refiner, so the
study has no Förstner setup any more.

## Results

Every gate passes.

| Gate | Record | On 0.11.0 |
|---|---|---|
| Workspace tests | — | **336 passed**, 0 failed |
| Per crate, all features | — | core 168, scene 36, kinematics 24 (G1.1, G1.2), synth 56: all pass |
| clippy | — | clean: workspace, wasm32, per crate with all features, the studio shell |
| `check-layering` | — | ok (nalgebra 0.34.2; shell 0.34.2) |
| `emit-schemas` | — | `scene` and `library` schemas regenerated (the intrinsics shape) |
| G0.1 wasm parity | `g0_1_wasm_parity.md` | **PASS**, 216 630 values, 0 bit mismatches |
| G2.1 wasm size | `g2_1_wasm_size.md` | **PASS**, 772 260 B gzip -9 (≤ 1 572 864). At P9 it was 638 942 B; the growth is calib-targets 0.16 and chess-corners 2 in the target code |
| G2.3, G10.1, G11.3, G12.2 (wasm timing) | their files | **PASS**: generator median 67.3 ms (≤ 100) |
| G3.1 remap | `g3_1_remap.md` | **PASS** for every camera (`pinhole_skew_offcentre` is now `pinhole_offcentre`, without skew) |
| G4.1 pixel convention | `g4_1_convention.md` | **PASS**. Blender worst 0.0029 px; web worst 0.0053 px (≤ 0.01). `offcentre_skew_tilt` is now `offcentre_tilt`, without skew |
| G4.2 corner bias | `g4_2_corner_bias.md` | Characterisation; the Radon baseline check passes (0.0313 px against 0.032 + 0.005). See below |
| G4.3 web vs Blender | `g4_3_cross_backend.md` | **PASS**, every setup ≤ 0.0193 px RMS (≤ 0.05) |
| G5.1 closed loop | `closed_loop.md` | **PASS**: closed_loop_ur5e (rig and `--camera cam_left`), eye_in_hand_ur5e, eye_to_hand_ur5e |
| G5.3 laser closed loop | `g5_3_laser_closed_loop.md` | **PASS**: laser_ur5e, rig3_laser_ur5e and, new, **laser_scheimpflug_ur5e** |
| G13.1 parity with the wheel | `g13_calibration.md` | **PASS** on all five examples and the single camera; max \|Δ\| 1.4e-12 (Scheimpflug), ≤ 5.6e-13 otherwise |
| G13.2 determinism | `g13_calibration.md` | **PASS**, three runs, one SHA-1 (`b7547266…`) |
| G13.3 residuals match the noise | `g13_calibration.md` | **PASS**, mean RMS 0.4957 px against 0.4984 expected (p = 34): −0.5 % (≤ 5 %) |
| `project check examples` | — | ok |
| Studio e2e (Playwright) | — | 23 passed |
| Book widgets e2e | — | 11 passed |

### G5.3 on the Scheimpflug rig

On 0.9 this rig stopped short of the truth (laser normal 0.024°, hand-eye 0.034°). On 0.11 it
reaches the truth:

| | cam_left | cam_right |
|---|---|---|
| laser normal | 1.3e-7° | 2.5e-9° |
| laser distance | 6.3e-7 mm | 3.5e-7 mm |
| tilt error | ≤ 6e-8° | ≤ 6e-8° |

The hand-eye is 1.1e-7° and 1.1e-2 µm.

### G4.2 with chess-corners 2

At s = 4, sharp, on linear renders (RMS px):

| Detector | 1.2 (2026-10-18) | 2.1 |
|---|---|---|
| ChESS, center of mass | 0.1825 | 0.1825 |
| ChESS, saddle point | 0.1247 | **0.0155** |
| Radon | 0.0313 | 0.0313 |
| Radon, parabolic | 0.0332 | 0.0332 |

- The Förstner setup no longer exists; on 1.2 it gave 0.0709.
- chess-corners 2's saddle point is an image refiner, and it now meets G4.2's original 0.02 px.
  The best row at s ≥ 4 is the saddle point at PSF σ = 1, 0.0149 px.
- `etendue detect` still uses Radon, the 1.2-era recommendation. Whether to switch is a separate
  decision.

### Studio shell

- clippy is clean.
- 11 tests passed, 2 ignored.
- The lock was re-resolved against the tag.

## Not re-run

- P4-5 determinism: rendering does not use calibration-rs or the detectors.
- G1.4 motion limits and the kinematics gates other than through the per-crate tests: those crates
  changed only in test fixtures.
