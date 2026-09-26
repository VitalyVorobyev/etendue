# etendue — Claude Code context

## Commands

```bash
cargo build --workspace                                    # build all crates
cargo run                                                  # launch the (frozen) GUI (blocks until window closed)
cargo test --workspace --locked                            # run all 209 tests
cargo clippy --workspace --all-targets -- -D warnings      # lint (must be clean)
cargo fmt --all                                            # format
cargo fmt --all --check                                    # CI format check
cargo doc --no-deps --workspace                            # build rustdoc

# etendue-wasm (npm @etendue/wasm) — wasm32 target comes from rust-toolchain.toml
cargo clippy -p etendue-wasm --target wasm32-unknown-unknown -- -D warnings
wasm-pack build crates/etendue-wasm --target nodejs --release
node crates/etendue-wasm/tests/node/g01_parity.mjs > target/g01.json
cargo run -p etendue-wasm --example g01_verify -- target/g01.json   # gate G0.1: 0 bit mismatches
```

Do NOT use `cargo run` in a non-interactive context; the window blocks the shell.

### Feature matrix

`--all-features` is run **per crate** (`cargo test -p <crate> --all-features`), never
across the whole workspace.

| Crate | Features | Notes |
|---|---|---|
| `etendue-core` | none | |
| `etendue-wasm` | none | wasm32 build + G0.1 parity in CI job `wasm` |
| `etendue-ui` | none | frozen |
| `etendue-scene` (P1) | `schemars` | JSON Schema derives; CI runs `--all-features` |
| `etendue-kinematics` (P1) | `opw` | analytic OPW IK via `rs-opw-kinematics` (`default-features = false`) |

A crate is added to this table in the same change that creates it.

## Architecture

Three-crate workspace at `/Users/vitalyvorobyev/vision/etendue/`. It is pivoting into a
package family (`docs/pivot/PLAN.md`, `docs/adrs/0001-pivot.md`). New crates are
created phase by phase, never up front.

```
etendue-ui    (binary crate, crates/etendue-ui — FROZEN, publish = false)
etendue-wasm  (cdylib+rlib, crates/etendue-wasm — npm @etendue/wasm, P0 stub API)
    └── etendue-core  (library crate, crates/etendue-core — crates.io)
            └── vision-calibration-core  (crates.io "0.8", patched to ../calibration-rs locally)
```

**`etendue-core`** — concrete-`f64` geometric and physical kernel. Modules:
- `scene` — `CameraEntity`, `LaserEntity`, `TargetEntity`, `Scene`
- `optics::thick_lens` — `ThickLens`, `coc_diameter`, `coc_diameter_px`,
  `sync_intrinsics_from_physical`
- `optics::coc` — Scheimpflug plane-of-best-focus and off-axis CoC
- `laser` — `LaserPlane`, `GaussianBeamWidth`, `stripe_on_target`, `project_stripe`
- `analysis` — `defocus_map`, `working_volume`
- `bank::schema` — `SensorSpec`, `LensSpec`, `LaserSpec` (seed JSON in
  `crates/etendue-core/assets/bank/`)

**`etendue-ui`** — binary `etendue`. Hand-written winit + wgpu + egui-wgpu render loop
(no eframe). Modules: viewport (wgpu pipelines), parameter panel (egui side panel),
simulated-image panel (egui_plot). **Frozen** (ADR 0001): it keeps building and passing
its tests but gets no new features. It is deleted at parity gate G6.3.

**`etendue-wasm`** — wasm-bindgen facade. The P0 surface (`project_points`,
`default_mvp_scene_json`) is a spike for gate G0.1; P2-1 replaces it. Its wasm32-only
`getrandom` 0.3/0.4 `wasm_js` dependencies are backend-selection shims (see its
`Cargo.toml`). Never call entropy from kernel code.

**calibration-rs dependency: crates.io + `[patch.crates-io]`**
- `vision-calibration-core = "0.8"` and `vision-calibration-dataset = "0.8"` come from
  crates.io, so the library crates are publishable.
- The root `Cargo.toml` `[patch.crates-io]` redirects them to
  `../calibration-rs/crates/*` for local development. The sibling checkout must exist
  to build this workspace. Published crates do not need it.
- CI clones calibration-rs at `CALIBRATION_RS_REF` (a tag). Bump that together with the
  version requirement.
- `vision-calibration-dataset` gets its patch line together with its first consumer
  (P1), because an unused patch makes cargo warn on every command.

### nalgebra HARD PIN — DO NOT change

```toml
nalgebra = { version = "0.34", features = ["serde-serialize"] }
```

etendue-core exchanges `Isometry3<f64>`, `Point3<f64>`, `Matrix3<f64>` across the
crate boundary into `vision-calibration-core`. A semver-incompatible second nalgebra
in the tree makes those distinct types and breaks every cross-crate call. The pin must
match calibration-rs exactly.

### Version set (resolved empirically in M0, do not bump without testing)

| Crate | Version |
|---|---|
| egui / egui-wgpu / egui-winit | 0.34 |
| egui_plot | 0.35 |
| wgpu | 29 |
| winit | 0.30 |
| nalgebra | 0.34 |

## Conventions

### Coordinate frames

- **World**: +Z up, right-handed.
- **Camera local**: calibration-rs/OpenCV (+Z forward, +X right, +Y down). Camera pose is
  `world_se3_camera: Isometry3<f64>`.
- Laser and target poses are also world-frame isometries. The existing `pose` fields are
  documented as `world_se3_self`; do not rename them.
- Transform naming follows calibration-rs ADR 0009: `a_se3_b` maps b → a. SE3 wire format
  is `{"rotation": [qx,qy,qz,qw], "translation": [tx,ty,tz]}` (nalgebra `Isometry3`
  serde). The frame tree is defined in `docs/adrs/0002-frame-tree.md`.

### Physical optics as source of truth

Physical optics parameters (focal length mm, f-number, focus distance m, principal-plane
gap mm) are the **source of truth**. `fx`/`fy` in the underlying `CameraModel` are
**derived** via `CameraEntity::sync_intrinsics_from_physical`. Never store the derived
pixel focal lengths as authoritative.

### Serde conventions — mirror calibration-rs exactly

```rust
#[serde(tag = "type", rename_all = "snake_case")]  // on enums
#[serde(flatten)]                                   // on embedded param structs
#[serde(alias = "tau_x")]                           // on renamed fields
```

## Constraints

1. **calibration-rs is a dependency, never a fork or vendor.** Any new functionality
   added to `vision-calibration-core` (e.g. `ThickLens` → `ApertureModel<S>`) is a
   separate upstream PR with explicit user review before merge.

2. **No eframe.** The render loop is a hand-written winit + wgpu event loop integrated
   with egui-wgpu. Do not replace or wrap it with eframe.

3. **Commit only when explicitly asked.** Never commit speculatively.

4. **No speculative scaffolding.** Each milestone creates only what it needs. Do not
   pre-create modules, types, or files for future milestones.

5. **Cargo.lock is committed** (etendue is a binary, not a library). Do not add it to
   `.gitignore`.

6. **`--all-features` per crate only**, following the feature matrix above. Never pass
   it workspace-wide.

7. **Single homes for math.** No FK/IK outside `etendue-kinematics`. No camera-model
   math outside calibration-rs. No rendering code in `etendue-core`.

8. **Every gate result is written to `docs/measurements/` with the commit SHA.** If a
   gate is mis-set, report the measured value and ask. Never relax a gate silently.

9. **`etendue-ui` is frozen** (ADR 0001). Keep it building and green, and add no
   features. It is deleted at parity gate G6.3.

10. **Upstream changes are drafts until the user reviews them.** That covers
    calibration-rs, calib-targets-rs, and lab-ui. Prepare them in a separate worktree or
    branch, and never merge or push them unasked.

## Defocus physics gotchas

These are hard-won lessons from M4; do not break them:

**Gotcha 1**: `ScheimpflugParams::compile()` is a *geometric sensor-plane remap* (it
produces a homography that remaps pixels onto the tilted sensor). It is NOT the defocus
/ focus model. Do not call it to compute CoC or the plane of best focus — those live in
`optics::coc`.

**Gotcha 2**: `CameraParams::build()` compiles the Scheimpflug tilt into a homography
and the resulting `CameraModel` **loses the tilt angles** — they are baked into the
homography matrix and are not recoverable from the built model. Therefore `CameraEntity`
must retain `CameraParams` as the source of truth for tilt. Never reconstruct
`ScheimpflugParams` from a built `CameraModel`.

**Gotcha 3**: The Scheimpflug plane-of-best-focus derivation (see
`docs/derivations/scheimpflug_pobf.md`) produces two distance regimes (a) and (b).
Regime (b) contains a `1/z` term that regime (a) does not. The regime is chosen by the
sign of `z - s_o` where `z` is the depth of the object point and `s_o` is the focus
distance. Mixing regimes silently produces wrong CoC values — the unit tests guard this.

## Quality gates — verify before every report

All must be clean:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo build --workspace
cargo clippy -p etendue-wasm --target wasm32-unknown-unknown -- -D warnings
```

When `etendue-wasm` or anything on the projection path changes, also re-run the G0.1
parity recipe from Commands.

The CI matrix runs these on ubuntu / macos / windows. A clean local run does not
guarantee the Windows build is clean (wgpu backend differs), but it is a necessary
condition.

## Pointers

| Resource | Location |
|---|---|
| Pivot plan (phases P0–P6, gates) | `docs/pivot/PLAN.md` |
| ADRs | `docs/adrs/` |
| Gate measurements | `docs/measurements/` |
| Original design doc (constraints partly superseded by ADR 0001) | `docs/handoff.md` |
| Scheimpflug CoC derivation | `docs/derivations/scheimpflug_pobf.md` |
| mdBook (architecture, chapters) | `book/` |
| Seed component bank | `crates/etendue-core/assets/bank/*.json` |
| calibration-rs source | `../calibration-rs/` |

## Roadmap

`docs/pivot/PLAN.md` is the roadmap, with phases P0–P6 and their gates. ADRs 0001–0006
were accepted on 2026-09-26; P0 is done and P1 is in progress. One item from the old post-MVP queue remains
open: promoting `ThickLens` to `ApertureModel<S>` in calibration-rs, as an upstream PR
with user review.
