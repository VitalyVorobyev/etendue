# ADR 0003: Kinematics Runs Once, in Rust — Baked Scenarios

- Status: Accepted
- Date: 2026-09-26 (accepted 2026-09-26)

## Context

Three consumers need robot link poses:

- the Rust ground-truth / dataset path (`etendue-synth`);
- the web viewer (scenario playback);
- the Blender render script.

If each one ran its own forward kinematics, there would be three
implementations in three languages, and they would drift. For ground truth,
drift means silent errors.

Off-the-shelf Rust kinematics is ruled out. `k` (openrr) 0.32 requires
nalgebra ^0.30, which conflicts with the 0.34 hard pin, and it was last
released in 2024.

## Decision

- **FK and IK exist only in `etendue-kinematics`.**
  - FK is a custom serial-chain implementation on nalgebra 0.34 over `urdf-rs`
    0.10. It supports revolute, prismatic, and fixed joints, with `origin` and
    `axis`.
  - IK is damped least squares. Analytic OPW IK sits behind the `opw` feature.
- **`etendue bake scene.json scenario.json -o baked.json`** compiles a scenario
  into a `BakedScenario`, sampled at a fixed `dt`:

  ```json
  {
    "version": 1,
    "dt": 0.01,
    "frames": ["world", "robot0/base_link", "robot0/shoulder_link", "...", "rig0", "cam0"],
    "samples": [
      {
        "t": 0.0,
        "world_se3_frame": [ { "rotation": [0,0,0,1], "translation": [0,0,0] }, "..." ],
        "capture": null
      },
      { "t": 1.23, "world_se3_frame": [ "..." ], "capture": { "id": "cap_000" } }
    ]
  }
  ```

  - `world_se3_frame` is index-aligned with `frames`.
  - Every frame in the tree appears in every sample. v1 does not elide static
    frames.
  - Samples may also record joint positions per robot, for display only.
  - The exact field set is fixed by `etendue-scene` in P1-1 and published as
    JSON Schema.
- **Motion model, v1:**
  - Steps are `ptp{q}`, `ptp{base_se3_tool}`, and `lin{base_se3_tool}`.
  - Joints follow trapezoidal profiles under the per-joint velocity and
    acceleration limits from `robot.json`.
  - Capture is **stop-and-shoot only**: a capture sample has zero joint
    velocity.
  - Motion blur and rolling shutter are deferred.
- **Consumers only apply transforms.** The web `FrameTreeRuntime`, the Blender
  script, and the dataset exporter read `world_se3_frame` and never compute a
  pose from joint values.
  - There is no FK in TypeScript or Python.
  - Interactive web editing gets fresh poses by calling `bake` through
    `@etendue/wasm` (P2-1), which is the same Rust code.

## Consequences

- There is one kinematics implementation. It is tested against a
  Pinocchio-generated fixture (G1.1) and FK∘IK round trips (G1.2).
- Any scene or scenario change requires a re-bake. Baking is cheap: it is FK
  per sample.
- Baked files grow as frames × samples × 7 doubles. For example, 30 frames ×
  2000 samples is about 8 MB of JSON. A binary encoding or static-frame elision
  is a later, measured decision.
- Because the capture flag lives in the baked file, the renderer and the
  ground-truth exporter agree by construction on which sample is image *k*.

## Alternatives considered

- **FK in TypeScript** (e.g. a URDF loader with its own joint math). Rejected
  as a second implementation that would drift, and not usable from Blender or
  Rust.
- **Blender armatures / IK.** Rejected for the same reason, and because it
  would make Blender a source of truth rather than a renderer.
- **`k` crate.** Excluded by the nalgebra hard pin (F4).

## References

- `docs/pivot/PLAN.md` §4 (ADR-0003), P1-3 to P1-6, gates G1.1–G1.2
- [ADR 0002](0002-frame-tree.md) (frame names used in `frames`)
