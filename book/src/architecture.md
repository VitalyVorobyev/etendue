# Architecture

## Workspace shape

Two crates, one workspace, one binary:

```text
etendue (cargo workspace, resolver = "3", edition = "2024")
├── crates/etendue-core/   # headless f64 kernel  (library)
└── crates/etendue-ui/     # desktop application   (binary "etendue")
```

```text
etendue-ui  ─dep─►  etendue-core  ─path-dep─►  vision-calibration-core
                                                  (at ../calibration-rs)
```

`etendue-core` is the geometric and physical kernel — scene, geometry,
thick-lens optics, laser line model, analysis. `etendue-ui` is the desktop
binary that drives it: a hand-written winit event loop with a wgpu + egui
render stack and the parameter panels.

## The calibration-rs path dependency

etendue depends on the `vision-calibration-core` library from
[calibration-rs] by **path**, not by crates.io:

```toml
vision-calibration-core = { path = "../calibration-rs/crates/vision-calibration-core" }
```

Path because both projects are in active development and the kernel is moving
faster than a crates.io publish cycle is comfortable with. The user's
preferred workflow is: add a `vision-calibration-core` improvement upstream
(in calibration-rs), the local etendue checkout picks it up the next
`cargo build`. No forks. No vendoring.

The `lib.rs` re-exports `vision_calibration_core as calibration`, so callers
inside `etendue-core` and downstream in `etendue-ui` see one identical set of
`nalgebra` types and camera models across the path boundary.

## The nalgebra 0.34 hard pin

`nalgebra` is a **hard pin** at `0.34` in the workspace `Cargo.toml`:

```toml
nalgebra = { version = "0.34", features = ["serde-serialize"] }
```

This pin is load-bearing, not stylistic. `etendue-core` exchanges
`Isometry3<f64>`, `Point3<f64>`, and `Matrix3<f64>` with
`vision-calibration-core` across the path boundary. A semver-incompatible
second `nalgebra` would make those distinct types, breaking every cross-crate
call. Both `etendue-ui` and `etendue-core` therefore re-declare `nalgebra`
through `workspace = true`, sharing the single 0.34 instance in the lock
tree.

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
