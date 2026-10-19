# G10.1, G10.2, G10.3: the triangulation head and the Optics workspace

- **Gate G10.1** (`docs/pivot/PLAN.md` P10):
  - kernel: a head's working-volume map on 64 × 64 takes ≤ 30 ms in wasm;
  - studio: a slider of the Optics workspace redraws the section and the map with JS work per
    frame p95 ≤ 16 ms.
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

## G10.1, studio side: PASS

Measured 2026-10-19 on branch `p10b/optics-workspace`, headed Chromium 153 on ANGLE Metal (Apple
M4 Pro, 120 Hz):

```bash
cd web/apps/studio && bun x playwright test --config playwright.perf.config.ts perf_optics
```

The test drags the Optics workspace's focus slider back and forth for 4 s. Every move redraws:
- the kernel's section, map and Scheimpflug setting;
- the React panels;
- the SVG section;
- the 64 × 48 canvas map.

| | Frames | Interval p50 | Interval p95 | Work p50 | Work p95 | Work max |
|---|---|---|---|---|---|---|
| Drag | 482 | 8.3 ms | 10.0 ms | 3.1 ms | **3.7 ms** | 6.8 ms |

The work limit is 16 ms. The frame probe now runs a heartbeat of its own, because a 2-D
workspace has no animation loop to count frames by.

## P10c: the sensor view, the stripe width and the laser's focus

Re-measured 2026-10-19 on `64639cb` (branch `p10c/stripe`), Apple M4 Pro, Node 24.20.0, headed
Chromium 153 on ANGLE Metal (120 Hz). P10c adds three things:
- **Kernel:** the map's samples carry the beam's imaged thickness (`beam_px`) and their pixel.
- **Studio:** the Sensor panel, which draws the laser line at seven ranges.
- **Laser model:** a laser's `waist_distance_m`. The head fixture is now focused at its aim.

| Gate | Limit | Measured |
|---|---|---|
| G10.1 kernel: `headMap` 64 × 64 | ≤ 30 ms | **0.90 ms** |
| G10.1 studio: focus-slider drag, work p95 | ≤ 16 ms | **3.6 ms** (482 frames; interval p95 9.2 ms) |
| G10.2, G10.3 (`cargo test -p etendue-core --test head`) | as above | pass (7 tests) |
| G0.1: wasm against native, bits | 0 mismatches | **0** of 216,624 values |
| G2.1: `etendue_wasm_bg.wasm`, gzip -9 | ≤ 1,572,864 B | **770,328 B** |

The stripe width is the imaged beam and the camera's blur, root sum of squares. On the default
StingRay head, over its 120 mm deep window:

| Laser focused | Camera | Stripe width |
|---|---|---|
| at the laser (the old model) | as built, f/4 | 36.0–39.8 px |
| at the laser | Scheimpflug | 37.1–37.4 px |
| at the aim point | as built, f/4 | 1.8–20.9 px (the widest is the camera's defocus at the window's ends) |
| at the aim point | Scheimpflug | 1.8–8.9 px |

The 50 µm waist has a Rayleigh range of 12 mm at 660 nm. Focused at the laser, the beam is
2.7 mm thick at the aim point; focused at the aim point, it is 0.1 mm thick there.

## P10f: the head in 3-D (2026-10-19, branch `p10f/anatomy`)

The head's section now carries the camera in 3-D (`Section.solid`). Its test checks three
things on a tilted sensor:
- each corner is on the sensor at its pixel, to 1e-9 px;
- a point along each field ray projects to its corner, to 1e-6 px;
- the plane of best focus is the camera's own, compared bit for bit.

| Gate | Limit | Measured |
|---|---|---|
| G10.1 kernel: `headMap` 64 × 64; `headSection` | ≤ 30 ms | **0.87 ms**; 0.49 ms |
| G0.1 (the section's JSON text now includes the solid) | 0 mismatches | **0** of 216,630 values |
| G2.1: gzip -9 | ≤ 1,572,864 B | **773,473 B** |
