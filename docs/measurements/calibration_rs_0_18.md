# calibration-rs 0.18.0: gate re-runs

## The change

| Dependency | From | To | Source |
|---|---|---|---|
| `vision-calibration-*` | 0.12.0 | **0.18.0** | crates.io; CI's `CALIBRATION_RS_REF` is `v0.18.0` in every workflow |
| `vision-calibration` wheel | 0.12.0 | 0.18.0 | PyPI, used by `tools/closed-loop` |

calib-targets (0.16.1), chess-corners and nalgebra (0.34.2, with 0.35 for the detector stack only)
are unchanged.

## When and where

- Measured 2026-11-11, Apple M4 Pro.
- etendue branch `p19d/calibration-rs-0.18`, commit `913f858` (the bump and the typed boundary;
  the module split after it moves code only, and the test suite is unchanged by it). It is
  compared with `main` at `7c76a94` on calibration-rs 0.12.0.
- Built against calibration-rs `v0.18.0` (`865e86d`) exactly: a clean clone of the tag, passed
  with `cargo --config <file>` / `ETENDUE_CARGO_CONFIG`. The 0.12.0 build used a clean clone of
  `v0.12.0`.

## What changed for etendue

- **The hand-eye exports carry one tagged `handeye` value** (0.17). etendue read the exports as
  untyped JSON, so the bump compiled and failed at run time (10 calibration tests). It now reads
  every export through calibration-rs's types, and sends configs, seeds and inputs through a
  check that refuses any key calibration-rs does not read (`calibrate/upstream.rs`, `strict`).
  A run's `result.json` has the new shape.
- **The rig workflows refine a Scheimpflug camera's tilt by default** (0.17,
  `refine_sensor: true`). This is the only change of results etendue's examples show: the
  `laser_scheimpflug_ur5e` covariance now covers the tilts (4 more parameters) instead of holding
  them fixed.
- **`rig_handeye_laserline` takes rig, hand-eye and plane seeds** (`JointManualInit`). etendue
  does not pass them yet; a procedure that seeds them is still refused, with a clearer message.
- **Python wheel.** `result.handeye.gripper_se3_rig` instead of `result.gripper_se3_rig`; its unit
  tests pass.

## Reports, 0.12.0 against 0.18.0

`etendue calibrate` (the `auto` procedure) on the six examples' ground truth:

| Example | Exact | Noisy (0.5 px, seed 3, 2 trials) |
|---|---|---|
| closed_loop_ur5e | identical estimates; predicted σ within 1.5e-7 relative | estimates move < 0.01 σ |
| eye_in_hand_ur5e | identical estimates | estimates move < 0.01 σ |
| eye_to_hand_ur5e | estimates within 9e-6 relative (of errors near 1e-6) | estimates move < 0.01 σ |
| laser_ur5e | identical estimates | estimates move < 0.01 σ |
| laser_scheimpflug_ur5e | tilts refined: errors stay ~1e-7 (all gated rows PASS) | tilts move up to 0.33 σ; the hand-eye 1.275° → 1.265° |
| rig3_laser_ur5e | identical estimates; predicted σ within 0.3 % | estimates move < 0.01 σ |

"Moves < 0.01 σ" is the largest change of any focal length, principal point or tilt over its
predicted σ. Every pass/fail verdict is the same.

## Gates

| Gate | 0.12.0 | 0.18.0 | Result |
|---|---|---|---|
| G0.1 wasm parity | 0 mismatches | 0 mismatches (216 630 values) | PASS |
| G2.1 wasm size (gzip) | 835 KB | 835 KB (limit 1.5 MB) | PASS |
| G13.1 native rows = the wheel's | PASS | max \|Δ\| 5.5e-13 (closed_loop, laser) | PASS |
| G16.2, 9 built-in × example cases | PASS | PASS, worst errors as before (fx 1.6e-7, hand-eye 2.1e-4°, laser normal 6.2e-5°) | PASS |
| G17.1, rig refined, std / σ | 0.92–1.04 | 0.93–1.05 | PASS |
| G17.1, defaults, std / σ | up to 2.39 | 0.98–2.42 | unchanged finding |
| G18.1, closed_loop, 8 poses | 0.0362 mm planned, 0.0462 mm grid; measured 0.0377 mm (1.04) | the same | PASS |
| G18.2, eye_to_hand, 8 poses | 0.0407 mm planned, 0.0499 mm grid; measured 0.0428 mm (1.05) | the same | PASS |
| Planner σ / calibration-rs σ (34 params) | 0.929–1.139, 0.938–1.101 | the same | unchanged |

Other checks:
- `cargo test --workspace`: 358 tests pass; the shell's tests pass.
- clippy is clean, including wasm32 and the shell.
- Web: generate:types, check:deps, typecheck, lint, format, unit tests (93 + 10) and build pass.
- Studio e2e: 36 of 36 pass.

The image-level loop (P5-1b, Blender) was not re-run: the detectors (calib-targets 0.16.1) are
unchanged.

## Procedure

```bash
etendue gt examples/<e>/scene.json examples/<e>/scenario.json -o ds/<e>           # six examples
etendue calibrate ds/<e> -o <v>/<e>.exact.json                                     # v = old, new
etendue calibrate ds/<e> --noise noise3.json --trials 2 -o <v>/<e>.noise.json      # {"pixel_sigma_px": 0.5, "seed": 3}
etendue calibrate ds/<e> --procedure <built-in> -o g16_<built-in>_<e>.json          # G16.2
etendue calibrate ds/closed_loop_ur5e --noise noise100.json --trials 200 [--procedure rig_free.json]   # G17.1
uv run --locked --project tools/closed-loop tools/closed-loop/parity.py new/<e>.exact.json ds/<e>     # G13.1
cargo test --release -p etendue-cli --test g18_plan -- --include-ignored --nocapture                  # G18, ETENDUE_G18_TRIALS=100
```
