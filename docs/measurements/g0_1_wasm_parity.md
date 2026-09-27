# G0.1 — WASM build and wasm/native projection parity

- Gate: **G0.1** (`docs/pivot/PLAN.md`, P0-3)
- Criterion: `vision-calibration-core` and a stub `etendue-wasm` build for
  `wasm32-unknown-unknown` and are callable from Node; projecting points through
  the `Scene::default_mvp()` camera gives the native result **bit-for-bit**.
- Result: **PASS**. 1462 output values, 0 differing bits.
- Measured: 2026-09-26, on etendue commit `56a374f` (clean tree). The result is
  identical to the pre-commit run on the `7511993-dirty` working tree.

## Environment

| Item | Version |
|---|---|
| Host | macOS, Apple silicon (aarch64-apple-darwin) |
| Rust toolchain | 1.95.0 (`rust-toolchain.toml`) |
| calibration-rs | `v0.8.1-1-g9896a990` (sibling checkout via `[patch.crates-io]`, code identical to the v0.8.1 release) |
| wasm-pack | 0.15.0 (`--target nodejs --release`, wasm-opt on) |
| Node | v24.20.0 |

## Procedure

```bash
wasm-pack build crates/etendue-wasm --target nodejs --release
node crates/etendue-wasm/tests/node/g01_parity.mjs > target/g01.json
cargo run -p etendue-wasm --example g01_verify -- target/g01.json
```

- `g01_parity.mjs` builds the probe points in JS: a 9×9×9 lattice filling a
  0.2 m cube around the default target centre `(0, 0, 0.30)`, plus the camera
  centre and a point behind the camera. That is 731 points.
- It fetches the scene with `default_mvp_scene_json()`, runs
  `project_points(scene_json, 0, xyz)` in the wasm module, and writes the inputs
  and outputs as IEEE-754 bit patterns.
- `g01_verify` decodes the exact input bits and runs the same
  `project_points_world` natively on `Scene::default_mvp()`. It then compares
  every output bit.
- The wasm side therefore also covers the scene's JSON round trip, which
  `json_round_trip_is_bit_exact` checks on the native side.

## Result

```text
G0.1 wasm/native projection parity
  points:            731
  imaged:            729
  not imaged (NaN):  2
  output values:     1462
  bit mismatches:    0
  max |Δ| (px):      0e0
  result:            PASS (bit-for-bit)
```

**Negative control:** flipping the last bit of one wasm output value in the
probe file makes the verifier report `bit mismatches: 1` and exit 1.

**Scope of the claim:** the default camera has zero Scheimpflug tilt and no
distortion. The tilt compilation evaluates `sin`/`cos` only at 0, which every
libm returns exactly. Everything else on the projection path uses IEEE-754
correctly-rounded operations (`+ − × ÷ √`), so bit-exactness is expected. Transcendental functions (`sin`,
`cos`, `atan2` in tilted-sensor compilation and in distortion models) go through
the platform libm natively and Rust's `libm` port on wasm32. Those can differ by
an ULP. Any later parity gate that involves tilt or distortion must state its
tolerance explicitly and not assume bit equality.

## Build findings

1. **Upstream `vision-calibration-core` does not build for wasm32 as released
   (v0.8.1).**

   ```text
   $ cargo build -p vision-calibration-core --target wasm32-unknown-unknown   # in calibration-rs
      Compiling getrandom v0.4.3
   error: The wasm32/64-unknown-unknown are not supported by default; you may need
          to enable the "wasm_js" crate feature.
   ```

   The cause is `rand = "0.10"` with default features (`sys_rng`/`thread_rng`),
   which pulls in `getrandom` 0.4. Core only needs seeded `StdRng` (`ransac.rs`).
   `vision-calibration-dataset` builds for wasm32 unchanged.

2. **Upstream fix, drafted and not merged.** The branch `feat/core-wasm32` lives
   in the worktree `../calibration-rs-wasm` and is uncommitted, pending user
   review. It makes these changes:
   - The workspace `rand` gets `default-features = false`.
   - Core adds `features = ["std", "std_rng"]`.
   - `vision-calibration-bench` keeps the old defaults through `thread_rng`.
   - A `wasm32` CI job is added.

   After the change, `getrandom` is gone from core's wasm32 graph. core and
   dataset build for wasm32. calibration-rs's own gates pass:
   - `cargo fmt --check`
   - `clippy --workspace --all-targets --all-features -D warnings`
   - `cargo test --workspace --all-features` (797 passed, 0 failed)
   - `xtask emit-schemas --check`

   `Cargo.lock` is unchanged. `rand` is not in core's public API, so the change
   is semver-neutral.

3. **etendue-side workaround, used for this measurement.** `etendue-wasm`
   declares `getrandom` 0.4 and 0.3 with `wasm_js` as wasm32-only dependencies.
   Through feature unification this selects the JS backend for the whole graph.
   The two versions come from different sources:
   - 0.4 comes via core's `rand` 0.10. It can be dropped once the upstream
     draft ships.
   - 0.3 comes via `etendue-core` → `argmin-math` (`vec` feature) → `rand` 0.9.

   The kernel never draws OS entropy; these shims exist only so the graph
   compiles.

## Size (informational, not a G0.1 criterion)

`etendue_wasm_bg.wasm`, release build, wasm-opt: **227,746 B raw /
87,663 B gzip -9**. G2.1 (≤ 1.5 MB gzipped) applies to the P2-1 API surface,
not this stub.

## Re-run on the P2-1 API (2026-09-27)

P2-1 replaced the stub surface: `default_mvp_scene_json` and the `etendue-core` `Scene`
path are gone. The probe now runs the published npm package (`build-npm.mjs`, `--target
web`, loaded in Node with `initSync`) on `examples/eye_in_hand_ur5e` with its UR5e model,
through `EtendueScene.projectPoints` for **both** cameras, at a fixed camera pose tilted 20°
about world X.

```text
G0.1 wasm/native projection parity
  points per camera: 730
  cam_left           imaged 729, not imaged (NaN) 1
  cam_right          imaged 729, not imaged (NaN) 1
  output values:     2920
  bit mismatches:    0
  max |Δ| (px):      0e0
  result:            PASS (bit-for-bit)
```

- Measured on etendue commit `71e7b8a` (branch `pivot/p2`), calibration-rs `b7e470b2`
  (v0.8.1 + calibration-rs#119).
- Both cameras carry Brown–Conrady distortion (`k1`, `k2` ≠ 0). Forward projection
  evaluates the distortion polynomial only (`+ − × ÷`), so bit equality still holds; the
  scope note above applies to tilt and to anything calling `sin`/`cos`/`atan2`.
- The camera pose crosses the boundary as 7 `f64`s (`Float64Array`), exact by
  construction; the scene crosses as JSON text parsed with `float_roundtrip`.
- The `getrandom` 0.3 shim is gone: `etendue-wasm` no longer depends on `etendue-core`
  (and so not on `argmin-math` → `rand` 0.9). The 0.4 shim stays until etendue requires a
  calibration-rs release that includes calibration-rs#119 (merged 2026-09-27, `b7e470b2`).
