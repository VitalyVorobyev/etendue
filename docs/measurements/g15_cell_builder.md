# G15.1: the guided cell builder

Gate of P15 (`docs/pivot/PLAN.md`).

- Measured 2026-11-02, Apple M4 Pro, Chromium (Playwright, SwiftShader).
- etendue branch `p15/cell-builder` (commit in the PR), on `p14c/library-workspace`.
- calibration-rs `v0.11.0`, a clean clone.

## G15.1: PASS

The Playwright spec `web/apps/studio/e2e/cell_builder.spec.ts` drives the Setup stepper in this
order: optics → rig → mounting → target → robot → review. It builds each example from library
items, downloads the scene it made, and compares that scene with the example's
(`e2e/close.ts`). Every number must agree within 1e-12. Ids, parents and every other value must be
identical.

| Example | Library items | Largest \|Δ\| |
|---|---|---|
| `eye_in_hand_ur5e` | rig `stereo_80mm_parallel` with cameras `python1300_pinhole` and `python1300_pinhole_right`, mount `flange_plate_30mm`, target `chess_9x6_25mm`, UR5e | 9.8e-18 |
| `eye_to_hand_ur5e` | the same rig and cameras on a stand, mount `board_holder_10mm`, target `chess_9x6_25mm`, UR5e | 1.1e-16 |

The differences are rounding only: both scenes hold the same items, poses and parameters.

## Procedure

```bash
node crates/etendue-wasm/scripts/build-npm.mjs
cd web/apps/studio && bun x playwright test e2e/cell_builder.spec.ts --reporter=list
```
