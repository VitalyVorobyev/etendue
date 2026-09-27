# G4.1 — pixel-centre convention probe (both backends)

- Gate: **G4.1** (`docs/pivot/PLAN.md` P4-2, ADR 0004)
- Criterion: render small emissive spheres at known 3D points in Blender and in the web
  `SensorView`; the intensity-weighted centroid of each sphere's image is within
  **≤ 0.01 px** of the analytic projection of its centre, on both backends.
- Result: **PASS on both backends, under both conventions.** Worst 0.0030 px in Blender and
  0.0049 px on the web; mean bias ≤ 0.0005 px.
- Measured: 2026-09-27, on etendue commit `d3b7f0f` (branch `pivot/p4-blender`), calibration-rs
  `b7e470b2`, Blender 5.1.1 (Cycles, Metal, Apple M4 Pro), Chromium 153 (SwiftShader WebGL).

## Procedure

```bash
cargo run --release -p etendue-cli -- measure g4-1 -o target/g4_1        # Blender
cd web/apps/studio && bun x vitest run --project browser                  # web (SensorView)
```

- **Cameras** (1280 × 1024), both with Brown–Conrady distortion:
  - `brown`: the examples' camera (fx 2318.8, k1 −0.08, k2 0.02).
  - `offcentre_skew_tilt`: fx 2300, fy 2296, principal point (612.3, 541.7), skew 0.6, k1 −0.05, k2 0.01, p1 2e-4, p2 −1e-4, Scheimpflug tilt 2° / −1°.
- **Spheres:** 35 per camera. Pixels on a 7 × 5 grid, off pixel centres (+0.37, +0.21), are back-projected to 0.5 m. Each sphere has radius 1 mm (≈ 4.6 px) and radiance `(N·V)²`, which is smooth and falls to zero at the limb. The background is black and there is no other light.
- **Rendering:** the canonical pinhole at s = 4, then resampled through the remap LUT with a 4 × 4 box filter over the pixel footprint.
  - Blender: 256 samples, multilayer EXR.
  - Web: half-float canonical target with 4× MSAA, float output target.
- **Measurement:** the luminance centroid in a ±10 px window around the analytic projection (calibration-rs `project_point_c`). Pixel `i` sits at `i + offset` for the convention used.

## Result

| backend | camera | convention | mean error (px) | RMS (px) | max (px) |
|---|---|---|---|---|---|
| Blender | brown | Integer | (−0.0001, −0.0005) | 0.0013 | 0.0030 |
| Blender | brown | Half | (−0.0000, +0.0001) | 0.0012 | 0.0023 |
| Blender | offcentre_skew_tilt | Integer | (−0.0001, −0.0004) | 0.0013 | 0.0025 |
| Blender | offcentre_skew_tilt | Half | (−0.0001, +0.0002) | 0.0013 | 0.0024 |
| web | brown | Integer | (+0.0001, −0.0004) | 0.0018 | 0.0037 |
| web | brown | Half | (+0.0002, +0.0003) | 0.0019 | 0.0040 |
| web | offcentre_skew_tilt | Integer | (+0.0005, −0.0003) | 0.0021 | 0.0041 |
| web | offcentre_skew_tilt | Half | (+0.0006, +0.0005) | 0.0023 | 0.0049 |

## Findings

1. **No half-pixel offset in either pipeline.** A slip would appear as a ~0.5 px mean; the
   means are below 0.001 px, under both conventions. Blender's raster (pixel `k` centred at
   `k + ½` of its film) and three.js's render target both agree with the canonical camera's
   principal point at `edge + W/2`. The convention is thus a labelling choice (ADR 0004),
   default `Integer`; P4-3 matches it to the detector.
2. **Pixel locking of the probe, not of the pipeline.** The first probe (0.5 mm spheres,
   uniform emission — hard-edged discs of 2.3 px radius) failed with 0.011 px worst in
   Blender. The errors did not change with 4× more samples and repeated exactly per
   sub-pixel phase (every row at phase .21 had the same −0.007 px), which is the signature
   of centroiding a small hard-edged blob. A 1 mm sphere with `(N·V)²` radiance removes it
   (worst 0.003 px). Bias from the finite sphere off-axis stays below 1e-3 px.
3. **Point-sampling a supersampled render aliases.** With a single bilinear sample per
   output pixel, the worst sphere was 0.035 px (Blender) and 0.019 px (web) off. Box-
   filtering over the pixel footprint (`remap_image_box`, `SensorView` `taps`), with the
   footprint taken from the LUT's finite differences, brings both under the gate.
   `etendue render` now resamples this way (`taps = ⌈s⌉`).
4. **Monte Carlo noise is not the limit.** 256 and 1024 Cycles samples gave the same errors
   to 1e-4 px.
