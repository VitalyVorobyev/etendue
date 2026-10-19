# P8 baseline: what one inspector edit cost before live editing

**Purpose (P8, first step):** measure the edit path before it changes, so gates G8.1 and G8.2
compare against a recorded number.

Measured 2026-10-19 on etendue `6875020` (main, before P8), Apple M4 Pro, Node 24.20.0 (V8, the
engine Chromium uses), release wasm from `build-npm.mjs`.

## Procedure

```bash
node crates/etendue-wasm/scripts/build-npm.mjs
node crates/etendue-wasm/tests/node/p8_rebake_baseline.mjs
```

- **Scene:** `examples/rig3_laser_ur5e` with its scenario: 5229 samples of 22 frames, a UR5e.
- **The edit:** move the board by 1 mm, then run what the studio ran for every keystroke:
  `updateScene` and `bake` (with joint derivatives, as the Motion charts need). The median of
  9 runs is reported.

## Result

| Step | Median |
|---|---|
| `updateScene` (parse, validate) | 0.16 ms |
| `bake` (5229 samples) | 85.7 ms |
| **One edit** | **85.8 ms** |

The bake is not the whole cost. Each bake was a new object, so three-react's `FrameTree` built a
new `FrameTreeRuntime`, and `Robot` (whose load effect depends on the runtime) downloaded and
parsed all 7 UR5e link meshes again. A slider moving at 60 Hz would have queued a bake and 7
mesh fetches every frame, at about five times the frame budget.

## After (P8, PR "live editing")

- **A move** of a placed entity is previewed with pose overrides on the bake: no kernel call.
  The release bakes once.
- **A parameter edit** that moves nothing (intrinsics, a laser's fan, a light) replaces the
  kernel's scene and keeps the bake object, so no runtime and no mesh is rebuilt.
- **A commit** that bakes builds a new runtime, but the meshes come from a per-URL cache
  (clones, no download).

Checked by the e2e test "a pose slider moves the camera live, bakes once on release, and undoes
as one step":
- no bake mark during the drag, exactly one after the release;
- one undo step;
- no `.glb` request after the initial load.

The frame-time gate (G8.1) is measured headed, like G2.2, in the PR that adds it.
