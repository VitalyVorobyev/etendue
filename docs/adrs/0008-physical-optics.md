# ADR 0008: Physical Optics in the Scene, and the Component Library

- Status: Proposed
- Date: 2026-10-19

## Context

etendue started as an optical design tool: thick-lens defocus, the Scheimpflug plane of
best focus, laser triangulation and the working volume. That model lives in
`etendue-core`, and the egui app used to expose it. Since the egui app was deleted
(ADR 0001 §4), the studio can no longer reach it:

- **Scene cameras carry no physical optics.** `CameraSpec` holds only calibration-rs
  `CameraParams` (fx, fy, cx, cy, skew, distortion, sensor tilt) and a resolution. A
  camera placed in a cell cannot report its defocus or working volume.
- **There is no bridge.** `etendue-core` keeps its own legacy scene model
  (`CameraEntity`, `LaserEntity`, `PhysicalOptics`). It does not depend on
  `etendue-scene`, although the layering rules allow it to.
- **The intrinsics derivation is wrong away from infinity focus.**
  `CameraEntity::sync_intrinsics_from_physical` sets `fx = f / pitch`. A lens focused at
  `s` images onto a sensor at the conjugate distance `v0 = f·d/(d − f)`, not at `f`.
  For a 25 mm lens focused at 0.3 m that is a 9 % error in fx.
- **The camera origin is ambiguous.** `optics::coc` puts the camera origin at the rear
  principal plane H′, with H a distance `g` toward the object. A pinhole model has to
  put its perspective centre at the front nodal point. In air that is H. Mixing the two
  conventions moves every camera by `g` along its axis.
- **The component bank goes unused.** `etendue-core::bank` (sensor, lens and laser
  specs with seed JSON) has no UI and no home in any document.
- **The Scheimpflug solver cannot run in the browser.** It sits behind the `solver`
  feature on argmin, which pulls `getrandom`, and the wasm graph must not contain it.
  No crate enables the feature.

The roadmap (PLAN P8–P13) adds an Optics workspace, a rig editor built from identical
units, and a persistent library. Each of these needs the physical description of a
camera to be a document, not a transient form.

## Decision

### 1. A camera may carry its physical optics

`CameraSpec` gains an optional `optics` block. `RigLayout`'s `CameraTemplate` gains the
same block.

```json
"optics": {
  "sensor": { "name": "Sony IMX174", "pixel_pitch_m": 5.86e-6 },
  "lens":   { "name": "Kowa LM25HC", "focal_length_m": 0.025, "f_number": 4.0, "principal_gap_m": 0.0 },
  "focus_distance_m": 0.30
}
```

- **The physical values are the source of truth.** fx and fy stay in `params`, because
  calibration, projection and every consumer of a scene read them. They are **derived**:
  `fx = fy = v0 / pitch`, with `v0` the conjugate image distance at the focus setting.
  `validate` (CLI and wasm) rejects a camera whose fx or fy differs from the derived
  value by more than 1e-9 relative. Editors write both fields together.
- **Only fx and fy are derived.** cx, cy, skew, distortion and the Scheimpflug tilt
  remain as-built parameters of the camera, and the optics block does not touch them.
  The tilt stays in `params.sensor` (CLAUDE.md gotcha 2), and the defocus model reads it
  from there.
- **Focus breathing is modelled.** Changing the focus distance changes fx, as it does
  for a unit-focusing lens.
- **A camera without the block behaves exactly as today.** Every existing scene stays
  valid, and the analyses that need optics report that the camera has none.

### 2. The camera origin is the front principal point H

The scene's camera frame, `world_se3_camera`, has its origin at **H**: the front
principal point, which is the perspective centre of the pinhole model. `focus_distance_m`
is measured from H along +Z.

`etendue-core`'s derivation keeps its H′ origin (`docs/derivations/scheimpflug_pobf.md`
is unchanged). The new bridge `etendue-core::from_scene` converts at the boundary:

- a point at scene camera depth `z` sits at core depth `z − g`;
- the scene focus distance `d` becomes core `s_o = d − g`;
- `v0 = image_distance(d, f)`.

With `g = 0` the two conventions coincide, and every formula reduces to the thin lens.

### 3. Fix the intrinsics derivation

`CameraEntity::focal_length_px` becomes `v0 / pitch`. The tests that pinned `f / pitch`
change. The thin-lens limit (`g = 0`) is checked against `fx = f·d / ((d − f)·p)` at
1e-12 relative (gate G9.1).

### 4. A component library of versioned documents

`etendue-scene` gets `library.rs`. Each item is one JSON file:

```json
{ "version": 1, "kind": "camera", "id": "imx174_lm25hc", "name": "IMX174 + LM25HC @ 0.3 m", … }
```

- **Kinds:**
  - parts: `sensor`, `lens`, `laser`, `target`;
  - designs: `camera` (optics, resolution, distortion, tilt), `head` (a camera design, a
    laser and `camera_se3_laser`, plus its design volume), `rig` (a `RigLayout` whose
    unit is a camera or head design).
- **The `etendue-core::bank` schema types move here as the part kinds.** The catalog
  loader and the seed files move to the project library (ADR 0009). The core keeps only
  math, which takes plain numbers or scene types.
- **`version` is checked before anything else.** Documents are `deny_unknown_fields`, so
  an older binary must refuse a newer file with a version error, not a field error.
- **Inside the library, designs embed their parts by value.** A head contains its camera
  design; it does not refer to it. A library file is therefore complete on its own.
- **Scenes copy designs by value and record where they came from.**
  `CameraSpec`, `LaserSpec` and `RigSpec` gain an optional `source: { "library": "<kind>/<id>" }`.
  A scene never depends on a library being present. A dataset made from it never
  dangles. The studio compares content to offer "update from library".
- The schemas are emitted into `schemas/library.schema.json`, and CI checks them for
  drift like the others.

### 5. Optics physics added to `etendue-core`

All of it is model-based: paraxial, with no ray tracing.

- `optics::budget`:
  - the effective f-number `N(1 + m)`;
  - near and far depth-of-field limits and the hyperfocal distance for a blur limit;
  - the Airy diameter `2.44·λ·N_eff`;
  - total blur `√(CoC² + Airy²)`;
  - the field of view at a distance, and mm per pixel.
- `optics::paraxial`: y–u ray transfer through the thick lens. It gives chief and
  marginal ray fans for drawing.
- `analysis::triangulation`: depth resolution (dZ per pixel) and triangulation angle
  over the laser fan plane, from the projection Jacobian along the plane.
- The existing `working_volume` grows per-cell blur, mm/px and dZ/px outputs. It
  remains a 2-D region on the fan plane.

### 6. The Scheimpflug solver drops argmin

A deterministic Nelder–Mead (`solver::nelder_mead`, no RNG, no dependencies) replaces
argmin. Before the change, the solver's optimum on the existing test cases is recorded.
The port must reproduce it to 1e-6 rad and 1e-6 m (gate G9.2).

The `solver` feature is removed, and the solver is always built. It is exposed to wasm,
whose graph keeps no `getrandom` (gate G9.3). argmin leaves `Cargo.toml`,
`Cargo.lock` and `deny.toml`.

### 7. The optics reach the browser through wasm

`etendue-wasm` gains an `optics` module. Each function takes JSON and returns JSON or
typed arrays:

- `deriveIntrinsics`, `opticsSummary`, `blurProfile`, `crossSection`;
- `workingVolume`, `solveScheimpflug`, `validateLibrary`.

As for every kernel function, native and wasm results are checked bit for bit (the
G0.1 recipe). If an analysis exceeds 100 ms per UI update, it moves to a worker or a
Tauri command (P6-2).

## Consequences

- **Scenes stay backward compatible.** `optics` and `source` are optional, and the
  examples validate unchanged.
- **The legacy `etendue-core::scene` model stays.** It is used through `from_scene`.
  Folding it into the scene types is not part of this change.
- **The analyses get one entry point from a scene.** The defocus map, the working
  volume and the solver all take a scene camera through the bridge.
- **The intrinsics fix changes numbers.** It changes the fx any physical-optics code
  path produces. No committed scene used that path, so no example changes.
- **A library item is a file in a project** (ADR 0009). The browser, the shell and the
  CLI all read the same format.
- **CLAUDE.md's feature matrix loses `solver`.**

## Alternatives considered

- **Keep optics only in the library, and the scene as pure intrinsics.** Rejected (user
  decision 2026-10-19). A camera in a cell would lose its defocus and working volume,
  which the rig editor and the cell need.
- **Drop fx and fy from cameras that carry optics.** Rejected. Every consumer
  (calibration-rs datasets, projection, Blender jobs) would need the optics model.
  Storing the derived values, and validating them, keeps those consumers unchanged.
- **Move the core's origin convention to H.** Not chosen. It would rewrite a proven
  derivation and its tests for no gain. A shift by `g` at the bridge is exact.
- **Live references from scenes into the library.** Rejected. A scene, and every dataset
  made from it, would depend on a library file staying unchanged.
- **Keep argmin, and run the solver natively through Tauri.** Rejected. The optimiser is
  interactive, and a browser build would lose it. Nelder–Mead is small.
- **Promote `ThickLens` to calibration-rs (`ApertureModel<S>`) first.** Deferred.
  Calibration estimates no aperture or focus parameter, and nothing in this roadmap
  needs it upstream.

## References

- `docs/pivot/PLAN.md` P9, P10, P11 (gates G9.1–G9.3, G10.x, G11.x)
- `docs/derivations/scheimpflug_pobf.md` (the H′ convention, the hinge rule)
- `crates/etendue-core/src/optics/coc.rs`, `scene/entity.rs` (`sync_intrinsics_from_physical`)
- [ADR 0001](0001-pivot.md) §4 (the egui app's features remain to be brought to the studio)
- [ADR 0009](0009-project-folder.md) (where library items live)
