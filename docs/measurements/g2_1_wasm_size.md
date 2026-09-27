# G2.1 — `@etendue/wasm` size

- Gate: **G2.1** (`docs/pivot/PLAN.md`, P2-1)
- Criterion: the release `.wasm` of `@etendue/wasm` is **≤ 1.5 MB gzipped** (1,572,864 B).
- Result: **PASS**. 310,354 B gzipped (20% of the budget).
- Measured: 2026-09-27, on etendue commit `71e7b8a` (branch `pivot/p2`).

## Environment

| Item | Version |
|---|---|
| Host | macOS, Apple M4 Pro (aarch64-apple-darwin) |
| Rust toolchain | 1.95.0 (`rust-toolchain.toml`) |
| calibration-rs | v0.8.1 + calibration-rs#119 (`b7e470b2`, sibling checkout via `[patch.crates-io]`) |
| wasm-pack | 0.15.0 (`--target web --release`, wasm-opt on) |

## Procedure

```bash
node crates/etendue-wasm/scripts/build-npm.mjs
gzip -9c crates/etendue-wasm/pkg/etendue_wasm_bg.wasm | wc -c
```

CI job `wasm` runs the same check on every push (`test "$size" -le 1572864`).

## Result

| Artifact | Bytes |
|---|---|
| `etendue_wasm_bg.wasm`, raw | 858,511 |
| gzip -9 | **310,354** |
| brotli -q 11 (informational) | 236,733 |

What is in it: `etendue-scene` (validation, frame graph), `etendue-kinematics` without `opw`
(URDF parsing via `urdf-rs`, FK, DLS IK, the scenario compiler), `vision-calibration-core`
camera models, and `serde_json`. The P0 stub was 87,663 B gzipped; the growth is kinematics
and URDF parsing (`urdf-rs` brings an XML parser and `regex`).

Headroom for what P3 adds (`remap_lut` through `etendue-synth`) and P6 (the etendue-core
analyses) is about 1.26 MB gzipped.

## After P3-1 (`remap`, etendue-synth)

318,694 B gzipped (+8,340 B), measured on commit `39d1f86` (branch `pivot/p3-synth`). Still
20 % of the budget.

## Re-measured on calibration-rs 0.8.2 (2026-09-27)

**319,254 B** gzipped (20 % of the budget), up from 318,133 B at P3-1. The increase comes
from the Newton undistortion (analytic Jacobians) in `vision-calibration-core`. The
`getrandom` shim is gone: nothing in the wasm graph draws entropy. Measured on etendue commit
`COMMIT` (branch `pivot/p4-corners`), calibration-rs `v0.8.2` (`ce883adf`).
