# G3.1 — canonical render camera and remap LUT round trip

- Gate: **G3.1** (`docs/pivot/PLAN.md` P3-1, ADR 0004)
- Criterion: over random pixels on full images of every supported model (including
  Scheimpflug with tilts up to 6°), the round trip `project(unproject(u))` is within
  **≤ 1e-6 px**. Report the `undistort` iterations needed at the image corners, and file an
  upstream issue if 8 iterations are insufficient.
- Result: **OPEN UPSTREAM — 6 of 10 gate cameras pass; 4 fail with calibration-rs's default
  iteration counts** (calibration-rs#120, filed 2026-09-27). The remap itself is exact: with
  converged undistortion every camera passes, and the LUT round trip is at float32
  resolution (≈1.2e-4 px) for all ten.
- Measured: 2026-09-27, on etendue commit `39d1f86` (branch `pivot/p3-synth`),
  calibration-rs `b7e470b2`.

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
brown_mild                    8     4.830e-7     4.830e-7      PASS                             8
brown_strong_barrel           8     3.221e-1     3.221e-1      FAIL                            25
brown_pincushion              8     1.535e-3     1.535e-3      FAIL                            14
rational                     10     9.159e-5     9.159e-5      FAIL                            14
thin_prism                   10    5.602e-10    5.602e-10      PASS                             8
division                      -    5.084e-13      0.000e0      PASS                   closed form
scheimpflug_6x_brown          8     8.303e-7     8.303e-7      PASS                             8
scheimpflug_4x4_barrel        8     6.497e-1     6.497e-1      FAIL                            28
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

LUT round trip, `s = 4`, every 8th pixel, converged undistortion where the default is not:
canonical coordinate → canonical ray → target pixel lands within **0.9e-4 … 1.3e-4 px** of the
pixel for all ten cameras; the bound is the float32 spacing of the LUT entry (≤ 7e-4 px).

Canonical cameras (margin 2 %) and LUT cost (native, release, M4 Pro, 3.1 MP):

| camera | canonical s = 1 | h-fov | canonical s = 4 | LUT ms |
|---|---|---|---|---|
| pinhole | 2090 × 1568 | 60.3° | 8356 × 6268 | 25–30 |
| brown_mild | 2172 × 1630 | 62.2° | 8684 × 6514 | 94 |
| brown_strong_barrel | 2606 × 1956 | 71.8° | 10424 × 7822 | 94 |
| rational | 2450 × 1838 | 68.5° | 9794 × 7346 | 185 |
| scheimpflug_4x4_barrel | 2910 × 2168 | 77.9° | 11638 × 8670 | 104 |

## Findings

1. **Default undistortion iterations are not enough at the corners** of strongly distorted
   lenses. calibration-rs's `undistort` is a fixed-point iteration with a fixed count (8
   Brown, 10 Rational / ThinPrism); it converges linearly and slows toward the corners. The
   counts needed for 1e-6 px are 14–28. Per ADR 0004 etendue does not override `iters`;
   calibration-rs#120 proposes tolerance-based stopping or Newton steps. `tests/g3_1.rs`
   records the four cameras with their measured errors, asserts they still fail as measured
   (so the list cannot go stale) and that they pass at 40 iterations.
2. **The canonical camera grows with distortion and tilt**: +27 % width for the strong barrel,
   +42 % with the 4.2°/4.2° tilt. At `s = 4` that is up to 100 MP per canonical render —
   within WebGL's 16k texture limit but heavy; the supersampling default is a P4-3 measurement.
3. **Web rendering** (`@vitavision/three` `SensorView`) resamples the canonical render through
   this LUT; a Chromium test checks orientation, sampling and the no-ray background. In the
   studio the projected target grid lies on the rendered checker edges at 320 % zoom. The
   quantitative convention probe is P4-2 (G4.1).
