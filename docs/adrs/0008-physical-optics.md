# ADR 0008: Physical Optics in the Scene, and the Component Library

- Status: Accepted (2026-10-19; revised the same day after review: mechanical camera frame,
  explicit cardinal points and pupils, focus breathing)
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
  `CameraEntity::sync_intrinsics_from_physical` sets `fx = f / pitch`. A lens focused
  near images onto a sensor at the conjugate distance `v0 > f`, so for a 25 mm lens
  focused at 0.3 m fx is off by about 9 %.
- **The camera frame has no physical meaning.** A pinhole pose has its origin at the
  centre of projection, which for a real lens is the **entrance pupil**, not a principal
  point. The entrance pupil moves when the lens focuses, and it differs from lens to lens.
  `optics::coc` puts its origin at the rear principal point H′ and its aperture at the
  principal planes. None of these points is fixed to the camera body, so a pose defined at
  any of them changes when the lens is refocused or swapped. That is not what a mount
  drawing, a hand-eye calibration or a rig design means by "where the camera is".
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
  "lens": {
    "name": "Kowa LM25HC",
    "focal_length_m": 0.025,
    "f_number": 4.0,
    "principal_gap_m": 0.0,
    "entrance_pupil_m": 0.0,
    "focusing": { "type": "unit" }
  },
  "focus_distance_m": 0.30,
  "tilt_rad": [0.0, 0.0]
}
```

| Field | Meaning |
|---|---|
| `focal_length_m` | effective focal length `f` |
| `f_number` | `N = f / D_E`, with `D_E` the entrance pupil diameter |
| `principal_gap_m` | signed `g = z_H − z_H′` (positive when H lies on the object side of H′) |
| `entrance_pupil_m` | signed `e = z_E − z_H`, the entrance pupil's position relative to H |
| `focusing` | how the lens focuses; `unit` (the whole lens moves) is the only variant for now |
| `focus_distance_m` | `D`, the distance from the image-plane datum to the plane in focus, as lens focus scales are marked |
| `tilt_rad` | the physical sensor tilt `(τx, τy)` about the image-plane datum point |

- **The physical values are the source of truth.** Everything the pinhole model needs is
  derived from them by the camera model (§3).
- **Distortion stays as-built,** and so does the principal point. cx, cy and the distortion
  are parameters of the built camera; the distortion acts in P's normalised coordinates. The
  focal lengths, the skew and the model tilt are derived (§3).
- **Focus breathing is modelled.** With unit focusing, refocusing moves the lens, its
  principal points and its pupils along the axis. fx changes with them (§3).
- **A camera without the block behaves exactly as today.** Every existing scene stays
  valid. Analyses that need optics report that the camera has none.

### 2. The camera's pose is a mechanical frame

The scene's camera frame, `parent_se3_self` and every pose derived from it, is the
camera's **mechanical frame M**. It is fixed to the body and does not depend on the
optical model.

- **Origin:** the image-plane datum point, where the lens-mount axis meets the nominal,
  untilted image plane (the ⦵ mark on a camera body).
- **Axes:** +Z along the mount axis toward the object, +X along the sensor rows, +Y
  down. These are the calibration-rs/OpenCV axes.
- **What does not move it:** refocusing, changing the aperture, swapping the lens, or
  changing a design parameter. None of them moves M or any frame attached to it.

The frame tree, the bake and the studio see only mechanical frames. The optical points
are positions on M's Z axis, derived from the optics block:

| Point | Position in M (z) |
|---|---|
| S, the sensor pivot (axis ∩ sensor) | `0` |
| H′, the rear principal point | `v0` |
| H, the front principal point | `v0 + g` |
| E, the entrance pupil | `v0 + g + e` |
| E′, the exit pupil | `v0 − s′E`, with `s′E = f·e / (e − f)` |
| F′ and F, the focal points | `v0 − f` and `v0 + g + f` |

- **v0 is set by the focus.** It is the image distance for the focus distance D. With
  `T = D − g`, the object distance `d` (from H) and `v0` (from H′) satisfy `d + v0 = T`
  and `1/d + 1/v0 = 1/f`, so

  ```
  v0 = (T − √(T² − 4fT)) / 2,   d = T − v0
  ```

  This needs `T ≥ 4f`, the minimum object–image distance, which `validate` checks. At
  infinity focus `v0 = f`.
- **The pupil magnification** is `m_p = f / (f − e)`. E and E′ are conjugate.

With `g = e = 0`, the points H, H′, E and E′ coincide, and the model is the thin lens.

### 3. The camera model owns every conversion

A new type, `etendue-core::optics::camera::PhysicalCamera`, is built from a scene camera
through `from_scene`. It is the only code that knows the optical points, and every
caller passes points in M or in the world.

- **Projection frame P.** P has its origin at the entrance pupil E and the axes of M,
  so `M_se3_P` is a translation by `z_E` along Z. The pinhole's centre of projection is
  E, because the chief ray of any object point, in focus or not, passes through it. The
  blur centroid of a defocused point lies on that ray. The nodal points (H, H′ in air)
  give the angle relation only, and they coincide with E only when `e = 0`.
- **The equivalent pinhole about P.** Chief rays leave E′ with slopes divided by `m_p`
  and meet the sensor at `L′ = v0 − s′E` from E′. For an untilted sensor this gives

  ```
  fx = fy = L′ / (m_p · pitch) = m0 · (d − e) / pitch,   m0 = v0 / d
  ```

  With `e = 0` this is `v0 / pitch`, the fix to `sync_intrinsics_from_physical`.

  A physical sensor tilt τ maps to calibration-rs's (OpenCV's) tilt model about P. The
  model decomposes the exact homography from P's normalised coordinates to pixels into
  K·H(τ′).
  - **With `m_p = 1`** the model tilt is the physical tilt, and K is the plain pinhole.
  - **For a tilt about one axis,** `tan τ′ = tan τ / m_p`, and the focal length across the
    tilt axis scales by `cos τ′ / cos τ`.
  - **For a compound tilt with `m_p ≠ 1`,** K has a lower-left term that calibration-rs's
    upper-triangular K cannot hold. A roll ψ of P about its axis, found by bisection,
    removes it. The rest is a small derived skew (−1.4e-3 of fx at τ = (0.1, 0.05) rad,
    m_p = 1.25). ψ goes into `M_se3_P`.
- **Circle of confusion in the H′ frame.** The existing derivation is kept and
  generalised from an aperture at the principal planes to the true exit pupil. The blur is
  the exit pupil, of diameter `D_X = m_p · f / N`, projected from the image point onto the
  tilted sensor:

  ```
  c = D_X · |s_i − v_sensor| / (s_i − s′E)
  ```

  Here `s_i` and `s′E` are measured from H′. With `e = 0` and `g = 0` this is the current
  formula, and its tests (including the regime (a)/(b) guards) keep passing unchanged. The
  plane of best focus depends on the conjugates only, not on the pupils: it is the
  conjugate of the sensor plane. The model computes the blur from explicit positions in M.
  - **Exact:** without tilt; and with tilt, the zero locus (the plane of best focus) and
    the blur's centre.
  - **First order with tilt, off that plane:** `v_sensor` is read at the image's lateral
    position, where the oblique chief ray meets the tilted sensor beside it. Over a
    ±0.2 rad field the diameter is within `0.6·tan θ` (relative) of the traced ellipse's
    mean diameter, with θ the sensor's tilt from the axis (measured: 0.54·tan θ at most).
    An exact blur ellipse comes with the Optics workspace (P10), whose optimiser needs it.
- **Its API takes mechanical inputs only:**
  - `project` / `backproject`: points in M, with rays returned in M;
  - `coc`, `blur` and `plane_of_best_focus`, in M;
  - `pinhole() → (M_se3_P, CameraParams)`;
  - `cardinal_points()`, for drawing.

  Nothing else applies an optical offset.
- **Consumers that need a pinhole pose get it from one resolution.**
  `etendue-core::from_scene::resolve` returns the scene in projection frames:
  - each camera with optics is moved to P and given the derived pinhole;
  - everything attached to it is re-expressed in P, so its world pose is unchanged.

  The CLI's `load` and the wasm session both work on that scene. So the ground truth and
  calibration-rs datasets (calibration estimates P), the Blender job's cameras, the remap
  tables, every wasm projection and the studio's frustum all see P, and none of them meets
  the optics. The ground truth also records `M_se3_P`, so a calibration result can be
  compared in either frame. The studio edits bodies: it maps a baked camera frame back
  through `mechSe3Projection` where it shows or moves one.
- **The stored params are the equivalent pinhole.** For a camera with optics, the model
  writes fx, fy, the skew and the tilt in `params` as the equivalent pinhole about P. `validate` (CLI
  and wasm) rejects any that differ from the derived values by more than 1e-9 relative.
  They are a readable copy, and nothing pairs them with the mechanical pose.
- **A camera without optics** has `M_se3_P = I` and its `params` as stored. Every current
  path stays bit-identical (G0.1).

### 4. Frame invariance is tested

The simulated image must not depend on where the frames are drawn. The tests (gate G9.4):

1. **Physical camera = derived pinhole.** A camera with optics at `world_se3_M` and a
   plain pinhole at `world_se3_P` with the derived params give the same results through
   every consumer: ground-truth observations, the render job's cameras and the wasm
   projections. The results must agree to 1e-9 px.
2. **Chief-ray trace.** An independent paraxial y–u trace in M is the reference. It
   traces the chief ray through E and E′, and the marginal rays through the pupil edges,
   onto the tilted sensor. It agrees with `project` to 1e-9 px and with `coc` to 1e-12
   relative. The cases cover `g ≠ 0`, `e ≠ 0` and tilt, on and off the plane of best
   focus.
3. **Rigid invariance.** Moving the whole cell, or re-expressing the camera under a
   different parent with a compensating pose, leaves every pixel and CoC unchanged to
   1e-9.
4. **Refocus keeps M.** Refocusing moves E, H and H′ inside the model while
   `world_se3_M` stays fixed. A point on the new focus plane has CoC 0, and its pixel
   matches the trace.
5. **Thin-lens limit.** With `g = e = 0`: `fx = f·d / ((d − f)·pitch)`, the near and
   far depth of field match the textbook formulas, and `M_se3_P` is a translation by
   `v0`. All are checked at 1e-12 relative (G9.1).

### 5. A component library of versioned documents

`etendue-scene` gets `library.rs`. Each item is one JSON file. It holds exactly one part, under
the key that names its kind:

```json
{ "version": 1, "id": "imx174_lm25hc", "name": "IMX174 + LM25HC, f/4 at 0.5 m", "camera": { … } }
```

(The first draft had a flat `{version, kind, id, name, …fields}`. serde cannot reject unknown
fields in a flattened document, so the part is nested.)

- **Kinds:**
  - parts: `sensor`, `lens`, `laser`, `target`;
  - designs:
    - `camera`: optics, resolution, distortion and tilt (a `CameraTemplate`);
    - `head`: a camera design, a laser and `camera_se3_laser` in the camera's mechanical
      frame, plus its design volume. It comes with the Optics workspace (P10), which is the
      first thing to make one;
    - `rig`: a `RigLayout` whose unit is a camera or head design.
- **The `etendue-core::bank` schema types move here as the part kinds.** A lens part
  gains `entrance_pupil_m` and `focusing`. The catalog loader and the seed files move to
  the project library (ADR 0009). The core keeps only math, which takes plain numbers or
  scene types.
- **`version` is checked before anything else.** Documents are `deny_unknown_fields`, so
  an older binary must refuse a newer file with a version error, not a field error.
- **Inside the library, designs embed their parts by value.** A head contains its camera
  design; it does not refer to it. A library file is therefore complete on its own.
- **Scenes copy designs by value and record where they came from.**
  `CameraSpec`, `LaserSpec` and `RigSpec` gain an optional `source: { "library": "<kind>/<id>" }`.
  A scene never depends on a library being present, and a dataset made from it never
  dangles. The studio compares content to offer "update from library".
- **Schemas.** They are emitted into `schemas/library.schema.json`, and CI checks them for
  drift like the others.

### 6. Optics physics added to `etendue-core`

All of it is model-based: paraxial, with no ray tracing. All of it goes through
`PhysicalCamera`.

- `optics::budget`:
  - the working f-number `N_w = N·(1 + |m| / m_p)`;
  - near and far depth-of-field limits and the hyperfocal distance for a blur limit;
  - the Airy diameter `2.44·λ·N_w`;
  - total blur `√(c² + Airy²)`;
  - the field of view at a distance, and mm per pixel.
- `optics::paraxial`: y–u ray transfer through the thick lens with its pupils. It is
  the reference trace of §4, and it gives the chief and marginal ray fans for drawing.
- `analysis::triangulation`: depth resolution (dZ per pixel) and triangulation angle
  over the laser fan plane, from the projection Jacobian along the plane.
- The existing `working_volume` grows per-cell blur, mm/px and dZ/px outputs. It
  remains a 2-D region on the fan plane.

### 7. The Scheimpflug solver drops argmin

A deterministic Nelder–Mead (`solver::nelder_mead`, no RNG, no dependencies) replaces
argmin. Before the change, the solver's optimum on the existing test cases is recorded.
The port must reproduce it to 1e-6 rad and 1e-6 m (gate G9.2). The solver's variables
become the physical ones: τ, D and N.

The `solver` feature is removed, and the solver is always built. It is exposed to wasm,
whose graph keeps no `getrandom` (gate G9.3). argmin leaves `Cargo.toml`,
`Cargo.lock` and `deny.toml`.

### 8. The optics reach the browser through wasm

`etendue-wasm` exposes the model to the studio. Each function takes JSON and returns JSON or
typed arrays:

- `deriveOptics(optics, params)`: the derived pinhole, `M_se3_P`, the cardinal points,
  magnification, pupil diameters and the working f-number (P9b);
- `EtendueScene.mechSe3Projection(camera)`: where a camera's projection frame sits in its body.
  The session works in projection frames (§3), so a baked camera frame is P;
- with the workspaces that need them: `blurProfile`, `crossSection`, `workingVolume`
  (P10), `solveScheimpflug` (P9c), `validateLibrary` (P9d).

As for every kernel function, native and wasm results are checked bit for bit (the
G0.1 recipe). If an analysis exceeds 100 ms per UI update, it moves to a worker or a
Tauri command (P6-2).

### 9. Findings while implementing (2026-10-19)

The model's tests, which compare it with an independent paraxial trace, found three defects
in the legacy core:

- **Mirrored τx.** calibration-rs's tilted plane is the virtual image plane in front of the
  projection centre. The real sensor is its reflection through the lens. So calibration-rs's
  `tilt_x` is the negative of `optics::coc`'s `tau_x`; `tilt_y` agrees.
  - `ThickLens` and the Scheimpflug solver passed the angles through unchanged. For a camera
    tilted about x, that mirrored the plane of best focus, every blur computed from it, and the
    solver's proposal.
  - Both now convert (`optics::coc_tilt`, CLAUDE.md gotcha 4).
  - A test pins the legacy plane of best focus against `PhysicalCamera`: exact for a tilt
    about one axis. For a compound tilt it agrees within `1 − cos τx`, because the legacy
    model composes the axes independently.
- **H's position.** The legacy text put H at `z = +g`, but its formulas use `d_H = z + g`,
  which puts H at `z = −g`. The text is corrected; the formulas and their tests are
  unchanged.
- **The solver stalled.** argmin's Nelder–Mead stopped at 0.49 px worst blur where the
  optimum is 0. The cost is steep in focus and one tilt, and nearly flat in the other.
  Restarting from the best point with a fresh simplex reaches 4e-6 px. The in-tree port (§7)
  keeps the restarts, and G9.2 is recorded on the restarted solver.

## Consequences

- **Poses mean what a mount drawing means.** A camera, a head or a rig unit is placed by
  its body. Refocusing or changing the lens in the Optics workspace never moves it in the
  cell.
- **Hand-eye ground truth is reported for P.** Calibration recovers the pinhole frame P,
  which moves with focus. The ground truth carries `M_se3_P`, so the lab can report the
  result in either frame.
- **Scenes stay backward compatible.** `optics` and `source` are optional. For a camera
  without optics, M is P, and the examples validate unchanged.
- **One more rule for consumers.** Code that needs a pinhole pose must take it from the
  camera model. The equivalence test (§4.1) runs every consumer on a physical camera, so a
  consumer that pairs `params` with the mechanical pose fails it.
- **The core's CoC changes only for `e ≠ 0`.** The H′ derivation and its regime split
  stay. `docs/derivations/` gains a note on the camera frames and the pupil
  generalisation.
- **CLAUDE.md gotcha 2 is restated.** For a camera with optics, the physical tilt lives
  in `optics.tilt_rad`, and `params.sensor` holds the derived model tilt. A built
  `CameraModel` is still never the source of either.
- **The legacy `etendue-core::scene` model stays.** It is used through `from_scene`.
  Folding it into the scene types is not part of this change.
- **A library item is a file in a project** (ADR 0009). The browser, the shell and the
  CLI all read the same format.
- **CLAUDE.md's feature matrix loses `solver`.**

## Alternatives considered

- **Pose at the front principal point H.** This was this ADR's first draft. Rejected
  on two counts:
  - H is not the centre of projection when the pupil is away from it. A 5 mm pupil offset
    on a 25 mm lens moves fx by 1.8 % at 0.3 m.
  - H moves with focus, so refocusing would move the camera in the cell.
- **Pose at the entrance pupil.** Rejected. It is the right projection centre but not a
  mechanical frame. It moves with focus and with the lens, so a hand-eye or a rig design
  would change with the lens setting.
- **The lens-mount flange as the datum.** Not chosen. It depends on the mount (C, CS) and
  on spacers or Scheimpflug adapters. Focus scales and sensor-tilt pivots refer to the
  image plane. A flange offset can be added to the sensor part later if a mount check
  needs it.
- **Keep optics only in the library, and the scene as pure intrinsics.** Rejected (user
  decision 2026-10-19). A camera in a cell would lose its defocus and working volume,
  which the rig editor and the cell need.
- **Store no equivalent pinhole in `params`.** Considered. Once poses are mechanical,
  every consumer needs the model anyway. The copy is kept because it lets someone read the
  file, and `validate` refuses it when stale. If it causes confusion, it can be dropped
  without changing any consumer.
- **Move the core's origin convention to M.** Not chosen. It would rewrite a proven
  derivation and its tests. The model's shift by `v0` is exact and stays internal.
- **Live references from scenes into the library.** Rejected. A scene, and every dataset
  made from it, would depend on a library file staying unchanged.
- **Keep argmin, and run the solver natively through Tauri.** Rejected. The optimiser is
  interactive, and a browser build would lose it. Nelder–Mead is small.
- **Internal and floating focusing.** Deferred. Such lenses change f as they focus, and
  modelling them needs lens data a datasheet rarely gives. `focusing` is an enum so that
  they can be added.
- **Promote `ThickLens` to calibration-rs (`ApertureModel<S>`) first.** Deferred.
  Calibration estimates no aperture or focus parameter, and nothing in this roadmap
  needs it upstream.

## References

- `docs/pivot/PLAN.md` P9, P10, P11 (gates G9.1–G9.4, G10.x, G11.x)
- `docs/derivations/scheimpflug_pobf.md` (the H′ convention, the hinge rule)
- `crates/etendue-core/src/optics/coc.rs`, `scene/entity.rs` (`sync_intrinsics_from_physical`)
- `vision-calibration-core` `models/sensor.rs` (the OpenCV tilt homography)
- [ADR 0001](0001-pivot.md) §4 (the egui app's features remain to be brought to the studio)
- [ADR 0009](0009-project-folder.md) (where library items live)
