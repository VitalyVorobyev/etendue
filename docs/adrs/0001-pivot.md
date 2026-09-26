# ADR 0001: Pivot to a Family of Independently Published Packages

- Status: Accepted
- Date: 2026-09-26 (accepted 2026-09-26)

## Context

etendue v0.1.0 has two parts:

- `etendue-core`, a concrete-`f64` optics/laser/analysis kernel;
- `etendue-ui`, a hand-written winit + wgpu + egui desktop app.

Milestones M0–M10 and the post-MVP items are done. The original handoff
(`docs/handoff.md`) set these constraints:

- "Pure Rust front-to-back. No Tauri / React / Vite / Bun."
- "No Blender dependency."
- Explicitly out of scope: "Realistic synthetic image rendering: path tracing,
  BRDFs, speckle, full sensor noise stack."

`book/src/introduction.md` repeats the last one.

The next set of needs does not fit a single desktop binary:

1. Scenes with real robots (URDF), calibrated multi-camera rigs attached by
   hand-eye poses, lasers, lights, targets, and robot motion scenarios.
2. Web 3D visualisation shared with the calibration-rs `calibration-diagnose`
   app, which is Tauri + React + React Three Fiber.
3. Synthetic images with exact ground truth, consumed directly by calibration-rs
   as datasets.
4. The existing design analyses (defocus map, working volume, triangulation
   angle), shown as overlays in that web scene.

**Primary consumer requirement:** the packages are used in the user's
professional work outside etendue. That means no monolith. Every package must
build, test, and publish on its own, and none may depend on the studio app.

## Decision

### 1. Package family

The `docs/pivot/PLAN.md` §2 tree is created phase by phase, never up front.

| Package | Phase | Published to | Role |
|---|---|---|---|
| `etendue-core` | existing | crates.io | Optics / laser / analysis kernel. No rendering code. |
| `etendue-scene` | P1 | crates.io | Versioned schema: frame tree, entities, robots, scenarios, lights, render jobs, baked scenarios ([ADR 0002](0002-frame-tree.md), [ADR 0003](0003-baking.md)). |
| `etendue-kinematics` | P1 | crates.io | URDF → chain, FK, IK, scenario baking. The only home of FK/IK. |
| `etendue-synth` | P3 | crates.io | Canonical render camera, remap LUTs, analytic GT, sensor noise, DatasetSpec emission, EXR ingest ([ADR 0004](0004-canonical-render-camera.md), [ADR 0006](0006-ground-truth.md)). |
| `etendue-cli` | P1 | crates.io (binary) | `etendue bake / render / gt / validate`. Embeds the Blender script ([ADR 0005](0005-blender-renderer.md)). |
| `etendue-wasm` | P0 stub, P2 API | npm `@etendue/wasm` only | wasm-bindgen facade. `publish = false` on crates.io. |
| `@vitavision/three`, `@vitavision/three-react` | P2 | npm, from the **lab-ui** repo (ticket L8-1) | Reusable three.js / R3F scene components. |
| `web/apps/studio` | P2 | not published | Vite app that wires the packages together. |
| `etendue-ui` | frozen | never (`publish = false`) | Kept building and green. Deleted at parity gate G6.3. |

### 2. Dependency rules

CI enforces these with a `cargo metadata` layering check, starting from the
first phase that has two library crates to check.

- `etendue-scene` may depend on nalgebra, serde, schemars (feature),
  `vision-calibration-core`, and `vision-calibration-dataset`. It depends on
  no other etendue crate.
- `etendue-kinematics` may depend on `etendue-scene` and `urdf-rs`.
  `rs-opw-kinematics` sits behind the `opw` feature. The `k` crate is excluded
  because it requires nalgebra ^0.30, which conflicts with the 0.34 hard pin.
- `etendue-synth` may depend on `etendue-scene`, `etendue-kinematics`,
  `vision-calibration-core`, `vision-calibration-dataset`, and `exr`.
- `etendue-core` may depend on `etendue-scene` (to build a `Scene` from a
  spec). The reverse is forbidden.
- `etendue-wasm` may depend on any library crate and on no binary crate.
- `@vitavision/three-react` → `@vitavision/three` → `three`.
  - `@vitavision/three` never imports React.
  - Neither package depends on `@etendue/wasm`. Kernel calls (bake, LUT,
    projection) are injected through a small TypeScript interface that the
    studio app wires to `@etendue/wasm`.

### 3. Rendering scope

Realistic synthetic rendering comes **into** scope only as a Blender/Cycles
backend ([ADR 0005](0005-blender-renderer.md)). etendue writes no renderer of
its own.

- The geometric tier (analytic projection, remap, ground truth) lives in Rust
  and on the web.
- The photometric tier is Cycles.
- `etendue-core` stays free of rendering code.

### 4. `etendue-ui` is frozen

It gets no new features. It keeps building and passing its tests, and is marked
`publish = false`.

It is deleted at parity gate G6.3, once the user has confirmed that every
feature has a web equivalent: viewport, parameter panel, simulated-image panel,
and heatmap.

### 5. Publishing

- **crates.io:** `etendue-{scene,kinematics,synth,core,cli}`.
- **npm:** `@etendue/wasm` from this repo. The 3D packages publish from lab-ui
  under `@vitavision/*`.
- Each ecosystem has one version train: changesets on npm, the release
  workflow for Rust.
- Earliest publish dates:
  - `etendue-scene` after G1.1–G1.2;
  - the web packages after G2.2 and P2-4;
  - `etendue-synth` after G4.1–G4.3.
- calibration-rs is consumed from crates.io (`vision-calibration-* = "0.8"`).
  `[patch.crates-io]` redirects to the sibling checkout for local development.
  A path dependency cannot be published, so the old path dependency had to go
  (P0-2).

### 6. Superseded constraints

This ADR supersedes the following:

- the handoff's "Pure Rust front-to-back. No Tauri / React / Vite / Bun";
- the handoff's "No Blender dependency";
- the "realistic synthetic rendering" item in the handoff's and the book
  introduction's out-of-scope lists.

The other out-of-scope items still stand: Zemax-style component design,
polarization, coherent diffraction, and lens-prescription databases.

## Consequences

- Three toolchains: cargo, bun, and Blender. Blender runs locally only, never
  in CI.
- `etendue-core` and `vision-calibration-core` must build for
  `wasm32-unknown-unknown`. Gate G0.1 passed
  (`docs/measurements/g0_1_wasm_parity.md`).
  - It currently relies on a `getrandom` backend shim in `etendue-wasm`.
  - An upstream calibration-rs change that trims `rand` to seeded RNGs only is
    drafted for user review.
- Upstream work this pivot needs, each a separate calibration-rs or
  calib-targets-rs PR with user review:
  - the `rand` trim above;
  - public target-geometry primitives in `calib-targets-print` (P3-3);
  - a DeviceSpec laser-plane derivation (P5);
  - the existing `ApertureModel<S>` roadmap item.
- CI grows a wasm32 job now. Web jobs arrive with P2, and a
  `--all-features`-per-crate matrix arrives when the first features
  (`schemars`, `opw`) land in P1.
- `etendue-ui`'s egui/winit stack is the source of every RustSec finding in the
  workspace. At P0 the only one left is RUSTSEC-2026-0192 (`ttf-parser`
  unmaintained, reached through winit's Linux decorations). Deleting the crate
  at G6.3 removes that class of finding.

## Alternatives considered

- **Keep extending the egui desktop app.** Rejected. It cannot be reused by
  `calibration-diagnose` or in a browser, and the professional consumers need
  libraries, not an app.
- **Write an etendue renderer**, either a rasteriser with PBR or a path tracer.
  Rejected. Cycles is mature, and photorealism is not what etendue adds.
- **Host the 3D web packages in this repo.** Rejected under decision D1: lab-ui
  owns the `@vitavision/*` scope and its toolchain.
- **Keep the calibration-rs path dependency.** Rejected because it blocks every
  crates.io publish (F6).

## References

- `docs/pivot/PLAN.md` §0, §2, §7, §8
- `docs/handoff.md` (constraints superseded above)
- lab-ui `docs/plan/PLAN.md` L8-1
- calibration-rs ADR 0009 (pose naming) and ADR 0018 (schema-driven UI)
