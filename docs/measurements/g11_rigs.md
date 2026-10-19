# G11.1, G11.2, G11.3: rigs and their coverage

Gates of P11 (`docs/pivot/PLAN.md`), measured 2026-10-19 on branch `p11a/rig-kernel` (on
`0b01fed`), Apple M4 Pro.

## G11.1: a two-camera `common` layout is a stereo pair: PASS

`RigLayout { count: 2, image_x: "common" }` puts the cameras at ±ρ on the rig's x axis with
image +X along it.

```bash
cargo test -p etendue-scene --all-features --lib stereo
```

Results, for the converged pair (φ = atan(ρ/d)) and the parallel pair (φ = 0):
- The baseline is 2ρ and lies along the rig's x axis to 1e-12.
- The image +Y axes are equal to 1e-12, so the rows line up.
- Image +X lies in the baseline plane, pointing along +X.
- For the parallel pair, the two image +X axes are equal to 1e-12.

## G11.2: overlap against the analytic frusta: PASS

A parallel pinhole pair has baseline B = 0.1 m, fx = 1000 px and a 1000 × 800 px image. Its
fields at depth z are a·z by c·z, with a = 1 and c = 0.8. Between depths z₀ and z₁, both cameras
see ∫ (a·z − B)·c·z dz.

```bash
cargo test -p etendue-core --release --test rig_coverage -- --nocapture
```

| Grid | Box | Coverage, 2 views | Analytic | Error | Limit |
|---|---|---|---|---|---|
| 128³ | x ±0.3, y ±0.24, z 0.2–0.6 m | 0.042957 m³ | 0.042667 m³ | **0.68 %** | 2 % |

## G11.3: coverage of 64³ in wasm: PASS

The rig is a ring of three heads:
- IMX174 + LM25HC at f/4, from `examples/library`;
- the StingRay laser on each camera's body, 100 mm off its axis;
- an 80 mm ring converging at 0.35 m.

The box is ±150 mm wide and 0.2–0.5 m deep, the blur limit 3 px, and each time is the median of 5
runs.

```bash
node crates/etendue-wasm/scripts/build-npm.mjs
node crates/etendue-wasm/tests/node/g11_3_coverage.mjs
```

| Grid | Time | Budget |
|---|---|---|
| 64³ | **12.7 ms** | 100 ms |
| 32³ | 1.7 ms | — |

On the 64³ grid, 1154 voxels are seen by all three cameras and 1974 are measured by a pair.

## Parity and size

- **G0.1:** the probe adds the ring's coverage on 20 × 20 × 16 (12,800 counts). Of 18,524
  values, **0 differ** between native and wasm.
- **G2.1:** `etendue_wasm_bg.wasm` is **689,423 B** gzipped (limit 1,572,864).
