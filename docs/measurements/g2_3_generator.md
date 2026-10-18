# G2.3: sphere pose generator in wasm

**Gate (P7-6):** the sphere pose generator (`etendue-synth::poses::generate`, through
`@etendue/wasm`'s `generatePoses`) handles 400 candidates seen by three cameras in ≤ 100 ms, the
median of 7 runs. The studio recomputes the candidates on every change of the generator form, on
the main thread. If the gate fails, the plan is to measure first and then move the generator to
a Web Worker.

Measured 2026-10-17 on etendue `f05a157` plus the uncommitted P7 work (branch `p7/authoring`).
Apple M4 Pro, Node 24.20.0 (V8, the same engine as Chromium), release wasm from `build-npm.mjs`.

## Procedure

```bash
node crates/etendue-wasm/scripts/build-npm.mjs
node crates/etendue-wasm/tests/node/g2_3_generator.mjs
```

- **Scene:** `examples/rig3_laser_ur5e`, a ring of three cameras on a UR5e and a 9 × 6 chessboard.
- **Spec:** radii 400–600 mm (5), θ 0.1–0.6 rad (5), φ a full turn (16), no roll: 400
  candidates.
- **Work per candidate:**
  - damped-least-squares IK, seeded from the previous solution, with restarts only on failure;
  - joint-margin and Jacobian checks;
  - FK, then projecting the board's 54 points into every camera.
- **Then:** the tour ordering (nearest neighbour and 2-opt) over the kept poses.

## Result: PASS

| Candidates | Cameras | Kept | Median | Min | Max | Gate |
|---:|---:|---:|---:|---:|---:|---|
| 400 | 3 | 290 | 79.3 ms | 79.1 ms | 106.3 ms | ≤ 100 ms |

Re-run after P7-7 (the same build as the G2.1 re-run, 618 037 B): median 85.0 ms (min 80.1,
max 109.7), **PASS**.

The slowest run is the first, before V8 tiers up the wasm. At 79 ms the studio form stays
responsive, so the generator stays on the main thread. The studio's default spec (48
candidates) takes about 10 ms.

## Native and wasm

The same spec on `examples/rig3_laser_ur5e` gives the same result from `etendue scenario sphere`
(native) and `generatePoses` (wasm):
- the same candidates kept;
- the same tour order;
- bit-identical tool poses.

The stored joint configurations differ by at most 2.9e-15 rad. IK iterates on `sin`/`cos`, and
G0.1 guarantees bit parity for the projection chain only. The committed example scenarios are
the CLI's.
