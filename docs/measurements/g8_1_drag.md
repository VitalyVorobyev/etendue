# G8.1, G8.2: live editing

- **Gate G8.1** (`docs/pivot/PLAN.md` P8): during a 5 s slider drag on `rig3_laser_ur5e`:
  - 0 bakes;
  - JS work per frame p95 ≤ 4 ms;
  - frame interval p95 ≤ 1.1× the display period.
- **Gate G8.2:** a commit makes one undo entry and loads no mesh.
- Measured 2026-10-19 on branch `p8b/gizmo` (on top of `7a1334f`, P8a). The host and browser
  are those of G2.2: Apple M4 Pro, Playwright Chromium 153 headed, WebGL on ANGLE Metal, here
  at 120 Hz.

## Procedure

```bash
cd web/apps/studio && bun x playwright test --config playwright.perf.config.ts perf_drag   # G8.1, headed
cd web/apps/studio && bun run test:e2e   # G8.2: "a pose slider moves the camera live…", "the gizmo drags…"
```

`e2e/perf_drag.spec.ts`:
1. Loads the example and selects the rig.
2. Records 2 s idle; the median interval is taken as the display period.
3. Drags the rig's x slider back and forth for 5 s, moving the pointer about every 5 ms.

The work of a frame is all JS since the previous frame:
- animation callbacks;
- pointer and mouse listeners;
- message-port tasks, where React's scheduler renders the slider's updates.

The run writes `target/perf/g8_1.json`.

## G8.1 result: PASS on bakes and work; the interval criterion is mis-set

| | Frames | Bakes | Interval p50 | Interval p95 | Interval p99 | Work p50 | Work p95 | Work p99 |
|---|---|---|---|---|---|---|---|---|
| Idle | 240 | – | 8.3 ms | 9.3 ms | 9.4 ms | 0.4 ms | 0.7 ms | 0.8 ms |
| Drag | 602 | **0** | 8.3 ms | 9.3 ms | 9.4 ms | 0.7 ms | **1.0 ms** | 1.2 ms |

- **Bakes:** 0 during the drag, and 1 on release.
- **Work:** p95 1.0 ms against a 4 ms limit. The single 72 ms frame is the commit's bake on
  release, which falls inside the recording window.
- **Interval: FAIL as written, but the gate is mis-set.** It asks for p95 ≤ 1.1 × the median
  interval (9.13 ms) and measures 9.3 ms. The idle page measures the same 9.3 ms: Chromium's
  rAF timestamps on this display alternate between 8.3 and 9.3 ms. The drag leaves the interval
  distribution exactly as idle.

  The proposed criterion is drag interval p95 ≤ 1.1 × **idle** p95, which measures 9.3 ≤ 10.2
  ms. It is not applied until the user agrees; the spec still asserts the original.

## G8.2 result: PASS

The e2e tests "a pose slider moves the camera live, bakes once on release, and undoes as one
step" and "the gizmo drags the selection live, keeps it selected, and commits one step" check:

- **During the drag:** the world pose moves, there are 0 bakes, and no undo entry yet.
- **On release:** exactly 1 bake, one undo entry, and no `.glb` request after the initial load
  (the mesh cache).
