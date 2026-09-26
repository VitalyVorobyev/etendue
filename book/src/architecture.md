# Architecture

## Workspace shape

The pivot (`docs/adrs/0001-pivot.md`, `docs/pivot/PLAN.md`) adds crates phase by
phase. After P1:

```text
etendue (cargo workspace, resolver = "3", edition = "2024")
├── crates/etendue-core/        # headless f64 optics kernel          (library, crates.io)
├── crates/etendue-scene/       # scene/scenario/baked JSON schema    (library, crates.io)
├── crates/etendue-kinematics/  # URDF FK, IK, scenario baking        (library, crates.io)
├── crates/etendue-cli/         # `etendue validate | bake`           (binary "etendue")
├── crates/etendue-wasm/        # wasm-bindgen facade, P0 stub        (npm @etendue/wasm)
├── crates/etendue-ui/          # desktop application, frozen         (binary "etendue-ui")
└── xtask/                      # emit-schemas, check-layering
```

```text
etendue-ui   ─dep─┐
                  ├─►  etendue-core  ─dep─►  vision-calibration-core (crates.io 0.8)
etendue-wasm ─dep─┘                            ↳ [patch.crates-io] → ../calibration-rs
```

`etendue-core` is the geometric and physical kernel — scene, geometry,
thick-lens optics, laser line model, analysis. `etendue-ui` is the desktop
binary that drives it: a hand-written winit event loop with a wgpu + egui
render stack and the parameter panels. `etendue-ui` is frozen by the web
pivot (`docs/adrs/0001-pivot.md`) and is never published.

## The calibration-rs dependency

etendue depends on the `vision-calibration-core` library from
[calibration-rs] through **crates.io**, so every etendue library crate can be
published:

```toml
[workspace.dependencies]
vision-calibration-core = "0.8"

[patch.crates-io]
vision-calibration-core = { path = "../calibration-rs/crates/vision-calibration-core" }
```

The `[patch.crates-io]` entry redirects the dependency to a sibling
calibration-rs checkout for local development. The user's workflow is
unchanged: add a `vision-calibration-core` improvement upstream (in
calibration-rs), and the local etendue checkout picks it up on the next
`cargo build`. No forks. No vendoring. `cargo package` / `cargo publish`
ignore patches, so a published `etendue-core` depends on the crates.io
release only.

Because a `[patch]` path must exist, building *this workspace* still needs the
sibling checkout. CI clones calibration-rs at the tag matching the version
requirement (`CALIBRATION_RS_REF` in the workflows), so CI does not drift
when calibration-rs `main` moves.

The `lib.rs` re-exports `vision_calibration_core as calibration`, so callers
inside `etendue-core` and downstream in `etendue-ui` see one identical set of
`nalgebra` types and camera models across the crate boundary.

## The nalgebra 0.34 hard pin

`nalgebra` is a **hard pin** at `0.34` in the workspace `Cargo.toml`:

```toml
nalgebra = { version = "0.34", features = ["serde-serialize"] }
```

This pin is load-bearing, not stylistic. `etendue-core` exchanges
`Isometry3<f64>`, `Point3<f64>`, and `Matrix3<f64>` with
`vision-calibration-core` across the crate boundary. A semver-incompatible
second `nalgebra` would make those distinct types, breaking every cross-crate
call. Both `etendue-ui` and `etendue-core` therefore re-declare `nalgebra`
through `workspace = true`, sharing the single 0.34 instance in the lock
tree.

## The WebAssembly build

`etendue-wasm` compiles the kernel and `vision-calibration-core` for
`wasm32-unknown-unknown`. `rust-toolchain.toml` lists the target, so rustup
installs it automatically. Two build facts matter:

- Two `getrandom` versions reach the wasm32 graph. Version 0.4 comes via
  calibration-rs's `rand` 0.10, and 0.3 comes via `argmin-math`'s `rand` 0.9.
  `getrandom` refuses to build there without a JS backend, so `etendue-wasm`
  enables `wasm_js` on both, as wasm32-only dependencies. Feature unification
  then applies it across the graph. The kernel never draws OS entropy; the
  shim only makes the graph compile. An upstream calibration-rs change that
  removes `getrandom` from core is drafted.
- Gate G0.1 checks that the wasm build under Node and the native build project
  points through the default camera **bit-for-bit** identically
  (`docs/measurements/g0_1_wasm_parity.md`). CI's `wasm` job re-runs the
  check on every push.

```bash
wasm-pack build crates/etendue-wasm --target nodejs --release
node crates/etendue-wasm/tests/node/g01_parity.mjs > target/g01.json
cargo run -p etendue-wasm --example g01_verify -- target/g01.json
```

## Version set (UI stack)

The UI stack pins are equally deliberate — the egui ecosystem moves quickly
and the integration crates require matching minor versions:

| crate            | version | notes                                  |
|------------------|---------|----------------------------------------|
| `egui`           | 0.34.2  | immediate-mode UI                      |
| `egui-wgpu`      | 0.34.2  | wgpu integration for egui              |
| `egui-winit`     | 0.34.2  | winit integration for egui             |
| `egui_plot`      | 0.35    | versions independently; targets 0.34   |
| `wgpu`           | 29.0.3  | exact version egui-wgpu 0.34 declares  |
| `winit`          | 0.30.13 | exact version egui-winit 0.34 declares |
| `pollster`       | 0.4     | block on wgpu device-creation futures  |

These versions were resolved empirically in M0 against the exact deps
egui-wgpu / egui-winit 0.34 declare, not guessed from changelogs.

## Edition and resolver

Edition **2024** workspace-wide; resolver **3**. Both crates inherit them
through `edition.workspace = true`.

## The kernel is concrete f64

calibration-rs's traits are generic over `S: RealField + Copy`. etendue
deliberately is **not**: `etendue-core` uses concrete `f64` throughout — no
`S: RealField` parameters on any kernel type. The lib.rs is explicit:

> The numeric type is concrete `f64` throughout this crate; there is no
> `S: RealField` genericity.

The reuse from calibration-rs is the **f64-locked** Camera / CameraModel
projection chain (`vision_calibration_core::Camera`, `CameraParams`,
`ScheimpflugParams`). Carrying the generic up into etendue's kernel would buy
no portability — the application is a desktop tool, not a `no_std`
arithmetic library — and would inflict generic-parameter noise on every
optics signature. The trade is recorded in the lib.rs module doc-comment.

## Coordinate conventions

The world frame is **right-handed, +Z up**. The viewport orbit camera and
the ground grid (z = 0) match it; every entity carries an `Isometry3<f64>`
pose that maps its local frame into this world frame.

Camera-local follows the calibration-rs convention: **+z forward** (toward
the object), +x right in the image, +y down. `Camera::project_point_c`
rejects camera-frame `z ≤ 0`; `backproject_pixel` returns a ray as a point on
the local z = 1 plane. A `CameraEntity::pose` is therefore "world ←
camera-local".

These conventions are documented in `scene/entity.rs` and `scene/scene.rs`
and exercised by the default-MVP unit tests (camera optical axis points at
the target centre; triangulation angle is in the 10°–45° range; ...).

## Build profile

```toml
[profile.dev]
debug = 1

[profile.dev.package."*"]
opt-level = 2
```

`debug = 1` keeps debug-build link times reasonable on the dev machine
(line-table debug info, not the full DWARF). `opt-level = 2` on all
dependencies — the dev profile only optimises etendue's own crates at `0`,
which keeps the inner-loop iteration fast while leaving wgpu, egui, and
nalgebra at speed. The user has this pattern globally in
`~/.cargo/config.toml`; the workspace `Cargo.toml` echoes it so a fresh
checkout matches.

[calibration-rs]: https://github.com/VitalyVorobyev/calibration-rs
