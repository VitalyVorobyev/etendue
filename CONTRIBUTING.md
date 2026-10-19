# Contributing to etendue

This is the developer guide: how the repository is laid out, how to build and test it, and where
the design records live. The user documentation is the book in `book/`, published at
https://vitalyvorobyev.github.io/etendue.

## Layout

| Path | What |
|---|---|
| `crates/etendue-scene` | Versioned JSON documents: scene (frame tree of robots, rigs, cameras, lasers, lights, targets, parts), scenario, baked scenario, robot manifest. No I/O, no kinematics. |
| `crates/etendue-kinematics` | URDF robots: FK, Jacobian, damped-least-squares IK, analytic OPW IK (feature `opw`); scenario compilation and baking. The only home of FK/IK. |
| `crates/etendue-synth` | Synthetic data: canonical render camera and remap LUTs, board geometry (calib-targets' print), analytic ground truth and calibration-rs dataset emission, laser stripes, render jobs, sensor model, EXR/PNG I/O (feature `images`). |
| `crates/etendue-core` | Optics kernel: the physical camera model (mechanical frame, pupils, focus breathing) and its bridge from scenes, thick lens, Scheimpflug circle of confusion, laser plane and beam width, defocus map, working volume, the Scheimpflug design solver. |
| `crates/etendue-cli` | The `etendue` binary and the `etendue_cli` pipeline library (load, gt, render, detect, poses). Embeds the Blender script (`blender/etendue_blender/`). |
| `crates/etendue-wasm` | wasm-bindgen facade, published to npm as `@etendue/wasm`, with a typed TS layer in `js/`. |
| `web/apps/studio` | The studio (React, three.js). `src-tauri/` is its Tauri desktop shell, its own cargo workspace. |
| `xtask` | `cargo xtask check-layering`, `cargo xtask emit-schemas`. |
| `schemas/` | JSON Schemas emitted from `etendue-scene` (CI checks them for drift). |
| `assets/robots/` | Robot descriptions (`robot.urdf`, `robot.json`); meshes are rebuilt by `tools/robot-assets`. |
| `examples/` | Example scenes and scenarios. |
| `tools/` | `robot-assets` (pinned upstream robots → URDF + GLB), `closed-loop` (calibration-rs closed loop), `fixtures` (FK fixtures). |
| `docs/` | Design records: `adrs/`, `pivot/PLAN.md` (the roadmap and its gates), `measurements/` (every gate result, with commit SHA), `derivations/`. |

## Building from source

etendue depends on calibration-rs's `vision-calibration-core` and `-dataset` from crates.io. For
development, `[patch.crates-io]` in the root `Cargo.toml` (and in the Tauri shell's) redirects
them to a sibling checkout, so clone both side by side:

```bash
git clone https://github.com/VitalyVorobyev/calibration-rs
git clone https://github.com/VitalyVorobyev/etendue
git -C calibration-rs checkout v0.9.0      # the tag CI uses (CALIBRATION_RS_REF)
cd etendue
cargo build --workspace
```

The Rust toolchain (with the wasm32 target) comes from `rust-toolchain.toml`. The web side needs
[bun](https://bun.sh) and [wasm-pack](https://rustwasm.github.io/wasm-pack/); the Python tools
need [uv](https://docs.astral.sh/uv/) ≥ 0.13.

```bash
node crates/etendue-wasm/scripts/build-npm.mjs     # @etendue/wasm → crates/etendue-wasm/pkg
cd web && bun install && bun run dev               # the studio in a browser, http://localhost:5178
uv run --locked --project tools/robot-assets tools/robot-assets/build.py   # robot meshes (git-ignored)
cargo app                                          # the desktop app (debug), reading this checkout
cargo app-release                                  # the release bundles (xtask/src/app.rs; meshes, wasm, bun first)
```

## Checks

CI runs these on Linux, macOS and Windows; run them before a pull request:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo clippy -p etendue-wasm --target wasm32-unknown-unknown -- -D warnings
cargo xtask check-layering && cargo xtask emit-schemas --check
cargo test -p <crate> --all-features          # per crate (core, scene, kinematics, synth), never workspace-wide
cargo clippy --manifest-path web/apps/studio/src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path web/apps/studio/src-tauri/Cargo.toml
cd web && bun run generate:types:check && bun run check:deps && bun run typecheck \
  && bun run lint && bun run format:check && bun run test && bun run build \
  && (cd apps/studio && bun run test:e2e)
```

The web toolchain is TypeScript 7, oxlint (type-aware, `web/.oxlintrc.json`) and oxfmt
(`.oxfmtrc.json`); `bun run format` formats. Run
`git config blame.ignoreRevsFile .git-blame-ignore-revs` to skip formatting commits in blame.

Gates that need Blender run locally (the pinned version is in `etendue.toml`). Their commands are
in each `docs/measurements/` record.

## Conventions

- **Frames**: world +Z up, right-handed, metres. Cameras follow calibration-rs/OpenCV (+Z
  forward, +X right, +Y down). Transforms are named `a_se3_b` (maps b → a); the SE3 wire format is
  `{"rotation": [qx, qy, qz, qw], "translation": [x, y, z]}`. See `docs/adrs/0002-frame-tree.md`.
- **One home for each kind of math**: FK/IK in `etendue-kinematics`, camera models in
  calibration-rs, no rendering code in `etendue-core`.
- **nalgebra is pinned** to the version calibration-rs uses (0.34); a second nalgebra in the
  graph breaks every type crossing into `vision-calibration-core`. `cargo xtask check-layering`
  checks it, the crate layering, and that the calibration-rs version is the same in `Cargo.toml`,
  the workflows and `tools/closed-loop`.
- **Determinism**: no OS entropy in kernel code; the wasm graph has no `getrandom`. Randomness is
  seeded.
- **Gates**: every measurement goes to `docs/measurements/` with the commit SHA it was measured
  on. A gate that turns out mis-set is reported as measured, never relaxed silently.
- **Upstream** changes (calibration-rs, calib-targets-rs, lab-ui's `@vitavision/*` packages) are
  pull requests there, not forks or vendored copies.

## Releases

Push a `v*.*.*` tag. `release.yml` runs the checks, builds the `etendue` CLI archives and the
studio bundles (unsigned) for Linux x64, macOS arm64 and Windows x64, and publishes a GitHub
Release with the tag's `CHANGELOG.md` section. Its `workflow_dispatch` trigger is a dry run that
builds everything without publishing.
