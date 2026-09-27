# etendue — Claude Code context

## Commands

```bash
cargo build --workspace                                    # build all crates
cargo run -p etendue-ui                                    # launch the (frozen) GUI (blocks until window closed)
cargo test --workspace --locked                            # run all 270 tests
cargo clippy --workspace --all-targets -- -D warnings      # lint (must be clean)
cargo fmt --all                                            # format
cargo fmt --all --check                                    # CI format check
cargo doc --no-deps --workspace                            # build rustdoc

# etendue-wasm (npm @etendue/wasm) — wasm32 target comes from rust-toolchain.toml
cargo clippy -p etendue-wasm --target wasm32-unknown-unknown -- -D warnings
node crates/etendue-wasm/scripts/build-npm.mjs          # wasm-pack --target web + typed layer → crates/etendue-wasm/pkg
node crates/etendue-wasm/tests/node/g01_parity.mjs > target/g01.json
cargo run -p etendue-wasm --example g01_verify -- target/g01.json   # gate G0.1: 0 bit mismatches
gzip -9c crates/etendue-wasm/pkg/etendue_wasm_bg.wasm | wc -c       # gate G2.1: ≤ 1572864

# web/ (bun workspace; build @etendue/wasm first — it is a workspace member)
cd web && bun install
bun run generate:types        # crates/etendue-wasm/js/types from schemas/ (CI: generate:types:check)
bun run check:deps            # web layering rules
bun run typecheck && bun run lint && bun run test && bun run build
bun run dev                   # studio at http://localhost:5178 (blocks; do not run non-interactively)
cd apps/studio && bun run test:e2e     # Playwright, SwiftShader, bakes the reference with etendue-cli
cd apps/studio && bun run test:perf    # gate G2.2, headed Chromium on the GPU (local only)

# Scene tooling (etendue-cli binary is `etendue`)
cargo run -p etendue-cli -- validate examples/eye_in_hand_ur5e/scene.json examples/eye_in_hand_ur5e/scenario.json
cargo run -p etendue-cli -- bake <scene.json> <scenario.json> -o target/baked.json
cargo run --release -p etendue-cli -- render <scene.json> <scenario.json> -o target/render --samples 16   # Blender, local only
python3 -m unittest discover -s crates/etendue-cli/blender/tests                                          # Blender conventions
cargo run --release -p etendue-cli -- measure g4-1 -o target/g4_1                                          # gate G4.1, Blender side
cd web/apps/studio && bun x vitest run --project browser                                                  # gate G4.1, web side

# Workspace policy (CI job `checks`)
cargo xtask check-layering                                 # ADR 0001 dependency rules + single nalgebra
cargo xtask emit-schemas [--check]                         # schemas/*.schema.json from etendue-scene

# Kinematics gates
cargo test -p etendue-kinematics --all-features --test gates -- --nocapture   # G1.1, G1.2 (incl. OPW)
cargo run --release -p etendue-kinematics --example g1_2_ik                   # full G1.2 DLS report

# Robot assets and FK fixtures (Python via uv; see tools/robot-assets/robots.toml)
uv run --locked --project tools/robot-assets tools/robot-assets/build.py --report docs/measurements/g1_3_robot_assets.md
uv run tools/fixtures/fk_fixture.py
```

Do NOT use `cargo run -p etendue-ui` in a non-interactive context; the window blocks the
shell. The UI binary is `etendue-ui`; the `etendue` binary is the CLI.

### Feature matrix

`--all-features` is run **per crate** (`cargo test -p <crate> --all-features`), never
across the whole workspace.

| Crate | Features | Notes |
|---|---|---|
| `etendue-core` | none | |
| `etendue-scene` | `schemars` | JSON Schema derives (xtask enables it to emit `schemas/`) |
| `etendue-kinematics` | `opw` | analytic OPW IK via `rs-opw-kinematics` (`default-features = false`) |
| `etendue-synth` | `images` | canonical camera, remap LUT, GT, dataset, render jobs; `images` = EXR ingest + PNG (native render path, off in wasm) |
| `etendue-cli` | none | binary `etendue` |
| `etendue-wasm` | none | wasm32 build + G0.1 parity in CI job `wasm` |
| `etendue-ui` | none | frozen, binary `etendue-ui` |
| `xtask` | none | not published |

CI job `checks` runs clippy and tests with `--all-features` for `etendue-scene`,
`etendue-kinematics` and `etendue-synth`, and the Blender script's pure-Python tests.

A crate is added to this table in the same change that creates it.

## Architecture

Workspace at `/Users/vitalyvorobyev/vision/etendue/`. It is pivoting into a package
family (`docs/pivot/PLAN.md`, `docs/adrs/0001-pivot.md`). New crates are created phase
by phase, never up front. P0 and P1 are done.

```
etendue-cli (bin `etendue`) ──► etendue-kinematics ──► etendue-scene ──► vision-calibration-{core,dataset}
etendue-ui  (bin `etendue-ui`, FROZEN) ──► etendue-core ──► vision-calibration-core
etendue-wasm (npm @etendue/wasm) ──► etendue-kinematics ──► etendue-scene, etendue-synth ──► vision-calibration-core
web/apps/studio ──► @etendue/wasm, @vitavision/{ui,stage2d,charts} (npm), web/packages/* (incubating)
xtask (emit-schemas, check-layering)
```

`cargo xtask check-layering` enforces these rules (ADR 0001 §2).

**`etendue-core`** — concrete-`f64` geometric and physical kernel. Modules:
- `scene` — `CameraEntity`, `LaserEntity`, `TargetEntity`, `Scene`
- `optics::thick_lens` — `ThickLens`, `coc_diameter`, `coc_diameter_px`,
  `sync_intrinsics_from_physical`
- `optics::coc` — Scheimpflug plane-of-best-focus and off-axis CoC
- `laser` — `LaserPlane`, `GaussianBeamWidth`, `stripe_on_target`, `project_stripe`
- `analysis` — `defocus_map`, `working_volume`
- `bank::schema` — `SensorSpec`, `LensSpec`, `LaserSpec` (seed JSON in
  `crates/etendue-core/assets/bank/`)

**`etendue-scene`** — versioned JSON documents: `SceneSpec` (frame tree of robots, rigs,
cameras, lasers, lights, targets, parts; ADR 0002), `ScenarioSpec`, `BakedScenario`
(ADR 0003), `RobotManifest` (`robot.json`). `FrameGraph` resolves the attachment tree.
It does no I/O and no kinematics. Schemas are committed in `schemas/`, and CI checks
them for drift.

**`etendue-kinematics`** — the only home of FK and IK. `RobotModel` is a URDF (via
`urdf-rs`) bound to a manifest. It provides FK, the geometric Jacobian, damped-least-
squares IK with deterministic restarts, and analytic OPW IK behind `opw`. `compile`/`bake`
turn a scenario into a trajectory or baked scenario: synchronised trapezoidal PTP,
spline-timed LIN, and stop-and-shoot captures.

**Robot assets** — `assets/robots/<id>/{robot.urdf, robot.json, LICENSE}` are committed.
`meshes/*.glb` are git-ignored and regenerated byte-identically by
`tools/robot-assets/build.py` from pinned upstream SHAs. The robot base is the REP-199
`base` link, i.e. the controller frame (ADR 0002). Pinocchio FK fixtures for G1.1 are in
`tools/fixtures/fk/`.

**`etendue-synth`** — synthetic-image support (P3). `remap`: `CanonicalCamera::cover`
chooses the canonical render pinhole for a target camera, `remap_lut` tabulates
`canonical.project(target.backproject(u))` (ADR 0004). The pixel-centre convention is an
explicit `PixelCentre` argument until probe P4-2 decides it. Gate G3.1:
`cargo run --release -p etendue-synth --example g3_1_remap` (open upstream,
calibration-rs#120).

**Blender backend** (ADR 0005, P4) — `etendue render` writes `job.json` (etendue-synth
`job`: meshes, boards, lights, canonical cameras, per-capture `world_se3_frame`), runs the
embedded bpy-only script `crates/etendue-cli/blender/etendue_blender/render.py` (Cycles,
fixed seed, Standard view, multilayer EXR with Depth and IndexOB), then remaps each EXR
through the LUT to `images/<camera>/<capture>.png`. Conventions live only in
`blender/etendue_blender/convert.py` (Rx(π) for cameras and emitters, Rx(−π/2) undoing the
glTF importer's Y-up). The Blender version is pinned in `etendue.toml` (5.1.1). Blender runs
locally, never in CI.

**`etendue-ui`** — binary `etendue-ui`. Hand-written winit + wgpu + egui-wgpu render loop
(no eframe). Modules: viewport (wgpu pipelines), parameter panel (egui side panel),
simulated-image panel (egui_plot). **Frozen** (ADR 0001): it keeps building and passing
its tests but gets no new features. It is deleted at parity gate G6.3.

**`etendue-wasm`** — wasm-bindgen facade (P2-1). `Session` (JS `EtendueScene`) loads a
scene plus robot sources `{id, manifest, urdf}` (the host reads files; the crate does no
I/O), then `bake`, `project_points`, `backproject_pixels`, `target_extent`, `remap`. Documents cross
as JSON text (bit-exact with `float_roundtrip`). `scripts/build-npm.mjs` wraps the
wasm-pack output with the typed layer in `js/` (types generated from `schemas/` by
`web/scripts/generate-wasm-types.ts`). The wasm32-only `getrandom` 0.4 `wasm_js`
dependency is a backend-selection shim until the `vision-calibration-*` requirement moves
to a release with calibration-rs#119. Never call entropy from kernel code.

**`web/`** — bun workspace (lab-ui toolchain: `@vitavision/config-{ts,eslint,vitest}`, TS
6.0.3, Tailwind v4, tokens only). `apps/studio` is the P2-5 studio (not published).
`packages/*` are `private` `@vitavision/*` packages **incubating** here and moved to lab-ui
by PR (user decision): `three` (no React, no kinematics, no camera math), `three-react`
(R3F; per-frame work in `useFrame`, never React state), `workbench` (studio shell pieces,
playhead store), `ui-next` (additions to `@vitavision/ui`). They follow lab-ui's Definition
of Done so the move is a copy. `scripts/check-deps.ts` enforces their layering.

**calibration-rs dependency: crates.io + `[patch.crates-io]`**
- `vision-calibration-core = "0.8"` and `vision-calibration-dataset = "0.8"` come from
  crates.io, so the library crates are publishable.
- The root `Cargo.toml` `[patch.crates-io]` redirects them to
  `../calibration-rs/crates/*` for local development. The sibling checkout must exist
  to build this workspace. Published crates do not need it.
- CI clones calibration-rs at `CALIBRATION_RS_REF` (a tag). Bump that together with the
  version requirement.
- Both calibration-rs crates have a `[patch]` line. Do not add a patch line for a crate
  nothing uses yet: cargo warns about unused patches on every command.
- `serde_json` has `float_roundtrip` enabled workspace-wide, because the default parser
  can be off by one ULP. Keep it: bit-exact round trips of poses and ground truth depend
  on it.

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
# when web/ or etendue-wasm changes (after build-npm.mjs):
cd web && bun run generate:types:check && bun run check:deps && bun run typecheck \
  && bun run lint && bun run test && bun run build && (cd apps/studio && bun run test:e2e)
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
were accepted on 2026-09-26. P0 and P1 are done, with gates G0.1 and G1.1–G1.3 recorded
in `docs/measurements/`. P2 (web packages) is next.

One item from the old post-MVP queue remains open: promoting `ThickLens` to
`ApertureModel<S>` in calibration-rs, as an upstream PR with user review.
