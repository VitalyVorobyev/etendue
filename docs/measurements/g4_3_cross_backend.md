# G4.3 — cross-backend corner agreement (Blender vs web)

- Gate: **G4.3** (`docs/pivot/PLAN.md` P4-4)
- Criterion: the same frame, camera and detector on both backends give corners whose RMS
  difference is **≤ 0.05 px**.
- Result: **PASS for all three chess-corners refiners.** Web vs Blender, per corner, at s = 4:
  center of mass **0.018 px**, Förstner 0.040 px, saddle point 0.043 px RMS.
- Measured: 2026-09-27, on etendue commit `COMMIT` (branch `pivot/p4-cross-backend`),
  calibration-rs `v0.8.2`, chess-corners 1.2.0 (Rust crate in Blender's pipeline, its wasm
  build `@vitavision/chess-corners` 1.2.0 in the browser), Blender 5.1.1 (Cycles, Metal,
  Apple M4 Pro), Chromium 153 (SwiftShader WebGL).

## Procedure

```bash
cargo run --release -p etendue-cli -- measure g4-2 -o target/g4_2   # Blender side; writes detections.json
cd web/apps/studio && bun x vitest run --project browser --reporter=verbose --silent=false \
  src/probe/g4_3.browser.test.ts                                    # web side + comparison
```

- **Scene:** the G4.2 scene: the examples' camera, the 10 × 8 mesh chessboard of 20 mm
  squares, five poses and 315 visible corners. The web side reads every parameter from the
  Blender run's `detections.json`, so both backends render the same definition.
- **Blender:** Cycles at s = 4 with 256 samples per target pixel under a uniform white
  environment, then the remap LUT and a 4 × 4 box filter (`etendue render`'s path).
- **Web:** `SensorView` at s = 4 (canonical pinhole and LUT from `@etendue/wasm`, half-float
  canonical target, 4× MSAA, 4 × 4 box filter). The board squares and a far background wall
  use unlit constant radiances equal to the Blender scene's (dark 0.03, light 0.85, sky 1.0).
- **Detector input on both sides:** linear luminance, scaled so the image maximum maps to 240,
  rounded to 8 bits; ChESS with threshold 15 and each refiner.
- **Comparison:** for every visible ground-truth corner, the nearest detection within 1.5 px
  on each backend. The metric is the distance between the two detections.

## Result

| refiner | web vs Blender RMS (px) | max (px) | web vs GT RMS (px) | Blender vs GT RMS (px) |
|---|---|---|---|---|
| center_of_mass | **0.0184** | 0.068 | 0.1835 | 0.1823 |
| forstner | **0.0403** | 0.334 | 0.0714 | 0.0725 |
| saddle_point | **0.0426** | 0.330 | 0.1256 | 0.1257 |

All 315 corners matched on both backends for every refiner.

## Findings

1. **The backends agree to well within G4.3.** Each backend's error against GT matches the
   other's to 0.001 px RMS. The detector's large per-corner error (G4.2) is the same on both
   backends, and it cancels in the comparison.
2. **The remaining difference comes from how sensitive each refiner is to the image.**
   Center of mass reads the smoothed ChESS response, and its cross-backend difference is
   0.018 px. Förstner and saddle point fit the raw pixels, and a few corners differ by up
   to 0.33 px. The images themselves differ in small ways: Cycles' pixel filter and noise,
   and the specular term of the Blender board's material, against MSAA with an unlit
   material on the web.
3. The web probe runs only where `target/g4_2/detections.json` exists. Blender is local-only
   (PLAN §6), so CI registers the test as skipped.
