# G4.2 — corner bias study (Blender, chess-corners)

- Gate: **G4.2** (`docs/pivot/PLAN.md` P4-3)
- Criterion: on noise-free renders, chess-corners output agrees with the analytic ground truth to
  **RMS ≤ 0.02 px** at the chosen default supersampling.
- Result: **FAIL, and the gate is mis-set for this detector. It is reported, not relaxed.**
  - Rendering converges: at s ≥ 4 the error stops changing with s.
  - The error that remains belongs to the detector. On an **exact**, renderer-free image of the
    same board, chess-corners' refiners are off by 0.08–0.19 px RMS. That is the same level as on
    the Blender renders, and it matches chess-corners' own synthetic benchmark
    (`chess-rs/docs/reference/refiner-comparison.md`: mean 0.06–0.11 px on clean corners).
  - No configuration reaches 0.02 px.
- Measured: 2026-09-27, on etendue commit `COMMIT` (branch `pivot/p4-corners`),
  calibration-rs `6e1bd222`, chess-corners 1.2.0, Blender 5.1.1 (Cycles, Metal, Apple M4 Pro).

## Procedure

```bash
cargo run --release -p etendue-cli -- measure g4-2 -o target/g4_2   # full table in target/g4_2/g4_2.md
```

- **Camera:** the examples' camera (1280 × 1024, fx 2318.8, k1 −0.08, k2 0.02), the same one G4.1
  used.
- **Board:** a mesh chessboard of 10 × 8 squares, 20 mm each, so 63 inner corners. Squares are
  about 93 px at 0.5 m.
- **Poses:** five, 315 corners in total.
  - frontal at 0.5 m
  - tilted 35° about x
  - tilted 35° about y
  - oblique: 25°/−25°/15° at 0.55 m, off-centre
  - far: 1.0 m, 20°/10°/5°
  - Corners closer than 12 px to an image edge are excluded.
- **Rendering:**
  - Lighting: a uniform white environment and no lights, so a Lambertian plane has no shading to
    sample.
  - Samples: 256 per target pixel at every s, i.e. `256 / s²` per canonical pixel with a minimum
    of 16.
  - Resampling: the canonical render goes through the remap LUT with the `⌈s⌉²` box filter
    (`etendue render`'s path). s ∈ {1, 2, 4, 8}.
- **Exact reference:** the same board area-sampled on 8 × 8 points per pixel through
  calibration-rs `backproject_pixel` and a ray–plane intersection. It uses the same radiances as
  the render (background 1.0, light 0.85, dark 0.03), and no renderer is involved.
- **PSF:** an optional Gaussian blur, σ ∈ {0, 0.5, 0.7, 1.0} px, applied after resampling to
  stand in for a real lens.
- **Detector:** `DetectorConfig::chess()` with calib-targets' workspace threshold of 15. It is run
  with each refiner (`center_of_mass`, the chess-corners default; `forstner`; `saddle_point`) on
  8-bit luminance scaled so that the image maximum maps to 240. The luminance is used either
  linear, as a sensor delivers it, or sRGB-encoded, as `etendue render` writes by default.
- **Matching:** the nearest detection within 1.5 px of each visible ground-truth corner. Every
  configuration matched 315/315 corners, except `saddle_point` on sRGB (311–314 in four rows).

## Result: detector vs analytic ground truth (linear, sharp)

RMS / max in px, over 315 corners:

| image | center_of_mass | forstner | saddle_point |
|---|---|---|---|
| Blender s = 1 | 0.131 / 0.269 | 0.119 / 0.222 | 0.104 / 0.206 |
| Blender s = 2 | 0.175 / 0.367 | 0.074 / 0.257 | 0.121 / 0.270 |
| Blender s = 4 | 0.182 / 0.390 | 0.073 / 0.275 | 0.126 / 0.282 |
| Blender s = 8 | 0.186 / 0.389 | **0.071** / 0.271 | 0.127 / 0.283 |
| **exact image** | 0.189 / 0.395 | 0.079 / 0.253 | 0.130 / 0.293 |

What the table shows:
- **The mean error is at most 0.013 px on sharp linear images** (every s, and the exact image),
  and at most 0.036 px in any configuration (Förstner with blur or sRGB). There is no convention
  offset between the renderer, the LUT, the ground truth and the detector. That confirms G4.1's
  `Integer` default against the detector (ADR 0004): chess-corners puts pixel `i` at coordinate
  `i`.
- **The scatter is the detector's.** It is a deterministic function of each corner's sub-pixel
  phase and the local perspective. It is mirror-symmetric about the image centre, not radial, so
  it is not a geometry error. It appears at the same level on the exact image as on the renders.
- **s = 4 is converged.** s = 4 and s = 8 differ by less than 0.004 px RMS for every refiner;
  s = 1 and s = 2 are not converged.

## Other factors

**sRGB encoding makes the error worse.** On the s = 8 render, RMS is 0.24 (center of mass), 0.20
(Förstner) and 0.19 px (saddle point); on the exact image it is 0.26, 0.23 and 0.20 px. The
nonlinearity makes the dark and light quadrants asymmetric around the saddle. Images meant for
calibration should be linear: `etendue render --sensor` writes raw linear images.

**A lens PSF does not rescue it.** At σ = 1 px, on the s = 8 render: center of mass 0.109,
Förstner 0.155, saddle point 0.078 px RMS (exact image: 0.114, 0.158, 0.083). Blur helps the
center-of-mass and saddle-point refiners and hurts Förstner, whose 5 × 5 structure tensor
assumes a sharp edge. This agrees with chess-corners' own blur row.

**The renderer's own share** is the per-corner difference between detections on the Blender
render and on the exact image, with the same detector, linear and sharp:

| s | center_of_mass | forstner | saddle_point |
|---|---|---|---|
| 1 | 0.081 / 0.206 | 0.100 / 0.375 | 0.061 / 0.340 |
| 2 | 0.039 / 0.158 | 0.060 / 0.289 | 0.059 / 0.334 |
| 4 | 0.032 / 0.142 | 0.059 / 0.253 | 0.063 / 0.343 |
| 8 | 0.029 / 0.141 | 0.055 / 0.256 | 0.057 / 0.345 |

(RMS / max px.) Two readings of this:
- It is the same order as G4.3's cross-backend budget of 0.05 px.
- It is also far above G4.1's geometric accuracy for the renderer (0.003 px on sphere centroids).
  Small photometric differences between the render and the exact image are the likely cause: the
  light radiance with specular, the Cycles pixel filter, and Monte Carlo noise. A refiner that
  moves 0.1–0.3 px with the sub-pixel phase magnifies such differences. This is not isolated
  further here.

## Decision needed

G4.2 as written ("RMS ≤ 0.02 px against the analytic GT") measures the detector, not the
renderer, and chess-corners 1.2 cannot meet it even on an exact image. Options:

1. **Keep G4.2 as characterisation, and gate the renderer** on the render-vs-exact difference
   above at s = 4 (0.03 px with center of mass). The detector floor is recorded here and used as
   the noise floor in P5-1.
2. **Improve the refiner upstream** (chess-rs, a model-based saddle fit on the area-sampled
   corner), then re-run G4.2 unchanged. The exact-image column is then the refiner's acceptance
   test.
3. **Leave G4.2 failing** until option 2 lands.

Default until decided: s = 4 (converged), linear output, `saddle_point` or `center_of_mass`
refiner.
