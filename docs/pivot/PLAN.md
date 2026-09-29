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
│   └── etendue-ui/          FROZEN    removed at parity gate G6.3
├── web/                     NEW P2    bun workspace
│   ├── apps/studio/                   Vite app (not published); Tauri only if §8 trigger fires
│   └── packages/                      incubating @vitavision/* packages (private), each moved to
│                                      lab-ui by PR once stable: three, three-react (L8-1),
│                                      workbench (new), ui-next (additions to @vitavision/ui)
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
  `vision-calibration-dataset`, and `exr`.
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
  renders. etendue-core's `optics::coc` remains the reference for it.
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

- **P2-7 — Studio desktop shell (Tauri).** D4's trigger fired: launching renders from the UI
  ([ADR 0007](../adrs/0007-tauri-studio-shell.md), proposed). Tauri 2 around the studio, in its own
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
    boards (calib-targets-rs#106).
  - Open: the web viewers still draw a checker, so ChArUco markers and even-row boards need a
    `cells` option in `@vitavision/three` (`docs/measurements/p3_3_target_geometry.md`).

### P4 — Blender backend (photometric tier)

- **P4-1 — Job writer, embedded script, `etendue render --backend blender`.** Cycles on
  the Metal GPU, fixed seed, Filmic/AgX **off** (Standard view transform, linear EXR).
  Done when the example scenario renders end to end and the CLI checks the Blender
  version.
  *Status (2026-09-27):* done. `etendue render` (job.json, embedded script, EXR → LUT →
  PNG); the eye-in-hand example renders 10 captures × 1 camera in 76 s at 16 samples on an
  M4 Pro (Metal). Blender 5.1.1 pinned in `etendue.toml`. Findings: Blender 5 selects
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
  baseline, not a blocker (user decision). chess-corners' **Radon** detector is 2–6× closer than
  the ChESS refiners: 0.032 px RMS at s = 4 on linear renders, 0.046 px on the exact image. The
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
  (`docs/measurements/closed_loop_images.md`). The renders found an IK branch in which the arm
  hid the board (fixed with `initial_q`), and that the GT has no robot self-occlusion yet.
- **P5-2 — Laser sheet in Cycles (spike).** Compare (a) a spot light with a slit
  texture against (b) an area light with near-zero spread behind a slit occluder.
  Measure the across-stripe profile on a matte plane. Done when gate **G5.2** passes:
  the fitted stripe width vs depth is within ≤10% of `etendue-core` `GaussianBeamWidth`
  plus the CoC prediction over the design depth range.
- **P5-3 — Laser closed loop.** Run the calibration-rs rig laserline problem on
  synthetic data. Done when gate **G5.3** passes: plane normal error ≤0.01° and distance
  error ≤0.05 mm.

### P6 — Design overlays and egui retirement

- **P6-1** — Surface the `etendue-core` analyses (defocus map, working volume,
  triangulation angle) as `@etendue/three` overlays via WASM.
- **P6-2 — Performance trigger.** If any analysis takes >100 ms per UI update in WASM
  (measured), escalate by adding the analysis as a Tauri command or as a Web Worker
  with a wasm threads build. Do not do this pre-emptively.
- **P6-3 — Parity gate.** Every `etendue-ui` feature (viewport, parameter panel,
  simulated-image panel, heatmap) has a web equivalent, confirmed by the user. When that
  holds, delete `crates/etendue-ui`.

### Deferred (not planned; do not scaffold)

Motion blur and rolling shutter (on-the-fly capture), a Mitsuba 3 backend, a telecentric
projection model (upstream to calibration-rs first), a voxelized working volume (the
existing roadmap item 2), and collision checking.

## 6. CI

- Rust: the existing four gates plus `--all-features` per crate (new features:
  `schemars`, `opw`), a wasm32 build, `emit-schemas --check`, and the layering check.
- Web: `bun run typecheck lint test` per package, plus the Playwright e2e and perf
  harness (the perf job reports results but only fails on a >20% regression against
  the committed baseline).
- Blender jobs run **locally only**, not in CI. Gates G4.x and G5.x run via
  `cargo xtask measure <gate>` and write to `docs/measurements/`.

## 7. Publishing

- crates.io: `etendue-{scene,kinematics,synth,core,cli}`. `etendue-wasm` publishes only
  to npm.
- npm: `@etendue/wasm` from this repo; the 3D and workbench packages are published from
  lab-ui under `@vitavision/*` (they incubate here unpublished, `private: true`). Use one version train per ecosystem, managed
  with changesets (npm) and the existing release workflow (Rust).
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
