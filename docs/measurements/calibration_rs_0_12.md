# calibration-rs 0.12.0: gate re-runs

## The change

| Dependency | From | To | Source |
|---|---|---|---|
| `vision-calibration-*` | 0.11.0 | **0.12.0** | crates.io; CI's `CALIBRATION_RS_REF` is `v0.12.0` in every workflow |
| `vision-calibration` wheel | 0.11.0 | 0.12.0 | PyPI, used by `tools/closed-loop` |

calib-targets (0.16.1), chess-corners and nalgebra (0.34.2, with 0.35 for the detector stack only)
are unchanged.

## When and where

- Measured 2026-11-02, Apple M4 Pro.
- etendue commit `4c6ab0e`, branch `chore/calibration-rs-0.12`. It is compared with `main` at
  `d7b53a6` on calibration-rs 0.11.0.
- Built against calibration-rs `v0.12.0` (`8d5d1f08`) exactly: a clean clone of the tag, passed
  with `cargo --config <file>` / `ETENDUE_CARGO_CONFIG`. The 0.11.0 build used a clean clone of
  `v0.11.0`.

## What changed for etendue

- **The camera-model enums are `#[non_exhaustive]`.** Three matches gained a wildcard arm:
  - `from_scene` (sensor tilt check);
  - `thick_lens::scheimpflug_tilt`;
  - `gate::with_iters`, which also sets Kannala-Brandt's iteration count.
- **Distortion seeds are tagged `DistortionParams`.** Procedure seeds write
  `"type": "brown_conrady5"`.
- **Fisheye models.** The equidistant projection and Kannala-Brandt distortion appear in the
  schemas and the TS types. Scene and library validation refuse them, because etendue's physical
  optics, render remap and calibration seeds assume a pinhole.
- **Python wheel.** It has one `Camera` class. The cross-check only changed a type hint, and its
  unit tests pass.
- **`view_scatter`.** calibration-rs now measures it by leaving out one view at a time, so the
  predicted σ changes. The estimates do not.

## Reports, 0.11.0 against 0.12.0

`etendue calibrate` (the `auto` procedure) on the six examples' ground truth, compared value by
value:

| Example | Exact | Noisy (0.5 px, seed 3, 2 trials) |
|---|---|---|
| closed_loop_ur5e | identical | identical |
| eye_in_hand_ur5e | identical | identical |
| eye_to_hand_ur5e | identical | identical |
| laser_ur5e | identical | estimates within 3e-11 relative; predicted σ up to 4.2 % |
| laser_scheimpflug_ur5e | identical | estimates within 2e-11 relative; predicted σ up to 3.4 % |
| rig3_laser_ur5e | predicted σ up to 1.9 %; estimates identical | estimates within 7e-11 relative; predicted σ up to 1.2 % |

On the noisy laser runs, the estimate differences are floating-point noise (relative 1e-15 to
7e-11, the largest in residuals near zero). Every non-laser report is bit for bit the same.

## Gates

| Gate | 0.11.0 | 0.12.0 | Result |
|---|---|---|---|
| G0.1 wasm parity | 0 mismatches | 0 mismatches | PASS |
| G2.1 wasm size (gzip) | 833 KB | 835 KB (limit 1.5 MB) | PASS |
| G16.2, 9 built-in × example cases | PASS | PASS | Same estimates. Only the planar `intrinsics` and `stereo` stages' predicted σ change, by up to 2.4 %. |
| G17.1, rig refined, std / σ | 0.92–1.04 | 0.92–1.04 | PASS (unchanged) |
| G17.1, defaults, std / σ | up to 2.39 | up to 2.39 | unchanged finding |
| G18.1, closed_loop, 8 poses | 0.0362 mm planned, 0.0462 mm grid; measured 0.0377 mm (1.04) | the same | PASS |
| G18.2, eye_to_hand, 8 poses | 0.0407 mm planned, 0.0499 mm grid; measured 0.0428 mm (1.05) | the same | PASS |
| Planner σ / calibration-rs σ (34 params) | 0.93–1.14, 0.94–1.10 | 0.929–1.139, 0.938–1.101 | unchanged |

G16.1 compared `auto` with the code before P16, so it is not re-run. The report comparison above
covers the same ground: `auto` gives the same estimates on 0.12.

The image-level loop (P5-1b, Blender) was not re-run. The detectors (calib-targets 0.16.1,
chess-corners 2.1) are unchanged, and the estimates on the same observations are too.

Other checks:
- `cargo test --workspace`: 354 tests pass.
- The per-crate `--all-features` tests pass, and so do the shell's tests.
- clippy is clean, including wasm32 and the shell.
- `emit-schemas --check` and `check-layering` pass.
- On the web side, the types check, check:deps, typecheck, lint, format, unit tests (103) and
  build all pass.
- Studio e2e: 31 of 31 pass.

## Procedure

```bash
etendue gt examples/<e>/scene.json examples/<e>/scenario.json -o ds/<e>           # six examples
etendue calibrate ds/<e> -o <v>/<e>.exact.json                                     # v = old, new
etendue calibrate ds/<e> --noise noise3.json --trials 2 -o <v>/<e>.noise.json      # {"pixel_sigma_px": 0.5, "seed": 3}
etendue calibrate ds/<e> --procedure <built-in> -o g16_<built-in>_<e>.json          # G16.2
etendue calibrate ds/closed_loop_ur5e --noise noise100.json --trials 200 [--procedure rig_free.json]   # G17.1
cargo test --release -p etendue-cli --test g18_plan -- --include-ignored --nocapture                  # G18, ETENDUE_G18_TRIALS=100
```
