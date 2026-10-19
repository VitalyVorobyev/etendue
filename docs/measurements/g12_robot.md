# G12.1, G12.2: jogging a robot

Gates of P12 (`docs/pivot/PLAN.md`), measured 2026-10-19 on branch `p12/robot-jog`, Apple M4 Pro.

## G12.1: FK bit parity, native against wasm: PASS

The G0.1 probe evaluates `robotLinkPoses` for the UR5e at 1000 seeded joint vectors, each joint
within 90 % of its range. All 91,000 values are bit-identical natively and in wasm, and so are a
TCP (`robotTcp`) and an IK answer (`robotIk`). See `g0_1_wasm_parity.md` for the fixes:
- `libm` for joint rotations, URDF `rpy` and IK's rotation error;
- in-order sums for `J·Jᵀ` instead of `matrixmultiply`.

G1.1 and G1.2 still pass with the same figures, and the same DLS subset failures as before
(1, 1, 0 and 3 of 400).

## G12.2: IK while dragging the TCP: PASS

```bash
node crates/etendue-wasm/scripts/build-npm.mjs
node crates/etendue-wasm/tests/node/g12_2_jog.mjs
```

The path is a UR5e TCP dragged over 1000 steps of a 150 mm circle in the base's x–y plane, from a
working pose 0.58 m out, toward the base. The tool also turns ±20° about its z axis and ±10°
about x. Each step solves from the previous answer with `restarts: 0`.

| | Measured | Limit |
|---|---|---|
| IK time p50 / p95 / max | 0.009 / **0.012** / 0.47 ms | p95 ≤ 2 ms |
| Converged | **1000 / 1000** | ≥ 99 % |
| Worst residual | 9.9e-11 m, 3.7e-11 rad | 1e-6 m, 1e-6 rad |
| Largest joint step | 3.8 mrad | (continuity) |
| Smallest manipulability | 0.038 | — |

The first draft of the path ran the circle away from the base, out to 0.86 m from the base axis,
beyond the UR5e's reach. 96.6 % of its steps converged, and the rest are unreachable poses. The
studio reports such a step as *Out of reach* and leaves the arm where it was.

## The P8 criteria while jogging: PASS

```bash
cd web/apps/studio && bun x playwright test --config playwright.perf.config.ts perf_jog
```

The test drags the Jog panel's J1 slider for 5 s on `rig3_laser_ur5e`, a three-camera rig on the
flange, in headed Chromium at 120 Hz. Every move runs FK in wasm and the pose overrides of every
link and of what hangs on it.

| | Frames | Bakes | Interval p50 / p95 | Work p50 / p95 |
|---|---|---|---|---|
| Idle | 217 | — | 8.3 / 9.4 ms | 0.4 / 0.6 ms |
| Jog | 602 | **0** | 8.3 / 9.3 ms | 2.4 / **2.9 ms** |

The work limit is G8.1's 4 ms. The jog's interval p95 does not exceed the idle one.

Re-measured 2026-10-19 on `c812423` against G8.1's revised interval criterion (p95 ≤ 1.1× the
idle p95), which `perf_jog.spec.ts` now asserts: idle 239 frames, interval p95 9.3 ms, work p95
0.6 ms; jog 603 frames, 0 bakes, interval p95 **9.3 ms** (≤ 10.2 ms), work p95 **3.1 ms**
(≤ 4 ms). PASS.
