# G4.2 — corner bias study (Blender, chess-corners)

- Gate: **G4.2** (`docs/pivot/PLAN.md` P4-3)
- Criterion: on noise-free renders, chess-corners output agrees with the analytic ground truth to
  **RMS ≤ 0.02 px** at the chosen default supersampling.
- Result: **characterisation. The measured accuracy is the accepted baseline, and G4.2 is not a
  blocker** (user decision, 2026-09-28). No detector reaches 0.02 px, and the gate is not relaxed:
  it is reported as measured.
  - Rendering converges: at s ≥ 4 the error stops changing with s.
  - The error that remains belongs to the detector. On an **exact**, renderer-free image of the
    same board, chess-corners' ChESS refiners are off by 0.08–0.19 px RMS. That is the same level
    as on the Blender renders, and it matches chess-corners' own synthetic benchmark
    (`chess-rs/docs/reference/refiner-comparison.md`: mean 0.06–0.11 px on clean corners).
  - **The Radon detector** (chess-corners' `DetectorConfig::radon()`, Duda & Frese 2018) is 2–6×
    closer: **0.032 px RMS at s = 4** on linear renders, 0.046 px on the exact image. It is the
    recommended detector for synthetic datasets.
- Measured: 2026-09-27, on etendue commit `9061863` (branch `pivot/p4-corners`),
  calibration-rs `b7e470b2`, chess-corners 1.2.0, Blender 5.1.1 (Cycles, Metal, Apple M4 Pro).
- Re-run 2026-09-28 on the calib-targets board (P3-3), with the same results to within Monte
  Carlo noise: `p3_3_target_geometry.md`.
- Radon added 2026-09-28, on etendue commit `e213946` plus the Radon setups in
  `crates/etendue-cli/src/corners.rs` (branch `pivot/g4-2-radon`), calibration-rs `76353753`
  (0.8.3), chess-corners 1.2.0, Blender 5.1.1 (Cycles, Metal, Apple M4 Pro). The ChESS rows of
  that run match the recorded ones to within Monte Carlo noise (≤ 0.003 px RMS).

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
- **Detector:** two families, on 8-bit luminance scaled so that the image maximum maps to 240.
  - ChESS: `DetectorConfig::chess()` with calib-targets' workspace threshold of 15, run with each
    refiner (`center_of_mass`, the chess-corners default; `forstner`; `saddle_point`).
  - Radon: `DetectorConfig::radon()` with its preset (relative) threshold, run with each peak fit
    (`radon`: Gaussian, the default; `radon_parabolic`). The luminance is used either
  linear, as a sensor delivers it, or sRGB-encoded, as `etendue render` writes by default.
- **Matching:** the nearest detection within 1.5 px of each visible ground-truth corner. Every
  configuration matched 315/315 corners, except `saddle_point` on sRGB (311–314 in four rows).

## Result: detector vs analytic ground truth (linear, sharp)

RMS / max in px, over 315 corners:

| image | center_of_mass | forstner | saddle_point | radon | radon_parabolic |
|---|---|---|---|---|---|
| Blender s = 1 | 0.131 / 0.269 | 0.119 / 0.222 | 0.104 / 0.206 | 0.028 / 0.069 | 0.028 / 0.073 |
| Blender s = 2 | 0.175 / 0.367 | 0.074 / 0.257 | 0.121 / 0.270 | 0.027 / 0.079 | 0.028 / 0.086 |
| Blender s = 4 | 0.182 / 0.390 | 0.073 / 0.275 | 0.126 / 0.282 | **0.032** / 0.088 | 0.034 / 0.091 |
| Blender s = 8 | 0.186 / 0.389 | 0.071 / 0.271 | 0.127 / 0.283 | 0.033 / 0.085 | 0.035 / 0.092 |
| **exact image** | 0.189 / 0.395 | 0.079 / 0.253 | 0.130 / 0.293 | 0.046 / 0.103 | 0.047 / 0.109 |

The ChESS columns are the 2026-09-27 run; the Radon columns are the 2026-09-28 run, whose ChESS
rows agree with these to ≤ 0.003 px.

What the table shows:
- **The mean error is at most 0.013 px on sharp linear images** (every s, the exact image, and
  both detectors; Radon's is ≤ 0.003 px),
  and at most 0.036 px in any configuration (Förstner with blur or sRGB). There is no convention
  offset between the renderer, the LUT, the ground truth and the detector. That confirms G4.1's
  `Integer` default against the detector (ADR 0004): chess-corners puts pixel `i` at coordinate
  `i`.
- **The scatter is the detector's.** It is a deterministic function of each corner's sub-pixel
  phase and the local perspective. It is mirror-symmetric about the image centre, not radial, so
  it is not a geometry error. It appears at the same level on the exact image as on the renders.
- **s = 4 is converged.** s = 4 and s = 8 differ by less than 0.004 px RMS for every refiner;
  s = 1 and s = 2 are not converged.
- **Radon is closer to the truth on the renders than on the exact image** (0.032 vs 0.046 px at
  s = 4). The gap is mostly in the frontal pose (0.023 vs 0.060 px). A Gaussian PSF on the exact
  image does not close it (0.044 px at σ = 0.5–1 px), so it is not simply blur. A likely suspect
  is the frontal pose's regular sub-pixel phases, which a box-sampled image repeats exactly. This
  is not isolated further.

## Other factors

**sRGB encoding makes the error worse.** On the s = 8 render, RMS is 0.24 (center of mass), 0.20
(Förstner), 0.19 (saddle point) and 0.040 px (Radon); on the exact image it is 0.26, 0.23, 0.20
and 0.058 px. The
nonlinearity makes the dark and light quadrants asymmetric around the saddle. Images meant for
calibration should be linear: `etendue render --sensor` writes raw linear images.

**A lens PSF does not rescue it.** At σ = 1 px, on the s = 8 render: center of mass 0.109,
Förstner 0.155, saddle point 0.078, Radon 0.032 px RMS (exact image: 0.114, 0.158, 0.083,
0.044). Radon is insensitive to the blur (0.029–0.032 px at s = 4 over σ = 0–1 px). Blur helps the
center-of-mass and saddle-point refiners and hurts Förstner, whose 5 × 5 structure tensor
assumes a sharp edge. This agrees with chess-corners' own blur row.

**The renderer's own share** is the per-corner difference between detections on the Blender
render and on the exact image, with the same detector, linear and sharp:

| s | center_of_mass | forstner | saddle_point | radon | radon_parabolic |
|---|---|---|---|---|---|
| 1 | 0.081 / 0.206 | 0.100 / 0.375 | 0.061 / 0.340 | 0.049 / 0.122 | 0.050 / 0.133 |
| 2 | 0.039 / 0.158 | 0.060 / 0.289 | 0.059 / 0.334 | 0.035 / 0.096 | 0.036 / 0.106 |
| 4 | 0.032 / 0.142 | 0.059 / 0.253 | 0.063 / 0.343 | 0.032 / 0.084 | 0.032 / 0.093 |
| 8 | 0.029 / 0.141 | 0.055 / 0.256 | 0.057 / 0.345 | 0.030 / 0.082 | 0.031 / 0.091 |

(RMS / max px.) Two readings of this:
- It is the same order as G4.3's cross-backend budget of 0.05 px.
- It is also far above G4.1's geometric accuracy for the renderer (0.003 px on sphere centroids).
  Small photometric differences between the render and the exact image are the likely cause: the
  light radiance with specular, the Cycles pixel filter, and Monte Carlo noise. A refiner that
  moves 0.1–0.3 px with the sub-pixel phase magnifies such differences. This is not isolated
  further here.

## Decision (2026-09-28)

G4.2 as written ("RMS ≤ 0.02 px against the analytic GT") measures the detector, not the
renderer, and no chess-corners 1.2 detector meets it, even on an exact image. The user's decision:
try the Radon detector, and accept the current accuracy as the baseline. G4.2 is not a blocker.

- **Baseline:** Radon at s = 4 on linear renders, **0.032 px RMS / 0.088 px max** against the
  analytic ground truth, with a mean bias ≤ 0.003 px.
- **Defaults for synthetic datasets:** s = 4 (converged), linear output (`etendue render
  --sensor`), and the Radon detector. Among the ChESS refiners, Förstner is the closest on sharp
  images (0.073 px) and saddle point under blur (0.076 px at σ = 1 px).
- The image-level closed loop (P5-1) is no longer blocked. It inherits this baseline as its
  per-corner noise floor.
