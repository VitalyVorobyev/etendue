# G9.1, G9.4: the physical camera model (core)

- **Gate G9.1** (`docs/pivot/PLAN.md` P9): in the thin-lens limit (g = e = 0), fx, the
  projection frame's offset `M_se3_P`, and the near and far depth of field match the
  textbook formulas to 1e-12 relative.
- **Gate G9.4** (ADR 0008 §4), core part: the camera's projection and blur match an
  independent paraxial trace, and refocusing keeps the mechanical frame fixed. The parts
  that run through the scene's consumers (ground truth, render jobs, wasm, a re-parented or
  moved cell) come with the scene bridge (P9b).
- Measured 2026-10-19 on branch `p9a/camera-model` (on top of `477f164`), Apple M4 Pro,
  `cargo test` (debug).

## Procedure

```bash
cargo test -p etendue-core --test camera_frames -- --nocapture
```

`crates/etendue-core/tests/camera_frames.rs` runs over 25 cases: 5 lenses × 5 sensor tilts.
- **Lenses:** thin; thick; crossed principal planes; the entrance pupil in front of H; the
  entrance pupil behind H.
- **Tilts:** none; 0.08 rad about x; −0.06 rad about y; (0.07, 0.05); (−0.15, 0.1).
- **Points:** 200 object points at 0.6–1.4 × the 0.3 m focus distance, over a ±0.2 rad field.

The reference trace (`optics::paraxial`) uses only Gaussian conjugates and line–plane
intersections. Its exit pupil is the image of the entrance pupil, not the closed-form `m_p`.

## Result: PASS

| Check | Tolerance | Measured |
|---|---|---|
| projection vs the chief-ray trace (all 25 cases) | 1e-9 px | **8.3e-13 px** |
| back-projection passes through the point, from E | 1e-10 m | pass |
| blur on the plane of best focus; every rim ray meets the chief ray there | 1e-15 m; 1e-14 m | pass |
| blur without tilt vs the traced rim circle | 1e-12 × exit pupil | pass |
| blur with tilt vs the traced ellipse's mean diameter | 0.6·tan θ (relative) | 0.54·tan θ |
| G9.1: fx = f·d/((d − f)·p), `M_se3_P` = v0, near/far vs textbook, hyperfocal (4 focus distances) | 1e-12 relative (far 1e-10) | pass |
| m_p = 1: model tilt equals physical tilt; one axis: tan τ′ = tan τ/m_p, fy/fx = cos τ′/cos τ | 1e-12 | pass |
| refocus 2 m → 0.25 m: fx grows, E moves out, M fixed; focus plane sharp, projection = trace | 1e-15 m, 1e-9 px | pass |
| the legacy `coc` (g = e = 0, no tilt) | 1e-12 × exit pupil | pass |
| legacy `ThickLens` plane of best focus vs `PhysicalCamera` (calibration-rs tilt) | 1e-12 for one axis; 1 − cos τx for two | pass |

The tilted blur is first order: the model reads the sensor's position at the paraxial image's
lateral position, while an oblique chief ray meets the tilted sensor beside it. Its error
grows linearly with the tilt: about 2 % at 0.08 rad, and 9 % at the compound (−0.15, 0.1) rad
with the pupil behind H, whose cone is widest.

## Defects found and fixed

- **The legacy model's τx was mirrored** relative to calibration-rs (ADR 0008 §9).
  `ThickLens` and the Scheimpflug solver now convert with `optics::coc_tilt`.
- **`image_of` divided by the object distance** to get the magnification. That is 0/0 for a
  point on H, and the entrance pupil sits on H when e = 0. The first run of the projection
  test then passed vacuously, because `f64::max` drops NaN. The tests now assert finiteness.
- **argmin's Nelder–Mead stalled** at 0.49 px on the solver's fronto-parallel test (optimum
  0). Restarts reach 4e-6 px.
