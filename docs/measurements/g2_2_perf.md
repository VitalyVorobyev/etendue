# G2.2 — studio playback frame time

- Gate: **G2.2** (`docs/pivot/PLAN.md`, P2-3)
- Criterion: with 2 robots, 4 cameras, 1 target and live scenario playback, the **p95
  frame time over 600 frames is ≤ 16.7 ms** in Chromium on an M-series Mac.
- Result: **PASS**. p95 frame interval 7.8 ms; p95 main-thread work per frame 0.6 ms.
- Measured: 2026-09-27, on etendue commit `COMMIT` (branch `pivot/p2`).

## Environment

| Item | Version |
|---|---|
| Host | MacBook Pro, Apple M4 Pro, macOS; display at 144 Hz (ProMotion) |
| Browser | Playwright Chromium 153 (headed), WebGL on ANGLE Metal ("Apple M4 Pro") |
| Build | studio production build (`vite build`, `vite preview`) |
| Scene | `web/apps/studio/e2e/fixtures/perf_scene.json`: UR5e + ABB IRB 1200 with meshes, a stereo rig on each flange (4 cameras), one chessboard |
| Scenario | `perf_scenario.json`: both robots move in turn, 801 samples at dt 0.01 s, looped |

## Procedure

```bash
cd web/apps/studio && bun run test:perf     # playwright.perf.config.ts → e2e/perf.spec.ts
```

The spec loads the scene by dropping the two fixture files (robots resolve from the
repository library), turns on loop, presses play, and records 600 consecutive animation
frames through an init script that wraps `requestAnimationFrame`:

- **interval** — time between consecutive animation frames: what the viewer sees, and the
  gated number;
- **work** — main-thread time spent in that frame's animation callbacks (R3F render, the
  playback clock, the throttled readouts they trigger).

The run writes `target/perf/g2_2.json`.

## Result

| ms | p50 | p95 | p99 | max |
|---|---|---|---|---|
| frame interval | 6.9 | **7.8** | 8.4 | 20.8 |
| frame work | 0.4 | 0.6 | 0.7 | 1.3 |

The interval sits at the display's 144 Hz period (6.94 ms): playback never drops below the
refresh rate at p95. On a 60 Hz display the interval would be 16.7 ms by vsync alone, so
the work column is the useful headroom measure there.

## Findings

1. **Headless Chromium measures the wrong thing.** Headless, WebGL falls back to
   SwiftShader (CPU): the same run gave an 83.5 ms p95 interval with 0.5 ms of JS work — the
   time is software rasterisation. The perf config therefore runs headed, on the GPU. CI
   runners have no GPU, so G2.2 is a local gate; the e2e suite (SwiftShader) runs in CI.
2. **Per-frame updates never touch React state.** `FrameTree` reads the playhead in
   `useFrame` and copies baked poses into matrices; only the playback bar, the camera
   preview (one kernel projection per target line per frame) and the throttled inspector
   and chart (10 Hz) re-render.
