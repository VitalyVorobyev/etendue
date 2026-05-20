# etendue — Claude Code context

## Commands

```bash
cargo build --workspace                                    # build both crates
cargo run                                                  # launch the GUI (blocks until window closed)
cargo test --workspace                                     # run all 172 tests
cargo clippy --workspace --all-targets -- -D warnings      # lint (must be clean)
cargo fmt --all                                            # format
cargo fmt --all --check                                    # CI format check
cargo doc --no-deps --workspace                            # build rustdoc
```

Do NOT use `--all-features` — etendue has no feature flags. Do NOT use `cargo run` in
a non-interactive context; the window blocks the shell.

## Architecture

Two-crate workspace at `/Users/vitalyvorobyev/vision/etendue/`:

```
etendue-ui  (binary crate, crates/etendue-ui)
    └── etendue-core  (library crate, crates/etendue-core)
            └── vision-calibration-core  (path dep, ../calibration-rs/crates/vision-calibration-core)
```

**`etendue-core`** — concrete-`f64` geometric and physical kernel. Modules:
- `scene` — `CameraEntity`, `LaserEntity`, `TargetEntity`, `Scene`
- `optics::thick_lens` — `ThickLens`, `coc_diameter`, `coc_diameter_px`,
  `sync_intrinsics_from_physical`
- `optics::coc` — Scheimpflug plane-of-best-focus and off-axis CoC
- `laser` — `LaserPlane`, `GaussianBeamWidth`, `stripe_on_target`, `project_stripe`
- `analysis` — `defocus_map`, `working_volume`
- `bank::schema` — `SensorSpec`, `LensSpec`, `LaserSpec` (9 seed JSON files in
  `assets/bank/`)

**`etendue-ui`** — binary `etendue`. Hand-written winit + wgpu + egui-wgpu render loop
(no eframe). Modules: viewport (wgpu pipelines), parameter panel (egui side panel),
simulated-image panel (egui_plot).

**Path dependency constraint**: `vision-calibration-core` is at
`../calibration-rs/crates/vision-calibration-core` relative to the workspace root.
Both repos must be siblings under the same parent directory.

### nalgebra HARD PIN — DO NOT change

```toml
nalgebra = { version = "0.34", features = ["serde-serialize"] }
```

etendue-core exchanges `Isometry3<f64>`, `Point3<f64>`, `Matrix3<f64>` across the
path-dep boundary into `vision-calibration-core`. A semver-incompatible second nalgebra
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
- **Camera local**: +Z forward (calibration-rs convention). Camera pose is
  `world_from_camera: Isometry3<f64>`.
- Laser and target poses are also world-frame isometries.

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

6. **No `--all-features`** in any command — the workspace has no feature flags.

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

All four must be clean:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
```

The CI matrix runs these on ubuntu / macos / windows. A clean local run does not
guarantee the Windows build is clean (wgpu backend differs), but it is a necessary
condition.

## Pointers

| Resource | Location |
|---|---|
| Original design doc | `docs/handoff.md` |
| Scheimpflug CoC derivation | `docs/derivations/scheimpflug_pobf.md` |
| mdBook (architecture, chapters) | `book/` |
| Seed component bank | `assets/bank/*.json` |
| calibration-rs source | `../calibration-rs/` |

## Post-MVP queue (priority order)

1. Promote `ThickLens` to `ApertureModel<S>` in calibration-rs — upstream PR, user
   review required.
2. Voxelized working volume — per-voxel predicates: visible, illuminated, triangulation
   angle, resolution.
3. Multi-camera overlap — N-view visible-voxel intersection.
4. Component-picker UI — browse `assets/bank/`, drag onto scene.
5. `argmin`-based optimizer — optimize focal length / f-number / tilt / baseline for a
   given working-distance + depth-range spec.
6. Gaussian PSF — replace geometric CoC with a depth-dependent Gaussian blur kernel.
7. Mesh laser intersection — replace the current plane∩plane with full triangle-mesh
   intersection for non-planar targets.
