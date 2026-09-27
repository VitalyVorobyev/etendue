# G3.1 — canonical render camera and remap LUT round trip

- Gate: **G3.1** (`docs/pivot/PLAN.md` P3-1, ADR 0004)
- Criterion: over random pixels on full images of every supported model (including
  Scheimpflug with tilts up to 6°), the round trip `project(unproject(u))` is within
  **≤ 1e-6 px**. Report the `undistort` iterations needed at the image corners, and file an
  upstream issue if 8 iterations are insufficient.
- Result: **PASS — all 10 gate cameras, worst 8.2e-13 px**, with calibration-rs 0.8.2's
  default iteration caps. Undistortion now uses Newton's method (calibration-rs#122, fixing
  #120) and converges at the corners in 3–4 steps. The LUT round trip is at float32
  resolution (≤ 1.3e-4 px) for all ten.
- Measured: 2026-09-27, on etendue commit `COMMIT` (branch `pivot/p4-corners`),
  calibration-rs `v0.8.2` (`ce883adf`).
- History: first measured on etendue `39d1f86` against calibration-rs `b7e470b2` (0.8.1). Four
  cameras failed there with the fixed-point undistortion (table at the end).

## Procedure

```bash
cargo test -p etendue-synth --test g3_1 -- --nocapture          # gate (CI)
cargo run --release -p etendue-synth --example g3_1_remap        # report below
```

Ten cameras (`etendue_synth::gate::cameras`), 2048 × 1536 px, `fx = fy = 1800` (≈ 60°
horizontal) unless noted. Per camera: 10 000 random pixels (SplitMix64, fixed seed) plus the
four corners and two edge midpoints; the error is `|project_point_c(backproject_pixel(u)) − u|`,
both `vision-calibration-core` calls. Pixel coordinates use `PixelCentre::Integer`; the
result does not depend on it.

## Result

```text
camera                    iters   max err px    corner px      G3.1     corner iters for ≤1e-6 px
pinhole                       -    2.344e-13      0.000e0      PASS                   closed form
pinhole_skew_offcentre        -    2.344e-13      0.000e0      PASS                   closed form
brown_mild                    8    4.547e-13      0.000e0      PASS                             3
brown_strong_barrel           8    5.084e-13      0.000e0      PASS                             4
brown_pincushion              8    5.084e-13      0.000e0      PASS                             3
rational                     10    6.431e-13    1.137e-13      PASS                             3
thin_prism                   10    3.411e-13      0.000e0      PASS                             3
division                      -    5.084e-13      0.000e0      PASS                   closed form
scheimpflug_6x_brown          8    5.084e-13    2.274e-13      PASS                             3
scheimpflug_4x4_barrel        8    8.198e-13    3.216e-13      PASS                             4
```

| camera | parameters |
|---|---|
| pinhole_skew_offcentre | fy 1795, skew 0.8, principal point (1000, 790) |
| brown_mild | k1 −0.08, k2 0.02 |
| brown_strong_barrel | k1 −0.35, k2 0.15, k3 −0.03, p1 5e-4, p2 −3e-4 (wide-angle machine-vision lens) |
| brown_pincushion | k1 0.15, k2 0.05 |
| rational | k1 0.8, k2 0.2, k3 0.01, k4 1.1, k5 0.35, k6 0.02, p1 1e-4, p2 −1e-4 |
| thin_prism | k1 −0.1, k2 0.03, p1 2e-4, p2 −1e-4, s1 1e-3, s2 −5e-4, s3 8e-4, s4 2e-4 |
| division | λ −0.25 |
| scheimpflug_6x_brown | tilt_x 6°, brown_mild |
| scheimpflug_4x4_barrel | tilt 4.2° / 4.2°, brown_strong_barrel |

LUT round trip, `s = 4`, every 8th pixel, default iteration caps: canonical coordinate →
canonical ray → target pixel lands within **0.98e-4 … 1.32e-4 px** of the pixel for all ten
cameras; the bound is the float32 spacing of the LUT entry (≤ 7e-4 px).

Canonical cameras (margin 2 %) and LUT cost (native, release, M4 Pro, 3.1 MP):

| camera | canonical s = 1 | h-fov | canonical s = 4 | LUT ms |
|---|---|---|---|---|
| pinhole | 2090 × 1568 | 60.3° | 8356 × 6268 | 26–34 |
| brown_mild | 2172 × 1630 | 62.2° | 8684 × 6514 | 150 |
| brown_strong_barrel | 2608 × 1956 | 71.8° | 10426 × 7824 | 175 |
| rational | 2450 × 1838 | 68.5° | 9794 × 7346 | 238 |
| scheimpflug_4x4_barrel | 2912 × 2170 | 77.9° | 11648 × 8678 | 187 |

The canonical sizes grew by 2 px where the old undistortion had not converged: `cover` now sees
the true corner rays. LUT construction costs about 1.3–1.9× what it did with the fixed-point
iteration (94 → 150 ms for Brown, 185 → 238 ms for Rational). It is still a one-off cost per
camera.

## Findings

1. **Default undistortion iterations were not enough at the corners** of strongly distorted
   lenses (calibration-rs ≤ 0.8.1). `undistort` was a fixed-point iteration with a fixed count
   (8 Brown, 10 Rational / ThinPrism). It converged linearly and slowed toward the corners, so
   1e-6 px needed 14–28 iterations. calibration-rs 0.8.2 solves the forward map by Newton's
   method with analytic Jacobians (#122); `iters` is now a cap, and 3–4 steps reach 1e-12 px.
   Per ADR 0004 etendue never overrode `iters`, so the fix arrived through the dependency
   alone.
2. **The canonical camera grows with distortion and tilt**: +27 % width for the strong barrel,
   +42 % with the 4.2°/4.2° tilt. At `s = 4` that is up to 100 MP per canonical render —
   within WebGL's 16k texture limit but heavy; the supersampling default is a P4-3 measurement.
3. **Web rendering** (`@vitavision/three` `SensorView`) resamples the canonical render through
   this LUT; a Chromium test checks orientation, sampling and the no-ray background. In the
   studio the projected target grid lies on the rendered checker edges at 320 % zoom. The
   quantitative convention probe is P4-2 (G4.1).

## History: calibration-rs 0.8.1 (fixed-point undistortion)

Etendue `39d1f86`, calibration-rs `b7e470b2`:

```text
camera                    iters   max err px    corner px      G3.1     corner iters for ≤1e-6 px
pinhole                       -    2.344e-13      0.000e0      PASS                   closed form
pinhole_skew_offcentre        -    2.344e-13      0.000e0      PASS                   closed form
brown_mild                    8     4.830e-7     4.830e-7      PASS                             8
brown_strong_barrel           8     3.221e-1     3.221e-1      FAIL                            25
brown_pincushion              8     1.535e-3     1.535e-3      FAIL                            14
rational                     10     9.159e-5     9.159e-5      FAIL                            14
thin_prism                   10    5.602e-10    5.602e-10      PASS                             8
division                      -    5.084e-13      0.000e0      PASS                   closed form
scheimpflug_6x_brown          8     8.303e-7     8.303e-7      PASS                             8
scheimpflug_4x4_barrel        8     6.497e-1     6.497e-1      FAIL                            28
```
