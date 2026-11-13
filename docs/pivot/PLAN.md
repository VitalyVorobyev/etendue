# etendue pivot — Claude Code handoff (plan mode)

> Place at `etendue/docs/pivot/PLAN.md`. **Plan mode first.** Read the sources listed
> in §1 before proposing any change. Do not start coding until the user has approved
> the P0 plan. Existing `etendue/.claude/CLAUDE.md` rules stay in force unless §9 of
> this document amends them.

## 0. What changes

etendue (MVP M0–M6 done: `etendue-core` kernel + `etendue-ui` egui/wgpu desktop app)
pivots from a single desktop tool into a **family of independently published packages**:

1. **Scene and scenario model**: a frame tree with real robots (URDF), calibrated
   multi-camera rigs attached by hand-eye poses, lasers, lights, targets, and
   robot motion scenarios.
2. **Web visualization** as reusable three.js + React Three Fiber packages. These
   replace `etendue-ui` and are shared with the calibration-rs `calibration-diagnose` app.
3. **Synthetic image generation** with ground truth (GT). There is a geometric tier
   (web + Rust), and a photometric tier with **Blender/Cycles as the only realistic
   renderer**. etendue writes no renderer of its own.
4. **Optical/mechanical design**: the existing `etendue-core` analysis, surfaced as
   overlays in the web scene.

**Scope change to record in ADR-0001.** `book/src/roadmap.md` lists "realistic
synthetic rendering" as out of scope. It comes into scope only as the Blender backend.
`etendue-core` stays free of rendering code.

**Primary consumer requirement.** The packages are used in the user's professional
work, outside etendue. No monolith: each package must build, test, and publish on its
own, and none may depend on the studio app.

## 1. Read before planning (mandatory)

| Path | Why |
|---|---|
| `etendue/.claude/CLAUDE.md`, `etendue/docs/handoff.md`, `etendue/book/src/{architecture,roadmap,laser,scheimpflug}.md` | Existing rules, conventions, and Scheimpflug/defocus gotchas |
| `etendue/crates/etendue-core/src/scene/{entity,scene}.rs` | Current `CameraEntity`/`LaserEntity`/`TargetEntity` (pose = world ← local) |
| `calibration-rs/AGENTS.md` | Layering rules, determinism, quality gates |
| `calibration-rs/docs/adrs/0005,0009,0016,0018,0020,0021,0022,0023` | Camera pipeline, `a_se3_b` naming + SE3 wire order, DatasetSpec, schema-driven UI, laser manifest, DeviceSpec |
| `calibration-rs/crates/vision-calibration-core/src/models/{camera,params,sensor,distortion}.rs` | `CameraParams`, `CameraModel`, `ScheimpflugParams::compile`, iterative `undistort` |
| `calibration-rs/crates/vision-calibration-dataset/src/{spec,device_spec}.rs` | `DatasetSpec`, `RobotPoseSource`, `PoseConvention`, `DeviceSpec`, `HandeyeMountSpec`, `LaserPlaneSpec` |
| `calibration-rs/app/src/workspaces/Viewer3DWorkspace/*`, `app/src/lib/{se3,sceneExport}.ts` | Existing R3F components (~1.4 kLOC) that get extracted in P2 |
| `calib-targets-rs/crates/calib-targets-print/src/{lib,render}.rs`, `crates/calib-targets-wasm/` | Target geometry source and the existing WASM packaging pattern |

**Facts the plan relies on.** These were verified against source on 2026-09-26.
Re-verify each one; if any turns out false, stop and report.

- F1: calibration-rs camera pipeline is `pixel = K(sensor(distortion(projection(dir))))`.
  Only the `Pinhole` projection exists. Distortion is applied *before* the sensor
  homography. `undistort` is iterative (`iters` field).
- F2: `DeviceSpec` already models `rig_se3_cam` mounts, `HandeyeMountSpec::{EyeInHand{gripper_se3_rig}, EyeToHand{rig_se3_base}}`,
  and `LaserPlaneSpec` in the rig frame. Its units are datasheet-natural (`_mm`, `_deg`)
  and the pipeline world unit is metres.
- F3: calibration-rs `app/` is Tauri + React 19 + R3F 9 + three 0.185 + zustand + bun,
  with schemas emitted by `cargo xtask emit-schemas` and TS types generated from them.
- F4: `k` (openrr) 0.32 depends on **nalgebra ^0.30**, which conflicts with the
  nalgebra 0.34 hard pin. It was last released in 2024. **Do not use `k`.**
- F5: `urdf-rs` 0.10 has no nalgebra dependency and is safe to use. `rs-opw-kinematics` 3.0
  (BSD-3) uses `glam` and its default features pull in collisions/planning. If it is
  used, set `default-features = false` and convert glam↔nalgebra only at the boundary.
- F6: etendue currently path-depends on `../calibration-rs/crates/vision-calibration-core`.
  A crate with path dependencies cannot be published to crates.io.
- F7: `vision-calibration-py` builds abi3-py310 wheels. **They are not used inside Blender** (§5).

## 2. Target package layout

```text
etendue/                                   (cargo workspace + bun workspace)
├── crates/
│   ├── etendue-core/        EXISTING  optics/laser/analysis kernel             → crates.io
│   ├── etendue-scene/       NEW P1    versioned schema: frame tree, entities,  → crates.io
│   │                                  robots, scenarios, lights, render jobs
│   ├── etendue-kinematics/  NEW P1    URDF → chain, FK, IK, scenario baking    → crates.io
│   ├── etendue-synth/       NEW P3    render-camera choice, remap LUTs, GT,    → crates.io
│   │                                  sensor noise, DatasetSpec emission, EXR ingest
│   ├── etendue-wasm/        NEW P2    wasm-bindgen facade                      → npm @etendue/wasm
│   ├── etendue-cli/         NEW P1    `etendue bake|render|gt|validate`,        → crates.io (binary)
│   │                                  embeds the Blender script
│   └── (etendue-ui)         DELETED   2026-10-17, before parity (ADR 0001 §4 amended)
├── web/                     NEW P2    bun workspace
│   ├── apps/studio/                   the studio, the main app: Tauri desktop bundles + web build
│   └── (packages/)                    the incubated @vitavision/* packages, dropped 2026-10-17:
│                                      lab-ui publishes three, three-react, workbench, ui
├── crates/etendue-cli/blender/etendue_blender/
│                            NEW P4    pure-Python render script (bpy only), embedded in etendue-cli
│                                      (inside the crate so `cargo package` includes it)
├── tools/robot-assets/      NEW P1    uv project: xacro → URDF, meshes → .glb, license manifest
└── docs/adrs/               NEW P0
```

**Dependency rules.** These are enforced by `cargo xtask check-layering` or equivalent
`cargo metadata` checks in CI.

- `etendue-scene` → nalgebra, serde, schemars (feature), `vision-calibration-core`
  (for `CameraParams`), and `vision-calibration-dataset` (for `DeviceSpec`/`TargetSpec`).
  It has no other etendue dependency.
- `etendue-kinematics` → `etendue-scene` and `urdf-rs`. `rs-opw-kinematics` sits behind
  an `opw` feature.
- `etendue-synth` → `etendue-scene`, `etendue-kinematics`, `vision-calibration-core`,
  `vision-calibration-dataset`, and `exr`, and since P5-3 `etendue-core` (without its
  `solver` feature; ADR 0001).
- `etendue-core` → it may depend on `etendue-scene` to build a `Scene` from a spec,
  and must not go the other way.
- `etendue-wasm` → it may depend on any library crate and on no binary crate.
- `@vitavision/three-react` → `@vitavision/three` → `three`, and never React. These packages
  do **not** depend on `@etendue/wasm`. Kernel calls (bake, LUT, projection) are injected
  through a small TS interface that the studio app wires to `@etendue/wasm`, so the 3D
  packages stay reusable without etendue.

## 3. Conventions (single source; each backend converts only at its edge)

- World frame: right-handed, **+Z up, metres**. Camera frame: calibration-rs/OpenCV,
  meaning +Z forward, +X right, +Y down.
- Transforms: the ADR 0009 `a_se3_b` naming, where `a_se3_b` maps b → a. The SE3
  wire format is `{rotation: [qx,qy,qz,qw], translation: [tx,ty,tz]}` (nalgebra
  `Isometry3` serde), matching calibration-rs `app/src/lib/se3.ts`. The existing
  etendue `pose` fields are documented as `world_se3_self`; do not rename them.
- three.js: the OpenGL camera looks down −Z with +Y up, so it relates to the CV camera
  by a fixed Rx(π). Scene up is `(0,0,1)`. Blender uses the same camera relation and is
  natively Z-up. Both conversions live in exactly one function per backend, each with
  a round-trip unit test.
- Units in spec files: metres and radians in `etendue-scene`. `DeviceSpec` (mm/deg)
  is consumed through calibration-rs's own derivation layer, never re-implemented.
- Pixel-centre convention: **determined empirically** by probe P4-2, not assumed.
  The result is recorded in ADR-0004.

## 4. Key design decisions (write as ADRs in P0; the user reviews)

- **ADR-0001 Pivot.** The scope change (§0), `etendue-ui` frozen, and the package
  split (§2).
- **ADR-0002 Frame tree.** Every entity (camera, rig, laser, light, target, part)
  attaches to a named frame: `{parent: "world" | "<robot>/<link>" | "<entity>", parent_se3_self}`.
  Eye-in-hand means the rig is attached to `robot0/flange` with `flange_se3_rig`, which
  is the calibration hand-eye `gripper_se3_rig`. Eye-to-hand means the target is attached
  to the flange and the rig to the world. This yields both topologies without special cases.
- **ADR-0003 Baking.** Kinematics runs **once, in Rust**. `etendue bake` turns a scenario
  into `baked.json`: for each frame index, the time plus `world_se3_frame` for every frame
  in the tree, with capture events flagged. The web and Blender backends only apply
  transforms. There is no FK in TS or Python.
- **ADR-0004 Canonical render camera + remap.** Every backend renders only a **canonical
  pinhole**: square pixels, principal point at the image centre, no skew, a supersample
  factor `s`, and a field of view chosen by `etendue-synth` to cover the target camera
  image plus a margin. The target calibration model is applied afterwards as a
  remap LUT computed by `vision-calibration-core`:
  `u_out → n_d = sensor⁻¹(K⁻¹ u_out) → n_u = undistort(n_d) → u_render = K_r n_u`.
  This covers Brown/Rational/ThinPrism/Division distortion, skew, principal point, and
  Scheimpflug **geometry**, and it is the same LUT for web and Blender. Blender needs
  only `lens` and `sensor_width`, with no `shift_x/y`.
  Known limitation, documented rather than fixed: Cycles depth of field assumes a focal
  plane parallel to the sensor, so Scheimpflug **defocus** is wrong in photometric
  renders. etendue-core's `PhysicalCamera` (its blur) remains the reference for it.
- **ADR-0005 Blender is a thin renderer.** `etendue-cli` writes `job.json` (the baked
  frames for the capture events, meshes, materials from a small PBR subset, lights, and
  canonical camera parameters). It then invokes
  `$ETENDUE_BLENDER -b --factory-startup --python-exit-code 1 --python <embedded script> -- job.json out/`.
  Blender returns **linear float EXR** radiance, plus depth and object-index passes.
  Remap, exposure, noise, and quantization all happen in Rust (`etendue-synth`). The
  script uses only `bpy` and the stdlib, with no PyO3 wheels, so there is no Python ABI
  coupling. The Blender version is pinned in `etendue.toml` and checked at startup.
- **ADR-0006 Ground truth.** GT is analytic, not taken from renders. Target points come
  from the same `PrintableTargetDocument` that produced the texture or mesh. They are
  projected with the calibration-rs `CameraModel` and carry per-point visibility
  (frustum plus occlusion, with occlusion taken from the Blender object-index pass).
  The output is a calibration-rs `DatasetSpec` with images, `robot_poses`, and
  `pose_convention`, plus a `DeviceSpec` (nominal) and a `gt.json` (true `CameraParams`,
  `cam_se3_rig`, hand-eye, laser planes). Calibration-rs therefore consumes the
  synthetic datasets directly.

## 5. Phases, tickets, gates

Format: `ID — title` · **Files** · **Done when**. Every gate number is an *initial
proposal*. If a first measurement shows a gate is mis-set, report the measured value
and ask; do not silently relax it.

### P0 — Pivot groundwork

- **P0-1 — ADRs 0001–0006.** Files: `docs/adrs/000{1..6}-*.md`. Done when the user
  has approved them.
- **P0-2 — Publishable dependencies.** Replace the path dependency with
  `vision-calibration-core = "0.8"` and `vision-calibration-dataset = "0.8"`. Add
  `[patch.crates-io]` entries pointing to `../calibration-rs/crates/*` for local
  development. Files: `Cargo.toml`. Done when `cargo package -p etendue-core --no-verify`
  succeeds and `cargo tree -d | grep nalgebra` is empty.
- **P0-3 — WASM spike.** Run `cargo build -p vision-calibration-core --target wasm32-unknown-unknown`.
  If it fails (the suspect is `rand 0.10` → `getrandom`), prepare an **upstream PR draft**
  for calibration-rs and do not merge it; the user reviews. Done when gate **G0.1**
  passes: core plus a stub `etendue-wasm` build and are callable from Node
  (`project_point` on the `default_mvp` camera equals the native result bit-for-bit).
- **P0-4 — CLAUDE.md amendments** as described in §9.

### P1 — Scene schema, kinematics, baking (headless)

- **P1-1 — `etendue-scene` v1.** Covers `SceneSpec {version, frames, robots, cameras,
  rigs, lasers, lights, targets, parts}`, `ScenarioSpec`, and `BakedScenario`. Enums
  follow calibration-rs serde conventions (`tag="type"`, snake_case, `flatten`,
  `deny_unknown_fields`) and derive schemars behind a `schemars` feature. Files:
  `crates/etendue-scene/src/{lib,frame,entity,robot,scenario,light,baked}.rs`. Done when
  JSON round-trip property tests pass and `cargo xtask emit-schemas --check` is clean.
- **P1-2 — Robot assets.** Pin upstream descriptions by git SHA, expand xacro, convert
  meshes to `.glb` per link with `trimesh`, and write a `robot.json` manifest (joint
  tree, limits, mesh references, **licence**). Initial set: UR5e plus one OPW-type
  6-axis industrial arm, subject to licence. Files:
  `tools/robot-assets/{pyproject.toml,build.py,robots.toml}`, output to
  `assets/robots/<id>/` (git-ignored unless the licence allows redistribution). Done when
  gate **G1.3** passes: per-mesh vertex positions round-trip within ≤1e-6 m of the
  source mesh.
- **P1-3 — FK.** Write a custom serial-chain FK on nalgebra 0.34 over `urdf-rs`
  (revolute, prismatic, fixed; `origin` and `axis`). Files:
  `crates/etendue-kinematics/src/{urdf,chain,fk}.rs`. Done when gate **G1.1** passes:
  on 10k random configurations per robot, FK matches a fixture generated once with
  Pinocchio (`pip install pin`, script at `tools/fixtures/fk_fixture.py`, committed JSON)
  to ≤1e-9 m and ≤1e-9 rad.
- **P1-4 — IK.** Use generic damped-least-squares IK. Analytic OPW IK goes behind the
  `opw` feature. Done when gate **G1.2** passes on 10k reachable poses: FK(IK(T)) is
  within ≤1e-9 m / 1e-9 rad for OPW and ≤1e-6 m / 1e-6 rad for DLS; report the DLS
  failure rate and the 99th-percentile iteration count.
- **P1-5 — Scenario compiler.** Supports steps `ptp{q}`, `ptp{base_se3_tool}`, and
  `lin{base_se3_tool}`, with trapezoidal joint profiles under per-joint velocity and
  acceleration limits from `robot.json`. Capture is **stop-and-shoot only** in v1. It
  outputs `BakedScenario` at a fixed `dt`. Done when joint limits are never exceeded
  (property test) and captures occur only at zero velocity.
- **P1-6 — `etendue-cli` bake/validate.** Commands: `etendue validate scene.json scenario.json`
  and `etendue bake … -o baked.json`. Done when both work on example scenes
  `examples/{eye_in_hand_ur5e,eye_to_hand_ur5e}.json`.

### P2 — Web packages (the reusable components)

- **P2-1 — `@etendue/wasm`.** Uses wasm-bindgen, with TS types taken from the emitted
  JSON schemas; toolchain per lab-ui ADR-0002 via the calibration-rs `generate-types` pattern.
  *As built:* `new EtendueScene(scene, robots)` validates (as `etendue validate`), then
  `bake`, `projectPoints`, `backprojectPixels` (camera fields of view for viewers, so no
  camera math in TS) and `targetExtent`. `remap_lut` moves to P3-1 (it needs
  `etendue-synth`) and the etendue-core analyses to P6-1. Done when gate
  **G2.1** passes: the release `.wasm` is ≤1.5 MB gzipped (record the actual value).
- **P2-2 — `@vitavision/three` (lab-ui ticket L8-1; incubated in etendue `web/packages/three`,
  moved to lab-ui by PR).** Contents: conventions (§3), a `FrameTreeRuntime` that
  applies `BakedScenario` to an `Object3D` graph, a robot `.glb` loader bound to link
  frames, `CameraFrustum`, `LaserFan`, `TargetBoard` (mesh built from target
  primitives), and light gizmos. Uses only the framework-agnostic three API. Done when unit
  tests run under vitest with headless GL, or under Playwright where headless GL is not
  available. **`SensorView` moves to after P3-1**: it renders the canonical pinhole into a
  render target and remaps it through the LUT (RG32F texture), and the LUT is P3-1's.
  *Done with P3-1:* gizmos live on a separate layer, so sensor images show only the world.
- **P2-3 — `@vitavision/three-react` (incubated with P2-2).** Thin R3F components over P2-2. Per-frame updates go
  through `useFrame` and refs, never React state. Done when gate **G2.2** passes: with
  2 robots, 4 cameras, 1 target, and live scenario playback, p95 frame time is
  ≤16.7 ms over 600 frames in Chromium on an M-series Mac (Playwright perf harness at
  `web/apps/studio/e2e/perf.spec.ts`).
- **P2-4 — Extraction proof.** Port calibration-rs `Viewer3DWorkspace/{CameraFrustum,LaserPlane,TargetBoard}`
  onto `@vitavision/three-react` as a **branch in calibration-rs** (user review). Done when
  calibration-diagnose e2e tests pass unchanged with the packages consumed from a local
  `bun link` or tarball.
  *Status (2026-09-27):* done as calibration-rs draft PR #121 (packages from lab-ui#39
  tarballs; app e2e 7/7 unchanged). Its API findings — colour overrides, colour
  normalisation, non-pickable outlines, emphasis on boards and fans, a padded frustum pick
  hull, a configurable `SceneCanvas` — are fixed in the incubated packages.
- **P2-5 — Studio app.** A Vite app that loads scene, scenario, and baked JSON, plays
  scenarios, shows `SensorView` per camera, and exports `job.json`. It is not published.
  *v0 (this batch):* examples and dropped files, bake via `@etendue/wasm`, frame tree,
  3D viewport, inspector, joint chart, and per camera an analytic preview (targets projected
  by the kernel, on a `@vitavision/stage2d` stage). `SensorView` follows P3-1; `job.json`
  export follows P4-1.
- **P2-6 — Shared studio UI (user decision 2026-09-27).** UI the studio needs and lab-ui
  lacks is built in etendue first, to lab-ui's Definition of Done, then moved by PR:
  `@vitavision/workbench` (new package: app shell, split panes, tree view, playback bar with
  an external playhead store, file drop, toasts; amends lab-ui's "app shell out of scope")
  and additions to `@vitavision/ui` (`NumberInput` unit, `VectorInput`, `PoseInput`),
  incubated as `@vitavision/ui-next`. Done when the lab-ui PRs are open for review; the
  swap to published versions follows their release.
  *Status (2026-10-17):* done. The studio uses the published `@vitavision/three` 0.2,
  `three-react` 0.2, `workbench` 0.3 and `ui` 0.12, and `web/packages/` is gone.

- **P2-7 — Studio desktop shell (Tauri).** D4's trigger fired: launching renders from the UI
  ([ADR 0007](../adrs/0007-tauri-studio-shell.md)). Tauri 2 around the studio, in its own
  cargo workspace (`web/apps/studio/src-tauri`), on the `etendue_cli` library. Native commands do
  dataset generation (ground truth → Blender → detection, with progress and cancel), Blender's
  status and scenarios from tool poses. `@etendue/wasm` stays the per-frame kernel. Done when the
  studio opens a scene from disk, generates a dataset, and shows its images, all from the UI.
  *Status (2026-09-29):* built. Covers the Dataset panel, "Open scene…", "Import poses…", and the
  dataset image in camera views. The Rust tests pass (`cargo test` in `studio-tauri`), and so does
  the mocked-shell e2e (`e2e/tauri.spec.ts`). The interactive check in `bun run tauri dev` waits
  for the user.

### P3 — Geometric synthesis (Rust + web)

- **P3-1 — Canonical render camera and LUT** in `etendue-synth::remap`. Done when gate
  **G3.1** passes: over random pixels on full images of every supported model
  (including Scheimpflug with tilts up to 6°), the round-trip `project(unproject(u))` is
  within ≤1e-6 px. Report the `undistort` iterations needed at the image corners, and
  file an upstream issue if 8 iterations are insufficient.
  *Status (2026-09-27):* done. `remap` is in `@etendue/wasm` and `SensorView` in
  `@vitavision/three`. **G3.1 passes** on all 10 gate cameras (worst 8.2e-13 px) since
  calibration-rs 0.8.2 moved undistortion to Newton's method (calibration-rs#120/#122). Under
  0.8.1, 4 cameras needed 14–28 fixed-point iterations at the corners
  (`docs/measurements/g3_1_remap.md`).
- **P3-2 — Analytic GT and DatasetSpec emission** (ADR-0006). Done when
  `vision-calibration-dataset` `validate()` accepts every emitted manifest.
  *Status (2026-09-27):* `etendue-synth::{gt, dataset}` built — per-point projection with the
  three geometric visibility tests, topology and hand-eye from the frame tree, `dataset.json` /
  `robot_poses.json` / `gt.json`. Both examples validate (`tests/dataset.rs`). `etendue gt`
  writes them for a scene. Its board points come from calib-targets since P3-3.
  `device.json` (nominal `DeviceSpec`) needs sensor pixel pitch in the scene and follows in P5.
- **P3-3 — Target geometry source.** Build target meshes from `calib-targets-print`
  primitives. If they are not public API, prepare an upstream PR draft for
  calib-targets-rs; the fallback is the SVG texture at ≥8 texels per projected pixel.
  Both paths are measured in P4-3.
  *Status (2026-09-28):* done. calib-targets 0.15.2 made `board_primitives` public
  (calib-targets-rs#104), and 0.15.3 accepts nalgebra 0.34 (#105).
  - `etendue_synth::board` builds chessboard and ChArUco boards from the primitives, as
    non-overlapping cells, and takes the GT points from `resolved_points`.
  - `etendue render` meshes those cells; `etendue gt` uses the same points.
  - The target frame faces +Z, so print-down is −Y: the print's top-left is at −X/+Y.
  - Re-runs on the new board: G4.2 unchanged, G4.3 still passes. The mesh is exact, so the
    texture path is dropped.
  - Found calib-targets' ChArUco detector labelling corners one square off on rotated
    boards (calib-targets-rs#106; fixed in 0.15.5, #109).
  - Open: the web viewers still draw a checker, so ChArUco markers and even-row boards need a
    `cells` option in `@vitavision/three` (`docs/measurements/p3_3_target_geometry.md`).

### P4 — Blender backend (photometric tier)

- **P4-1 — Job writer, embedded script, `etendue render --backend blender`.** Cycles on
  the Metal GPU, fixed seed, Filmic/AgX **off** (Standard view transform, linear EXR).
  Done when the example scenario renders end to end and the CLI checks the Blender
  version.
  *Status (2026-09-27):* done. `etendue render` (job.json, embedded script, EXR → LUT →
  PNG); the eye-in-hand example renders 10 captures × 1 camera in 76 s at 16 samples on an
  M4 Pro (Metal). Blender 5.1.1 pinned in `etendue.toml`; since 2026-10-18 the 5.2 series
  (G4.1–G4.3, P4-5 and P5-1b re-run on 5.2.2). Findings: Blender 5 selects
  multilayer EXR through `media_type`; the glTF importer's Y-up rotation is undone in
  `convert.py`; EXR rows are top-first (`tests/blender_exr.rs`). Boards are calib-targets'
  print since P3-3.
- **P4-2 — Convention probe.** Render small emissive spheres at known 3D points, once
  in Blender and once in the web `SensorView` (readPixels). Compare the intensity-weighted
  centroid with the analytic projection. Done when gate **G4.1** passes: ≤0.01 px on
  both backends. The pixel-centre convention is then written into ADR-0004.
  *Status (2026-09-27):* **G4.1 passes on both backends** (worst 0.0030 px Blender,
  0.0049 px web, mean ≤ 0.0005 px, both conventions). No backend has a half-pixel offset;
  the convention is a labelling choice (default `Integer`, confirmed against the detector in
  P4-3). Resampling now box-filters the pixel footprint (`docs/measurements/g4_1_convention.md`).
- **P4-3 — Corner bias study.** On noise-free renders, compare chess-corners output with
  the analytic GT. Sweep supersampling s ∈ {1, 2, 4, 8} and board as mesh vs texture.
  Done when gate **G4.2** passes: RMS ≤0.02 px at the chosen default, with the curve
  committed to `docs/measurements/corner_bias.md`.
  *Status (2026-09-27):* measured with `etendue measure g4-2` (mesh board; since P3-3 the mesh is
  calib-targets' print, and the texture path is dropped). **G4.2 fails and is mis-set for chess-corners 1.2**: on an exact, renderer-free image
  its refiners are off by 0.08–0.19 px RMS, the same as on the renders. s = 4 is converged, the
  mean bias is ≤ 0.013 px (no convention offset, `Integer` confirmed), sRGB output roughly
  doubles the error, and the render-vs-exact difference is 0.03 px at s = 4 (center of mass).
  *Decided (2026-09-28):* G4.2 is a characterisation, and the current accuracy is the accepted
  baseline, not a blocker (user decision). chess-corners' **Radon** detector is closer than every
  ChESS refiner: 0.032 px RMS at s = 4 on linear renders (2.3–5.7×), 0.046 px on the exact image
  (1.7–4.1×). The
  synthetic-dataset defaults are s = 4, linear output, and Radon
  (`docs/measurements/g4_2_corner_bias.md`).
- **P4-4 — Cross-backend agreement.** Run the same frame, same camera, same detector on
  both backends. Done when gate **G4.3** passes: corner RMS difference ≤0.05 px. If it
  fails, suspect a convention error first.
  *Status (2026-09-27):* **G4.3 passes.** On the G4.2 scene at s = 4, the web `SensorView` vs
  Blender difference is 0.018 px RMS with center of mass, 0.040 px with Förstner and 0.043 px
  with saddle point (chess-corners 1.2.0; the wasm build runs in the browser). Each backend's
  error against GT agrees with the other's to 0.001 px (`docs/measurements/g4_3_cross_backend.md`).
- **P4-5 — Determinism.** Render the same job twice. Report the maximum absolute
  difference for Metal and for CPU. Done when GT-critical renders use whichever device
  is deterministic, or the variance is documented with its bound.
  *Status (2026-09-27):* CPU bit-exact; Metal within 2.4e-7 radiance run to run; GPU vs CPU
  up to 2.2e-2 per pixel. GPU stays the default with the bound documented, `--cpu` for
  bit-exact renders, never both in one dataset (`docs/measurements/p4_5_determinism.md`).
- **P4-6 — Sensor model** in `etendue-synth::sensor`: exposure, gain, shot/read/PRNU
  noise, and quantization, all with explicit seeds. Done when the photon-transfer
  curve (variance vs mean) reproduces the configured gain within ≤2%.
  *Status (2026-09-27):* done. `etendue-synth::sensor` (PRNU, shot, dark and read noise, full
  well, gain, black level, quantisation, ChaCha8 seeds); PTC recovers K within 0.43 %;
  `etendue render --sensor` writes raw mono PNGs (`docs/measurements/p4_6_sensor.md`).

### P5 — Closed loop and laser

- **P5-1 — Calibration closed loop.** Build a synthetic eye-in-hand dataset
  (UR5e, 20 poses, 1 camera and then a 2-camera rig) and run it through the calibration-rs
  `dataset_runner`. Done when gate **G5.1** passes on noise-free input: mean reprojection
  ≤0.05 px, fx/fy relative error ≤1e-4, cx/cy ≤0.05 px, hand-eye rotation ≤0.01° and
  translation ≤0.05 mm at a working distance of about 0.5 m. Record the baseline in
  `docs/measurements/closed_loop.md`.
  *Status (2026-09-27):* **G5.1 passes on analytic correspondences**, for one camera and for the
  two-camera rig, 5–8 orders of magnitude inside every limit. The loop is `etendue gt`, then
  `tools/closed-loop` feeding calibration-rs's solver through the 0.8.2 Python wheel, on
  `examples/closed_loop_ur5e` (20 poses at 0.45–0.55 m). CI runs it. The image-level loop is no
  longer blocked (G4.2 decided 2026-09-28; its baseline is Radon's 0.032 px). The loop found calibration-rs#124: identity-rotation mounts
  were rejected by the hand-eye init (`docs/measurements/closed_loop.md`).
  *Status (2026-09-29), image level (P5-1b):* render → `etendue detect` (Radon corners,
  calib-targets labels, checked against the GT) → calibration-rs. On `closed_loop_ur5e`, 40/40
  views are detected, with 0 mislabels and 0.051 px RMS against the GT. The calibration is
  limited by noise: every result sits inside the spread of unbiased white noise of the same RMS,
  and the principal point and hand-eye are outside G5.1's noise-free limits (cy up to 0.56 px,
  hand-eye 0.014° / 0.12 mm). How to gate the image-level loop awaits a decision
  (`docs/measurements/closed_loop_images.md`).
  *Status (2026-11-02), P5-1c:* the chessboard's corners come from ChESS with the saddle-point
  refiner (user decision of 2026-11-01, on the loop's confirmation): 0.012 against Radon's
  0.058 px RMS, hand-eye 0.0017° / 0.019 mm, every error within the white-noise spread. The
  other kinds keep Radon. These are synthetic images only
  (`docs/measurements/p5_1c_saddle_point.md`). The renders found an IK branch in which the arm
  hid the board (fixed with `initial_q`), and that the GT has no robot self-occlusion yet.
  *Re-run (2026-10-18, Blender 5.2.2, the 24 generated viewpoints):* 48/48 views, 0 mislabels,
  0.058 px RMS; still noise-limited (every intrinsic and hand-eye quantity inside the
  white-noise spread), the mean reprojection error 0.0005 px above its max. The gating decision
  still awaits the user.
- **P5-2 — Laser sheet in Cycles (spike).** Compare (a) a spot light with a slit
  texture against (b) an area light with near-zero spread behind a slit occluder.
  Measure the across-stripe profile on a matte plane. Done when gate **G5.2** passes:
  the fitted stripe width vs depth is within ≤10% of `etendue-core` `GaussianBeamWidth`
  plus the CoC prediction over the design depth range.
  *Superseded by P7-7 (2026-10-17):* the laser is composited in Rust, not lit in Cycles, and the
  renders have no defocus. G5.2 is redefined (the user's decision): in rendered laser images the
  fitted stripe width is within ≤10% of `GaussianBeamWidth` over the scene's depth range.
- **P5-3 — Laser closed loop.** Run the calibration-rs rig laserline problem on
  synthetic data. Done when gate **G5.3** passes: plane normal error ≤0.01° and distance
  error ≤0.05 mm.
  *Status (2026-09-28):* **G5.3 passes on analytic correspondences** (`examples/laser_ur5e`):
  the joint rig hand-eye laserline problem recovers the plane to 1.3e-7° and 1e-8 mm per
  camera. `etendue-synth::laser` gives the stripe (via `etendue-core::laser`, now a dependency)
  and the true planes, `etendue gt` writes a `rig_handeye_laserline` dataset, and CI runs the
  loop. The Scheimpflug variant does not pass and is recorded as a characterisation; whether
  G5.3 should gate Scheimpflug rigs is an open question for the user. Seeded as calibration-rs's
  ADR 0022 prescribes, it stops in a flat tilt / principal-point valley (plane normal
  0.008–0.010°, cy up to 2 px off), and the wheel lacks `manual_init`. Upstream drafts await
  review (`docs/measurements/g5_3_laser_closed_loop.md`).
  *Update (2026-11-01):* on calibration-rs 0.11 the Scheimpflug variant passes G5.3 on exact
  data too (laser normal ≤ 1.3e-7°, hand-eye 1.1e-7°; `calibration_rs_0_11.md`).
  *Update (2026-11-02):* it passes because calibration-rs holds p1 and p2 in that solve, so the
  radial distortion reveals the tilt. Under 0.1 px of noise the tilts come out within about 0.1°
  (`scheimpflug_observability.md`).

### P6 — Design overlays and egui retirement

- **P6-1** — Surface the `etendue-core` analyses (defocus map, working volume,
  triangulation angle) as `@etendue/three` overlays via WASM.
- **P6-2 — Performance trigger.** If any analysis takes >100 ms per UI update in WASM
  (measured), escalate by adding the analysis as a Tauri command or as a Web Worker
  with a wasm threads build. Do not do this pre-emptively.
- **P6-3 — Parity.** Every former `etendue-ui` feature (viewport, parameter panel,
  simulated-image panel, heatmap; also the Scheimpflug solver UI and the rig builder) gets a
  studio equivalent. *Status (2026-10-17):* the user waived the gate and `crates/etendue-ui`
  is deleted (tag `egui-ui-final`); the features remain to be brought to the studio.
- *Status (2026-10-19):* P6-1 and P6-3 are absorbed by P10 (Optics workspace) and P11 (Rigs
  workspace); P6-2 stays the escalation rule they follow.

### P7 — Authoring (user request 2026-10-17)

Turn the studio from a viewer into the tool that designs a calibration cell end to end: pick a
robot and a rig, place any calib-targets target, generate poses that every camera sees, get a
feasible, jerk-limited motion, export it, and render images (lasers included). User decisions:
FAIRINO FR5 and igus ReBeL join the robot set (FR5 local-only until its licence is known); the
PuzzlePole cylinder is rendered, ground-truthed and detected, not calibrated; one laser per
camera; motion must be feasible (limits, reachability, IK branch, singularity margin), with no
collision checking. Order: P7-0 → (P7-1, P7-3, P7-4) → P7-2 → P7-5 → P7-6 → P7-7.

- **P7-0 — Studio foundation.** Workspaces (Setup · Motion · Images · Dataset), a File menu,
  undoable edits baked in place (`EtendueScene.updateScene` keeps the robot models), Save / Save
  As with robot manifest paths rebased, export, and datasets from the scene as edited.
  *Status (2026-10-17):* done; ADR 0007 amended.
- **P7-1 — Robot library.** igus ReBeL, FAIRINO FR5 (local-only), `max_jerk` with provenance,
  a documented recipe for adding a robot. Gates G1.1–G1.3 re-run.
  *Status (2026-10-17):* done. G1.3 passes for all four robots (worst 3.0e-8 m), G1.1 to
  1.3e-15 m, G1.2 with 0 near-seed failures. FR5 builds into the git-ignored
  `assets/robots-local/` with `--include-local`; its `tool0` (0.1 m beyond the wrist centre) is
  unverified against FAIRINO's controller. Jerk limits are derived (10/s × acceleration) for
  every robot: no vendor publishes them.
- **P7-2 — Feasible motion.** Jerk-limited PTP (Ruckig, phase-synchronised) and LIN, the IK
  solution stored on `ptp_pose`, derivatives in the bake, a motion report, and exports
  (captures CSV/JSON, trajectory CSV, resolved scenario). Gate **G1.4**: sampled velocity,
  acceleration and jerk within limits, captures at rest, PTP straight in joint space.
  *Status (2026-10-17):* done; **G1.4 passes** on all four robots
  (`docs/measurements/g1_4_motion.md`). ADR 0003 amended (motion model v2), ADR 0001 amended
  (`rsruckig`).
- **P7-3 — Rig builder.** Mono, stereo, and 3- or 6-camera rings around the sensor axis at a
  tunable tilt, one laser per camera; dataset emission with per-camera planes. Gate **G5.3b**:
  the analytic laser loop on a 3-camera ring.
  *Status (2026-10-17):* layout, expansion, per-camera laser datasets, `closed_loop.py` and the
  studio's rig builder done; G5.3 still passes on `laser_ur5e`. **G5.3b passes** on
  `examples/rig3_laser_ur5e` (made by the rig builder and the P7-6 generator; every camera's own
  plane within 1.2e-5° / 2.8e-7 mm), and CI runs it.
  `etendue-core::Scene::triangulation_ring` is deleted (user's decision, 2026-10-17): the rig
  layout is the ring's one home.
- **P7-4 — Every printable target.** Chessboard, ChArUco, marker board, PuzzleBoard and the
  PuzzlePole cylinder as one target mesh for Blender and the web, 3-D ground truth. Gate
  **G3.2**: mesh area, coverage, circle chord error ≤1 µm, pole points vs calib-targets ≤1e-3 mm.
  *Status (2026-10-17):* done; **G3.2 passes** (`docs/measurements/g3_2_target_mesh.md`; pole
  points within 2.4e-5 mm). ADRs 0005 (job v3) and 0006 amended. `gt.json` stays at version 2
  (the 3-D point fields are optional). The web preview shows faint speckles where a pole's side is
  seen nearly edge-on (depth precision against the 10 µm dot lift).
- **P7-5 — Detection for every target.** Gate **G4.4** (characterisation): detection rate,
  mislabels, RMS against the ground truth per kind.
  *Status (2026-10-17):* done. `etendue detect` reads the target from `gt.json` and labels every
  kind (ChESS corners located by Radon for the self-identifying kinds). **G4.4**
  (`docs/measurements/g4_4_targets.md`): every view of every kind detected, 0 mislabels, 0.058 px
  RMS on the planar kinds; the PuzzlePole finds every corner seen within 60° of its normal
  (0.063 px RMS). Rendered with Blender 5.2.2 (pin 5.1.1 not installed). Upstream: calib-targets
  0.15.3 mislabelled ChArUco and marker boards turned past 45° in the image.
  - Fixed in calib-targets 0.15.5 (calib-targets-rs#109, released 2026-10-17): a cell
    alignment becomes a corner alignment through `calib_targets_core::cell_to_corner_alignment`;
    its regression test covers 0–270°.
  - etendue is on 0.15.5, and `rotated_charuco_and_marker_boards` runs again (passes).
- **P7-6 — Sphere pose generator.** Poses on spheres around the target, the sensor's +Z at the
  centre, θ/φ steps, several radii, kept only if reachable, feasible and seen by every camera,
  ordered by motion time. Gate **G2.3**: 400 candidates × 3 cameras in wasm ≤100 ms.
  *Status (2026-10-17):* done. `etendue-synth::poses`, `etendue scenario sphere`, wasm
  `generatePoses`, and the studio's Motion › Generate poses (candidates drawn as cones coloured by
  status, Apply as one undo step). **G2.3 passes**: 79 ms median
  (`docs/measurements/g2_3_generator.md`). `make_scenario.py` is retired: the three closed-loop
  examples are generated (24 viewpoints), and G5.1 and G5.3 still pass on them. The image-level
  loop (P5-1b) was re-run on them with Blender 5.2.2 (2026-10-18).
  - Not done from the plan: eye-to-hand generation (refused with a clear error), and previewing
    the robot at a clicked candidate.
- **P7-7 — Laser images.** A Rust compositor from Blender's depth, normal and albedo passes, and
  stripe extraction. G5.2 is redefined for it, by the user's decision (P5-2).
  *Status (2026-10-17):* compositor, job v4 (laser views, laser passes), `<capture>_laser.png`,
  and the studio's stripe overlay done. Cycles' Depth is planar (measured with Blender 5.2.2;
  ADR 0005 amended). On two rendered captures the stripe lies on the analytic stripe to within
  0.04 px mean.
  - Stripe extraction is in `etendue detect` and writes `laser_features.json`. The board image
    is subtracted and divided out, and a Gaussian is fitted per column.
  - **Image-level loop** on `rig3_laser_ur5e` (44 generated captures, Blender 5.2.2):
    - stripes are 0.06–0.07 px RMS from the analytic stripe, with no bias;
    - every laser plane is within the G5.3 limits (worst 0.0097°, 0.0047 mm), and so is the
      hand-eye;
    - the principal points are 0.25–0.40 px off, noise-limited as in P5-1b.
  - It is a characterisation: `g5_3_laser_closed_loop.md`.
  - **G5.2 (redefined) passes:** on the same renders every image's stripe is 1.021–1.035 times
    the beam's width; the excess is the pixel's own area (0.999–1.002 without it). A re-render
    with a 0.1 mm waist (the beam 45 % wider across the depths) gives 1.005–1.011, and the
    compositor test covers the width law from inside to 5× its Rayleigh range
    (`docs/measurements/g5_2_laser_width.md`).

### P8–P13 — From a cell editor to an optical-design instrument (user request 2026-10-19)

The studio edits cells well, but the optics the project started from never reached it. The
user asked for:
- an interactive inspector (sliders, immediate rendering);
- an Optics workspace (camera design from physical optics, laser triangulation, depth of
  field, Scheimpflug, ray propagation, a working volume to explore and optimise);
- a rig editor for classical stereo and for rings of k identical cameras or camera–laser
  pairs (radius ρ, tilt φ to the axis);
- interactive robots (joint sliders, TCP drag, trajectories);
- a calibration lab: noise-controlled synthetic observations, calibration, comparison with
  the ground truth.

Every view is interactive, in 2-D and 3-D, and every artifact lives in a project folder.

User decisions (2026-10-19):
- calibration runs natively (calibration-rs pipelines in Rust, through the shell) before
  any browser build;
- order after P8: optics, rigs, robot, calibration;
- a project folder holds every artifact (ADR 0009);
- a scene camera may carry its physical optics (ADR 0008).

Order: P8 → P9 → P10 → P11 → P12 → P13. Upstream lab-ui changes are drafts with interim
studio code, so no phase waits on a review.

- **P8 — Interactive editing.** Every field edit runs a full bake on the main thread. Each
  bake rebuilds the frame runtime, which re-fetches every robot mesh. Sliders cannot sit on
  that path.
  - **Measure first:** rebake time and mesh loads per edit on `examples/rig3_laser_ur5e`.
  - **Preview and commit:** `Studio.preview` / `commit` / `cancel`.
    - A pose preview never bakes. It overrides the edited frame and its descendants at
      every sample (`W_F' = W_F·P_old⁻¹·P_new`, descendants `W_D' = W_F'·W_F⁻¹·W_D`), in
      3-D and in the poses the camera views read.
    - Other previews rebake at most once per animation frame.
    - A commit is one undo step.
  - **No more reloads:** a mesh cache for robot visuals, and border rays memoised per
    camera.
  - **Inspector:** slider rows for pose (one per DOF, with a numeric field and a unit),
    collapsible sections, a fixed label column, tabular numerals.
  - **3-D gizmo:** `TransformControls` on the selected frame.
  - **lab-ui drafts:** a reusable frame runtime, overrides honoured by `pose()`, a gizmo,
    and a slider-number row.
  - **Gates:**
    - **G8.1:** during a 5 s slider drag on rig3_laser_ur5e there are 0 bakes, JS work p95
      ≤ 4 ms, and frame interval p95 ≤ 1.1× the idle interval p95 (headed, as G2.2; revised
      2026-10-19 with the user: the display period was mis-set, `g8_1_drag.md`).
    - **G8.2:** a commit makes one undo entry and loads no mesh.
- **P9 — Optics model, library, project folder** (ADRs 0008, 0009).
  - **Scene and library:**
    - `CameraSpec.optics` (lens with principal gap, entrance pupil and unit focusing;
      focus measured from the image plane; physical sensor tilt);
    - the camera pose is the mechanical frame M (image-plane datum); `PhysicalCamera`
      owns H, H′, E, E′ and the projection frame P at the entrance pupil, and every
      consumer takes its pinhole pose from it;
    - the stored fx, fy and tilt are the derived equivalent pinhole, validated; focus
      breathing is modelled (this also fixes `fx = f / pitch`);
    - library documents in `etendue-scene`;
    - the bridge `etendue-core::from_scene`.
  - **Core physics:** `optics::budget` (DOF, diffraction, total blur, mm/px),
    `optics::paraxial` (ray fans) and `analysis::triangulation` (dZ/px).
  - **Solver:** a deterministic Nelder–Mead replaces argmin; the `solver` feature goes.
  - **wasm:** `optics` functions.
  - **Project I/O:** in the shell and in the browser (File System Access).
  - **Reference project:** `examples/` becomes it, with the bank as its library. New
    entities come from project defaults.
  - **Gates:**
    - **G9.1:** thin-lens limit at 1e-12 relative: fx, `M_se3_P`, and near/far DOF.
    - **G9.2:** Nelder–Mead reproduces argmin's recorded optimum to 1e-6 rad / 1e-6 m.
    - **G9.3:** no `getrandom` in the wasm graph; bit parity for every new wasm function;
      G2.1 re-measured.
    - **G9.4:** frame invariance (ADR 0008 §4): a physical camera at M equals the derived
      pinhole at P through every consumer (1e-9 px); `project` and `coc` match an
      independent paraxial chief- and marginal-ray trace (1e-9 px, 1e-12 relative) with
      g ≠ 0, e ≠ 0 and tilt; moving the cell or re-parenting the camera changes no
      pixel; refocusing keeps M fixed.
- **P10 — Optics workspace.** A camera, or a camera + laser head, against a design volume.
  - **2-D cross-section** in the laser plane:
    - the image-plane datum, the principal points, the pupils, the tilted sensor;
    - paraxial ray fans, the plane of best focus, the DOF band, the field of view;
    - the laser line and the Scheimpflug hinge;
    - the working volume shaded by blur, mm/px or dZ/px;
    - draggable handles.
  - **3-D anatomy** view.
  - **Sensor view** of the stripe.
  - **Charts** against depth.
  - **Optimiser:**
    1. the hinge rule gives the start;
    2. Nelder–Mead minimises worst-case blur over (τ, D, N) under coverage;
    3. a lens search runs over the library.
  - **Output:** save as a camera or head design; place it in a cell with provenance.
  - **Gates:**
    - **G10.1:** slider to redraw p95 ≤ 16 ms; `workingVolume` 64×64 ≤ 30 ms in wasm.
    - **G10.2:** the cross-section's PoBF matches `plane_of_best_focus` to 1e-9.
    - **G10.3:** on a thin depth window the optimiser reproduces the analytic Scheimpflug
      tilt within 0.05°, never worse than its start.
  - *Status (2026-10-19):* the gates pass (`docs/measurements/g10_head_kernel.md`). Built in
    three steps:
    - **P10a:** the head kernel, `etendue-core::head`.
    - **P10b:** the workspace: the section, the map, the optimiser, the library and placing a
      head in a cell.
    - **P10c:** the sensor view (the laser line at seven ranges) and the stripe's width in pixels.
      A laser now has a `waist_distance_m`, where it is focused. Before, the waist was at the
      laser, so a focusable laser's thin waist made a 36–40 px stripe; focused at the aim point
      it is 1.8–8.9 px.
    - **P10d:** charts against range along the laser: the blur and the stripe width, and the
      depth and lateral resolution, on the line's centre and the worst across it.

    - **P10e:** the lens search: every library lens optimised on the head the same way, ranked
      by the window seen, then the worst blur, with the depth resolution beside it.

    - **P10f:** the 3-D anatomy view: the sensor, principal planes, pupils, field, fan, plane of
      best focus and the window coloured by blur. The head's section carries the camera in 3-D
      (`Section.solid`: the sensor's corners, the field's corner rays and the plane of best
      focus, in the body frame).

    Every P10 item is built.
- **P11 — Rigs workspace.**
  - **Schema:**
    - `ImageX::Common` makes k = 2 a classical stereo pair (baseline 2ρ, convergence
      ρ / tan φ);
    - `LaserLayout.camera_se3_laser` makes the unit a head;
    - the template carries optics.
  - **Core:**
    - `analysis::camera_overlap` (N-view coverage, visible and sharp);
    - `voxelized_overlap` pairs by `LaserSpec.camera`.
  - **UI:**
    - top view (ρ, phase) and side view (φ, the convergence point, DOF bands, overlap);
    - overlap voxels in 3-D;
    - a chart against φ;
    - a rig library, and placement in a cell.
  - **Gates:**
    - **G11.1:** k = 2 `Common`: parallel image +X to 1e-12, baseline 2ρ.
    - **G11.2:** two-frustum overlap within 2 % of the analytic volume at 128³.
    - **G11.3:** 64³ ≤ 100 ms in wasm.
  - *Status (2026-10-19):* the gates pass (`docs/measurements/g11_rigs.md`): 0.68 % and 12.7 ms.
    Built differently from the plan:
    - The coverage is one kernel, `etendue-core::rig::coverage`, over a `RigLayout` in the rig's
      frame. It counts camera views (sharp, with physical optics) and the pairs linked by
      `LaserSpec.camera`.
    - The legacy `voxelized_overlap` (index pairing, the optics kernel's own scene) stays as it was.
    - The workspace draws the rig in 3-D with its coverage voxels, side and top slices, and a
      chart against φ.
    - Camera units can be refocused on the working distance.
    - Deferred: drag handles in the slices, and depth-of-field bands. *Done in P11b:* the slices
      are drawn to scale with handles (working distance, ρ, φ, phase, box), and each camera's
      sharp range along its axis (`rig::sharpness`, judged as the coverage judges it) is a band
      on its axis.
- **P12 — Robot workspace (in Motion).**
  - **wasm:**
    - `robotLinkPoses(robot, q)`;
    - `robotIk(robot, base_se3_tcp, seed, opts)` (DLS, `restarts: 0` while dragging).
  - **Jogging:**
    - robot library preview;
    - joint sliders through P8's overrides, so the rig and its camera views follow;
    - TCP drag with IK, coloured by limit and singularity margin;
    - Cartesian jog.
  - **Programming:** "add PTP / capture here" and an editable step list.
  - **Trajectory view:** the TCP path drawn over the bake, coloured by step or binding
    limit, with capture markers and hover-to-seek.
  - **Gates:**
    - **G12.1:** FK bit parity on 1000 seeded q.
    - **G12.2:** UR5e IK p95 ≤ 2 ms in wasm, and ≥ 99 % success on a 1000-step drag path
      at 1e-6 m / 1e-6 rad.
  - *Status (2026-10-19):* both gates pass (`docs/measurements/g12_robot.md`). G12.1 is bit
    parity at 1000 joint vectors, after `libm` and in-order Jacobian products. G12.2 is p95
    0.012 ms with 1000/1000 converged.
    - **Built:** the Jog panel in Motion, with:
      - joint sliders, a TCP readout and manipulability;
      - Cartesian steps in tool or base axes;
      - the TCP gizmo (IK, *Out of reach*);
      - teaching a PTP move (joints stored) with or without a capture;
      - a step list with moves and deletes;
      - the TCP path with capture dots.
    - The jog shows on the bake through pose overrides: 0 bakes, work p95 2.9 ms.
    - **Deferred:** re-teaching a step in place, colouring the path by step or binding limit,
      hover-to-seek on the path, a robot library preview.
  - *Status, P12b:* three of the four deferred items are done.
    - **Re-teach.** A step is re-taught in place from the Jog panel: a PTP keeps the joints, a
      LIN the tool pose.
    - **Path colour.** The TCP path is coloured by step, with warnings in the warning colour.
      The move at or before the playhead is in the accent. Colouring by binding limit was
      dropped: a time-optimal move is always at a limit.
    - **Seek.** Pointing at the path marks a sample, and a click seeks to it. A pure hover-seek
      would move the playhead whenever the pointer crossed the path.
    - Still deferred: the robot library preview. *Done in P12c:* **Robot library…** in Setup's
      add menu lists the studio's robots with their axes, joint ranges and speeds and how far
      the TCP gets from the base (kernel FK over 4096 seeded joint vectors), and adds one or
      replaces the selected robot (one edit, refused when the program no longer fits).
- **P13 — Calibration lab.**
  - **Observations:** `etendue-synth::observe`, noise from a seed:
    - pixel σ;
    - outliers and dropouts;
    - laser σ;
    - robot pose σ.
  - **Single heads:** emission of a single camera + laser head.
  - **Calibration:**
    - `etendue-cli::calibrate`: calibration-rs sessions with in-memory inputs, and a port of
      `closed_loop.py`'s comparison rows;
    - `etendue calibrate` with N trials;
    - a shell command with progress and cancel;
    - results in the project's `runs/`.
  - **Calibrate workspace** (absorbs Dataset): Source → Calibrate → Compare.
    - Source is analytic + noise, or rendered + detected.
    - Compare has a table against the gate tolerances, residual histograms, and estimated
      vs true frusta and planes.
  - `closed_loop.py` stays as a cross-check until parity is recorded.
  - **Gates:**
    - **G13.1:** noise-free on closed_loop_ur5e, laser_ur5e and rig3_laser_ur5e: every row
      matches `closed_loop.py` within max(1e-9, 1e-6 rel), and G5.1, G5.3 and G5.3b pass
      through Rust.
    - **G13.2:** same seed, byte-identical report.
    - **G13.3:** at σ = 0.5 px, the reprojection RMS within 5 % of σ·√(1 − p/2n).
  - *Status (2026-10-19), P13a:* all three gates pass (`docs/measurements/g13_calibration.md`).
    - G13.1 differs from the wheel by at most 2e-12, and 2e-8 on the seeded Scheimpflug case.
    - G13.3 is −1.5 %.
    - By user decision, the detector stack may use its own nalgebra (ADR 0001 amendment).
    - A single head emits `laserline_device` and calibrates with the hand-eye alongside.
    - Eye-to-hand calibrates too.
    - CI runs the loops natively, with parity against the wheel.
    - **P13b** brings the shell command and the Calibrate workspace.
  - *Status, P13b:* done.
    - The shell's `calibrate_dataset` runs `etendue-cli::calibrate::run_dir`, with a progress
      step per trial and cancel, writing `calibration.json` into the dataset folder.
    - The studio's Dataset workspace is now **Calibrate**: generate, then calibrate (exact,
      noisy with trials, or detected) and compare.
    - The comparison is a table against the gate, the trial spread and a residual histogram.
    - **Deferred:**
      - estimated against true frusta and planes drawn in 3-D;
      - a noise preview on the camera views (wasm `observe`);
      - runs kept in the project's `runs/` (a run goes to its dataset folder);
      - a browser preview of the source.
  - *Status, P13c:* the first two deferred items are done.
    - **Estimate in 3-D.** The report's `estimate.offsets` give each estimated camera and the
      target in its true frame, and each laser plane, true and estimated, in the true camera's
      frame. The Calibrate workspace draws them over the truth in the warning colour,
      exaggerated ×1 to ×1000. They are tested against the hand-eye rows, eye-in-hand and
      eye-to-hand.
    - **Observations preview.** The wasm `observe` produces the observations, bit-identical to
      native (G0.1). It shares `etendue-synth::dataset::emit_scene` and
      `EmitOptions::STANDARD` with `etendue gt`. The workspace's camera view shows what a run
      observes at each capture: dots, outliers and dropouts over the true corners.
    - Still deferred: runs in `runs/`, and a browser preview.
  - *Status, P13d:* runs are kept (ADR 0009).
    - **Run folders.** A run is a folder holding:
      - `config.json`: the dataset and the options;
      - `report.json`: the report;
      - `result.json`: calibration-rs's export of the first trial.
    - **Where.** It is written by `etendue-cli::calibrate::write_run`, from the shell or from
      `etendue calibrate --run`, and listed by `read_runs`. The project's `runs/` holds it when
      a project is open for writing; otherwise the dataset's own `runs/`.
    - **Datasets.** With such a project open, new datasets default to its `datasets/<cell>`.
    - **Studio.** The Calibrate workspace lists a dataset's kept runs and shows any of them
      again.
    - Still deferred: a browser preview of the source.
  - *Status, P13e:* the browser has the Calibrate workspace as a preview. It shows the
    observations, exact or noisy, at each capture with their totals. Generating a dataset and
    calibrating stay in the desktop app. That closes P13's deferred list.

### P14–P18 — From a cell editor to a calibration planner (user request 2026-11-01)

The user asked for:
- an extendable, persistent bank of optical devices, robots and calibration targets;
- a clear tool to select optics, build a rigid rig, put it on a robot (or put the target on the
  robot), and select a calibration target;
- calibration robot poses suggested automatically;
- a full simulation of the calibration: each image with its detected features, reprojection
  errors, and statistics of the calibrated parameters;
- a bank of calibration procedures (intrinsics, extrinsics, hand-eye, non-linear refinements,
  which parameters are fixed and which free), and the sequence of them that calibrates a whole
  system.

User decisions (2026-11-01):
- the bank is layered: built-in, then a user bank, then the project, with what a project uses
  copied into it (ADR 0010);
- the phases follow the workflow's order, P14 → P15 → P16 → P17 → P18.

The user accepted ADRs 0010 and 0011 on 2026-11-01. Each step is one PR, merged when CI is
green; upstream changes (calibration-rs, lab-ui) stay drafts.

- **P14 — Component bank** (ADR 0010).
  - **Layers:** built-in, user bank (`Documents/etendue/bank`; a picked folder in a browser) and
    project, resolved by kind and id. Items record their `source` (datasheet) and, once copied
    into a project, their `origin`. The library format moves to version 2.
  - **New kinds:** `mount` (flange-to-device adapter) and imported robots (URDF + manifest).
  - **CLI:** `etendue library list|check|import`, `etendue project check --bank`.
  - **Studio:** a Library workspace (browse, create, edit, duplicate, rename, delete, import,
    export, use in project; forms from the library schema), and three-layer pickers in Setup.
    The studio resolves robots from the project's `robots/` too (ADR 0009 §2; today it reads only
    the app's library).
  - **Gates:**
    - **G14.1:** every built-in item round-trips byte-identically through the studio's save
      path and `library check`.
    - **G14.2:** a project that uses user-bank items still passes `project check` after the bank
      is removed (its copies are complete).
  - *Status (2026-11-02), P14a–c:* done (#54, #59, #56): library v2 with the `mount` kind, the
    bank and `etendue library`, the layered studio library, and the Library workspace.
    - Its forms are written by hand, not generated from the schema (see #56).
    - *P14d:* G14.1 and G14.2 pass (`docs/measurements/g14_bank.md`). Four built-in lens items
      did not round-trip until they were written with their defaulted fields.
- **P15 — Guided cell builder.** A Setup stepper that writes an ordinary `scene.json`:
  1. optics: a camera or head from the bank, or "design in Optics";
  2. rig: one camera, or a rig design;
  3. mounting:
     - **eye-in-hand:** the rig on the tool flange through a mount;
     - **eye-to-hand:** the target on the flange, the rig fixed in the cell;
  4. target: from the bank, sized against the field of view and working distance;
  5. robot and base placement.

  Each step checks reach and visibility live.
  - **Gate G15.1:** the builder recreates `examples/eye_in_hand_ur5e` and
    `examples/eye_to_hand_ur5e` from bank items (scenes equal to 1e-12; Playwright).
  - *Status (2026-11-02):* done (#63). G15.1 passes (`docs/measurements/g15_cell_builder.md`).
- **P16 — Calibration procedure bank** (ADR 0011).
  - **`CalibrationProcedure`:** ordered stages, each with:
    - a calibration-rs workflow and the devices it covers;
    - calibration-rs's own camera config and fix lists;
    - which estimates are free;
    - the solver;
    - seeds: an earlier stage, the nominal design, or the truth (simulation only).
  - **Engine:** `etendue calibrate --procedure` runs the stages through the workflows' `step_*`
    and `*_with_seed` functions. It checks the procedure against the dataset first.
  - **Built-ins:**
    - `intrinsics`, `stereo`, `eye_in_hand`, `eye_to_hand`, `laser_head`, `laser_rig`;
    - `auto`, today's behaviour, which stays the default.
  - **Studio:** a procedure editor in Calibrate: stages, a fixed/free table, seeds.
  - **Upstream:** missing hooks (`rig_handeye_laserline` has no steps) become draft calibration-rs
    PRs.
  - **Gates:**
    - **G16.1:** `auto` reproduces the current reports bit for bit.
    - **G16.2:** each built-in recovers the truth on exact data within G5.1 / G5.3 on its
      example cell.
  - *Status (2026-11-02), P16a:* the engine, the procedure kind and the built-ins are done
    (#57). The built-ins are `intrinsics`, `stereo`, `handeye`, `rig_handeye`, `laser_head` and
    `laser_rig`; both mountings share `handeye`. G16.1 and G16.2 pass
    (`docs/measurements/g16_procedures.md`).
    - *P16b:* the studio's procedure editor (Calibrate, and the Library workspace for procedure
      items): stages, a fixed/free table per slot, seeds, options and solver, checked by the
      kernel on every edit. A run takes the procedure by value; an edited one is kept in the
      project or the bank.
    - Upstream: `rig_handeye_laserline` seeded from an earlier rig hand-eye (calibration-rs#256,
      merged 2026-11-02, released in 0.13+); etendue does not use it yet (P19d).
  - *Update (2026-11-02):* the Scheimpflug head that no built-in calibrated is understood
    (`docs/derivations/scheimpflug_observability.md`, `docs/measurements/scheimpflug_observability.md`).
    - **Why its tilt is not determined.** With p1 and p2 free, a sensor tilt is matched by a
      principal-point shift f·τ, p1, p2 and a rotation of the camera frame, and no pose set
      separates them. With p1 and p2 held, the radial distortion determines it: about 0.1° at
      0.1 px on that head.
    - **Measurement is still right.** The camera-frame rows fail on calibrations whose laser
      plane in the gripper frame is right to 0.4 µm.
    - **A finding about every head.** `laser_head` and `auto` took the laser plane and the
      hand-eye from stages that estimate the camera separately.
  - *Update (2026-11-02, later):* fixed, with a check that sees it
    (`docs/measurements/head_triangulation.md`).
    - **G5.3c**, the triangulated rows: every true stripe pixel triangulated with the estimated
      camera, plane, hand-eye and robot pose, against the truth in the base; the largest gated at
      0.05 mm. It does not depend on the tilt's split, and it judges the result as a whole.
    - **What it found.** At 0.1 px of noise on the example head, `auto` was 0.25 mm rms off and
      `laser_head` 3.0 mm (up to 13 mm in a trial); on the Scheimpflug head `auto` was 29 mm off
      even on exact data. The head triangulates at a narrow angle (0.2–3.8 mm per pixel), so a
      camera a pixel apart from the plane's is millimetres off. The earlier plane-distance check
      missed errors within the plane.
    - **The fix.** `auto`'s head hand-eye and `laser_head`'s second stage hold the device stage's
      camera: 0.013 mm rms, the noise's floor (the rigs reach the same).
    - **Upstream.** calibration-rs's joint laser hand-eye opened to one camera (calibration-rs#268, merged
      2026-11-03; etendue does not use it yet, P19d). On the head it is right and no more precise than the held camera (0.014–0.016 mm).
- **P17 — Simulation lab.**
  - **Report v3:**
    - per view and per feature, residuals with ids (camera, view, point, observed and projected
      pixel);
    - per-stage results;
    - the **parameter values** of every trial, not only their errors;
    - calibration-rs's posterior σ (`uncertainty`);
    - the `analysis::quality` verdict.
  - **Runs** keep every trial's estimate, compactly.
  - **Studio:**
    - a per-image viewer: the rendered or synthetic image, detected features, reprojected points
      and residual vectors (magnified), per-view RMS, outliers;
    - parameter statistics: a histogram or scatter per parameter with the truth and the
      predicted σ band, and a table of mean, std, bias, RMSE and predicted σ;
    - stage by stage.
  - **Gates:**
    - **G17.1:** on `closed_loop_ur5e` with 200 noisy trials, the empirical std of fx, cx and the
      hand-eye matches calibration-rs's predicted σ to within 15 %.
    - **G17.2:** the per-feature residuals reproduce the report's RMS exactly.
  - *Status (2026-11-02), P17a–b:* done (#58, #60): report v3 and the Calibrate tabs (Per view,
    Parameters, Solver). G17.1 passes with the rig refined, and G17.2 passes
    (`docs/measurements/g17_sim_lab.md`).
- **P18 — Calibration pose planning.**
  - **Generator:**
    - It supports eye-to-hand; it refuses it today (`etendue-synth/src/poses.rs`).
    - It ranks the feasible candidates by the **system σ in mm**: the predicted covariance of the
      chosen procedure's free parameters (intrinsics, rig, hand-eye) propagated to the σ of a 3-D
      point in the robot base frame over the working volume (user decision, 2026-11-01).
      calibration-rs's `analysis::next_view` predicts only the intrinsics. Its extension to the
      rig and hand-eye terms is a draft upstream PR, and until it lands etendue computes those
      terms itself.
    - It picks K poses greedily, with hand-eye rotation diversity, under the existing reach,
      margin and visibility filters. Collision checking stays deferred.
  - **Studio:** "Suggest poses", with a pose budget and a curve of predicted σ against the
    number of poses, beside the sphere grid's.
  - **Gates:**
    - **G18.1:** for the same N, the planned poses give a system σ no larger than the grid's,
      and P17's empirical std confirms it (within 15 %).
    - **G18.2:** the eye-to-hand example yields a feasible tour.
  - *Status (2026-11-02), P18a–c:* done (#61, #62, #64): `etendue scenario plan`, eye-to-hand
    generation, and the studio's Suggest poses (wasm `planPoses`).
    - G18.1 passes: 0.0362 mm planned against 0.0462 mm for the grid at 8 poses, with 0.0377 mm
      measured.
    - G18.2 passes (`docs/measurements/g18_pose_planning.md`).
    - etendue computes the rig and hand-eye σ terms itself. Upstream `next_view` for rig hand-eye
      (calibration-rs#261) merged 2026-11-02; adopting it is P19d.

### P19 — Consolidation (review of 2026-11-11)

P14–P18 landed in a day. A review of the code, the studio and the docs found the cost: logic kept in
two or three places, an untyped boundary to calibration-rs, a studio that can lose work, and
documents that contradict each other. P19 pays that down before any new feature. Each step is one
PR, merged when green.

- **P19a — Docs truth pass.** CLAUDE.md, PLAN §6/§7 and the upstream PR states, README and the
  book's studio chapter for P14–P18.
- **P19b — Studio safety.**
  - an error boundary, so a kernel error in a render no longer blanks the app;
  - a confirm before unsaved work is dropped: opening a cell or project, closing the window
    (Tauri `onCloseRequested`, `beforeunload`), switching Library items with a dirty draft;
  - undo and save that act on what the workspace shows, or are absent where the scene is not shown.
  - Playwright covers each.
- **P19c — A typed calibration-rs boundary.**
  - `calibrate.rs` and `procedure.rs` read and write calibration-rs's own export and config types,
    not `serde_json::Value` with silent defaults; a config patch with an unknown key fails;
  - one trial loop (`etendue calibrate` calls `run_dir`);
  - `calibrate.rs` split by concern (report, rows, rigs, heads, statistics, runs).
  - **Gate:** G16.1 (`auto` bit for bit) and G16.2, G17.2 unchanged.
  - **Done with P19d (one PR):** the exports are read through calibration-rs's types; configs,
    seeds and inputs are still built as JSON but pass `strict`, which refuses a key calibration-rs
    does not read; `run_trials` is the one loop; `calibrate/` has nine modules of 120–400 lines.
- **P19d — calibration-rs 0.12 → 0.18.** Re-run the calibration gates (G5.x, G13.1, G16–G18) into
  `docs/measurements/calibration_rs_0_18.md`; seed `rig_handeye_laserline` stages from a
  `rig_handeye` stage (#256); use the joint one-camera laser hand-eye (#268) where it helps; use
  upstream `next_view` for the rig and hand-eye σ terms (#261) if G18.1 still holds.
  - **The bump is done** ([`calibration_rs_0_18.md`](../measurements/calibration_rs_0_18.md)):
    every gate holds, and only the Scheimpflug rig's results move (its tilts are now refined).
    Adopting #256, #261 and #268 is a follow-up PR.
  - **Adoption measured** ([`p19d_upstream_adoption.md`](../measurements/p19d_upstream_adoption.md)):
    - **#256 is adopted as seeds.** A laser rig's joint stage starts from a `rig_handeye` stage's
      result, and a new `planes` seed takes the design or an earlier stage's planes. On the
      examples it reaches the same minimum as the joint solve from datasheet values, so the
      built-ins are unchanged.
    - **#268 is not adopted.** A one-camera joint solve is no better under noise than the
      device-then-hand-eye route.
    - **#261 is not adopted.** The planner runs in wasm, which cannot take the pipeline crate,
      and G18 already checks it against calibration-rs's covariance.
    - **Finding:** the laser rigs' spread is up to 3.2 times calibration-rs's predicted σ (P20 T2).
- **P19e — One source for procedures and report types.**
  - the procedure engine's pure parts (`auto`, the check, the config patches, which freedoms apply
    to which workflow) move to a crate wasm can use, and the studio stops re-implementing them;
  - schemas for `gt.json`, `features.json` and the report, with the studio's types generated from
    them, as for the scene documents;
  - version checks and a migration path for stored runs, which today vanish silently when they do
    not parse.
  - **The procedure rules are done** (one PR): `etendue-scene::Problem::takes`/`seed_slots`/
    `seed_kinds`/`for_dataset` are the one table. Validation, `auto`, the studio (wasm
    `procedureRules`, generated types) and a test of the CLI's config routing all read it. It
    found two disagreements: `refine_intrinsics` on `rig_extrinsics` (now allowed: calibration-rs
    has it) and `refine_rig` routed for the laser rig (no longer).
  - **The report schemas and run versioning are done** (second PR): `calibration_run` and
    `kept_runs` schemas from `etendue-cli::calibrate` (a test writes and checks them, since
    nothing may depend on the CLI's library); the studio's report types are generated; a run a
    newer etendue wrote, or a damaged one, is listed with its reason. `gt.json` and
    `features.json` stay untyped on the studio side: the studio only names their paths.
- **P19f — Prune and type.** etendue-core modules with no consumer outside the crate (the
  egui-era `analysis`, `solver::scheimpflug`, `scene`, `laser::project`) are removed or kept for a
  named reason; `LibraryItem` becomes an enum (same JSON); gate tooling (`measure`, `corners`)
  moves behind a feature; etendue-cli's library API is narrowed or not published.
  - **The prune is done:** about 4,800 lines removed. The removed parts are `CameraEntity`,
    `ThickLens`, `optics::{coc, psf}`, `analysis`, `Scene`, `solver::scheimpflug` and
    `laser::project`. G9.2's subject (the Scheimpflug solver) is gone; the Nelder–Mead it
    measured stays (`head::optimise`), and G9.2's record stays as history. The derivation's
    plane-of-best-focus closed form is kept as an independent check in `tests/head.rs`. The kept
    `geom`, `scene::{LaserEntity, TargetEntity}` and `laser::{plane, width, intersect}` serve
    `etendue-synth`'s stripes.
  - **Not done, by decision:**
    - **`LibraryItem` stays a struct.** The same JSON as an enum needs `#[serde(flatten)]`, which
      serde cannot combine with `deny_unknown_fields`, so misspelt library keys would be accepted
      silently. The "exactly one part" check is in one place.
    - **The gate tooling stays.** `measure` and `corners` are private to the binary, not part of
      the library's API, and `measure` is already hidden from `--help`.
    - **The CLI library's published API** is part of the 0.3.0 publishing decision.
- **P19g — Studio structure.** `App.tsx` split by workspace and `state/studio.ts` by concern
  (history, I/O, bank, notifications); subscriptions narrow enough that playback re-renders only
  what moves; Optics and Rigs designs kept across reloads; one way to show a kernel error; a units
  and labels pass.
  - **`App.tsx` is split** (1246 → 481 lines). Each workspace is a hook returning its panes,
    as the Library's already was: `optics/`, `rigs/`, `calibrate/`, `motion/` and `setup/`
    `Workspace.tsx`. The File menu's exports (`panels/exports.ts`) and the edit shortcuts
    (`state/shortcuts.ts`) are their own modules. `App` keeps the selection, the shared 3-D view
    and the layout.
  - **`state/studio.ts` is split** (1187 → 775 lines). The parts moved out:
    - its types (`state/types.ts`);
    - where it reads from: the repository, files, project and bank folders (`state/sources.ts`);
    - the bank's and project's library items (`state/library.ts`, a `LibraryItems` helper the store
      delegates to through a two-method host).

    The store keeps the document, its edit history and opening and saving. Its public API is
    unchanged.
- **Release 0.3.0**, and a decision on publishing to crates.io and npm (§7).
  - **Decided (user, 2026-11-12):** 0.3.0 is a GitHub release only (CLI archives and app bundles,
    as for 0.2.0). etendue-cli is `publish = false`. Registry publishing waits until the
    real-data work (P20) settles the library crates' APIs. The P19g UX items left (narrower
    re-renders, Optics/Rigs drafts kept across reloads, one way to show a kernel error, a units
    pass) go to 0.3.x or 0.4.

After P19: **a real-data track**, P20 (user decision, 2026-11-11).

### P20 — Real data: a simulated twin of a real calibration

**Why.** Every result so far is synthetic. The planner predicts σ, the sim lab measures scatter,
and the detectors are checked on renders. None of it says how a real cell behaves: the real
detector's noise, whether the camera model fits the real lens, whether the planner's σ holds. P20
calibrates real datasets and builds each one's **twin**: the same cell, the same robot poses and
the same seen points, simulated. The differences between the real calibration and its twin are
where etendue's simulation is wrong. (User decisions of 2026-11-11: a real-data track after P19,
and the twin first.)

**Data.** calibration-rs's real datasets with robot poses, described by its `dataset.toml`
manifests:
- `kuka_1`: public, one camera on a KUKA arm, a chessboard, 30 poses;
- `DS8`: public, three cameras on a Denso arm, 42 poses;
- `scheimpflug_rig` and `rtv3d`: private. Each is six Scheimpflug cameras with lasers, eye-to-hand,
  ChArUco, with real laser images.

**Private data.**
- A dataset is named by id in a git-ignored `local/datasets.toml` (id → manifest path).
- Committed measurements may name a private dataset and give aggregate numbers only: reprojection
  error, σ, ratios, verdicts.
- Images, robot poses, calibrated parameters and paths never leave the machine.
- Gates on private data run locally. CI runs the public ones where it can (`kuka_1` is in
  calibration-rs's LFS).

**Steps** (one PR each):

- **P20a — Calibrate a real dataset.** `etendue real calibrate <id | dataset.toml>`.
  - It runs calibration-rs's dataset runner (`calibrate_dataset`: detection, cached, then the
    manifest's workflow) and reports what a run without truth can: residuals per feature and per
    view, σ, the quality verdict, and the views the runner rejected, with reasons.
  - It is kept as a run, like a synthetic one, and `Report` gains a "no truth" form.
  - **Gate R1:** on `kuka_1` and `DS8`, the report's estimates and residuals equal calibration-rs's
    own run of the same manifest (the same code: any difference is etendue's).
- **P20b — The twin.** From a real calibration, `etendue-synth` builds a `GroundTruth` directly:
  - the calibrated cameras, rig and hand-eye as the truth;
  - the manifest's target as calibration-rs lays it out;
  - the real robot poses;
  - in each view, exactly the points the real detector found.

  No scene or robot model is needed, since a real cell's robot may not be in the library.
  Observations get noise at the level the real residuals imply (calibration-rs's `noise`), and
  the twin is calibrated with the same workflow and config, N trials.
  - **Gate T1:** the twin recovers its truth on exact data (the G5.1 limits), and its scatter over
    trials matches its predicted σ (std/σ within 15 %, as G17.1).
  - **Characterisation T2** (`docs/measurements/p20_twin.md`), real against twin, per camera:
    - residual RMS ratio;
    - residual whiteness: spatial structure across the image, and per view;
    - the real σ against the twin's scatter;
    - the real result's distance from the twin's estimate, in σ.

    A ratio near 1 with white residuals means the noise model and camera model hold. Structure
    means a model error, and its shape says which.
- **P20c — The planner on real poses.** The P18 planner's σ for the real pose set (from the
  twin), against the real covariance and a split-half check on the real data: calibrate two halves
  of the views, and the difference of their estimates against √2·σ.
  - **Gate P1:** the planner's system σ is within 25 % of the split-half estimate on `kuka_1` and
    `DS8`. The bar is set after T2, and recorded as measured if it is mis-set.
- **P20d — Laser rigs.**
  - etendue's stripe detector (`detect/laser.rs`) becomes a `LaserPixelExtractor` for the dataset
    runner, so the private rigs' laser images calibrate their planes.
  - The twin gains the laser: stripes simulated at the same poses, and G5.3c triangulation on the
    twin against the real stripe residuals.
- **P20e — Studio.** Open a dataset by id, calibrate it, and show the real report beside its
  twin's: residual maps, σ, verdicts.

**Not in P20:** real-image import into a scene (the twin needs none), and robot models for
real cells.

### Deferred (not planned; do not scaffold)

Motion blur and rolling shutter (on-the-fly capture), a Mitsuba 3 backend, a telecentric
projection model (upstream to calibration-rs first), and collision checking. The voxelized
working volume left this list on 2026-10-19 (P11). After P8–P13:
- radiometry and SNR (laser power to electrons per pixel, using the sensors' QE, full well
  and read noise);
- MTF;
- occlusion ray casting;
- a Monte Carlo sweeps UI;
- OPW branches in wasm and the reach envelope;
- calibration in the browser (calibration-rs optim without `getrandom` and `rayon`);
- `ApertureModel` upstream.

## 6. CI

- Rust: the existing four gates plus `--all-features` per crate (new features:
  `schemars`, `opw`), a wasm32 build, `emit-schemas --check`, and the layering check.
- Web: `bun run typecheck lint test` per package and the Playwright e2e. The perf harness
  (G2.2, G8.1) needs a GPU and runs locally only (`bun run test:perf`).
- Blender jobs run **locally only**, not in CI. Gates G4.x run via `etendue measure <gate>` and
  write to `docs/measurements/`.

## 7. Publishing

- crates.io: `etendue-{scene,kinematics,synth,core,cli}`. `etendue-wasm` publishes only
  to npm.
- npm: `@etendue/wasm` from this repo; the 3D and workbench packages are published from
  lab-ui under `@vitavision/*` and consumed from npm (the incubated copies were dropped on
  2026-10-17). Use one version train per ecosystem, managed with changesets (npm) and the
  existing release workflow (Rust).
- *Status (2026-11-11):* nothing is on crates.io or npm yet. `release.yml` builds GitHub release
  assets only (v0.2.0, 2026-10-20). Whether to publish to the registries is decided in P19.
- Publish no earlier than: `etendue-scene` after G1.1–G1.2, web packages after G2.2 and
  P2-4, and `etendue-synth` after G4.1–G4.3.

## 8. Open decisions for the user (defaults apply if the user doesn't answer)

- **D1 Naming.** Resolved: Rust crates stay `etendue-*`, and the reusable web packages
  use the `@vitavision/*` scope in lab-ui.
- **D2 Robot set.** Default: UR5e plus one OPW 6-axis arm with a redistributable
  licence (P1-2 records the licence of each candidate).
- **D3 Blender pin.** Default: the version currently installed, recorded in `etendue.toml`.
- **D4 Studio shell.** Default: plain Vite. Tauri is added only when a native trigger
  fires: launching renders from the UI, or P6-2.

## 9. CLAUDE.md amendments (P0-4)

- Replace "Do NOT use `--all-features` — no feature flags" with the per-crate feature
  matrix from §6.
- Replace the path-dependency section with "crates.io deps + `[patch.crates-io]` for
  local calibration-rs".
- Keep the constraints unchanged: no fork or vendor of calibration-rs, upstream PRs only
  with user review, commit only when asked, and no speculative scaffolding (the §2 tree
  is created phase by phase, never up front).
- Add: "No FK/IK outside `etendue-kinematics`. No camera-model math outside
  calibration-rs. No rendering code in `etendue-core`."
- Add: "Every gate result is written to `docs/measurements/` with the commit SHA."
