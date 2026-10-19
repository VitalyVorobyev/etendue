# G10.1 (kernel), G10.2, G10.3: the triangulation head

- **Gate G10.1** (`docs/pivot/PLAN.md` P10), kernel side: a head's working-volume map on 64 × 64
  takes ≤ 30 ms in wasm, so the Optics workspace can redraw it as a slider moves. The studio's
  redraw time (slider to paint, p95 ≤ 16 ms) is measured with the workspace (P10b).
- **Gate G10.2:** the Scheimpflug setting puts the plane of best focus on the laser plane, and
  the section's drawing shows it: the traces coincide and the hinge rule holds.
- **Gate G10.3:** on a thin window the optimiser reproduces the analytic Scheimpflug tilt within
  0.05°, and it never ends worse than its start.
- Measured 2026-10-19 on branch `p10a/head-kernel`, Apple M4 Pro, Node 24.20.0 (wasm, release),
  `cargo test` (debug).

## Procedure

```bash
cargo test -p etendue-core --test head -- --nocapture
node crates/etendue-wasm/scripts/build-npm.mjs
node crates/etendue-wasm/tests/node/g10_1_head.mjs
```

**The head:**
- camera: the library's IMX174 + Kowa LM25HC at f/4;
- laser: the StingRay 660, 100 mm off the camera axis (camera −y), aimed at the axis 0.35 m
  out, its line along camera x;
- window: 120 mm deep and 40 mm wide around the aim point.

The Rust tests also use a thick lens with its pupil off H.

## Results: PASS

| Check | Tolerance | Measured |
|---|---|---|
| G10.1: `headMap` 64 × 64 in wasm (median of 7) | ≤ 30 ms | **0.64 ms** (4096/4096 visible) |
| `headSection` with 3 probes and the depth of field | — | 0.38 ms |
| `headOptimise`, square sensor, f/2–f/16, 9 × 9 | — | 2.2 ms (104 iterations) |
| Scheimpflug setting against the legacy hinge formula `tan θ = (s_o + g)/v0 · tan τ` (thin lens) | 1e-9 rad | pass; −14.13° |
| G10.2: blur on the window after it (3 lenses, thick, pupil off H) | 1e-9 px | pass |
| G10.2: the section's focus trace against the laser trace | 1e-9 | pass |
| G10.2: hinge rule, sensor ∩ H′ against focus plane ∩ H, same height | 1e-9 m | pass |
| G10.3: thin window (0.1 mm), optimised tilt against analytic | 0.05° | **0.0007°** |
| G10.3: never worse than the start | ≤ start + 1e-9 | pass |
| depth-of-field polylines: blur there against the limit | 1e-6 px | pass |

## What the optimiser shows

- **With the tilt free,** the whole laser plane is in focus, so only diffraction is left. The
  widest allowed aperture wins (f/2), and the worst blur halves against f/4.
- **With the sensor square to the axis,** on the 120 mm deep window, focus and f-number balance
  defocus against diffraction. The optimum is f/15.3 at a focus of 0.340 m, with a 6.4 px worst
  blur, against 20.4 px at f/4 focused on the aim point.
