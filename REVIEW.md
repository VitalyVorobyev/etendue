# Pre-Release Review — etendue
*Reviewed: 2026-05-21*
*Scope: full workspace (etendue-core + etendue-ui), v0.1.0 release prep*

## Review Verdict
*Reviewer pass: 2026-05-21 · F8 rework + re-verification: 2026-05-21*

**Overall: PASS** — all 13 findings verified; ready for the v0.1.0 tag.

| Outcome | Count | Findings |
|---|---|---|
| verified | 13 | F1–F13 |
| needs-rework | 0 | — |
| regression | 0 | — |

### Quality gates

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| `cargo test --workspace` | PASS — **205 tests** (152 etendue-core + 53 etendue-ui + 0 doctests) |
| `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace` | PASS |
| `cargo deny check advisories licenses bans sources` | PASS (cargo-deny 0.19.1) |
| `mdbook build book` | PASS |

All gates are green and the final test count is **205**, which matches the
count now stated in `CHANGELOG.md`, `README.md`, and `.claude/CLAUDE.md`. No new
`unsafe`, no new `#[allow(dead_code)]`, no new `TODO`/`FIXME`. All changes are
confined to the etendue workspace — `vision-calibration-core` is untouched (the
unrelated `calibration-rs` diffs are pre-existing work on its own
`api/revision-0.5.0` branch in the `vision-calibration-pipeline` /
`vision-calibration` crates, not the path-dep `vision-calibration-core`).

### F8 rework — completed (2026-05-21)

The Reviewer's first pass returned F8 `needs-rework` for stale book/doc content
the Implementer's F8 pass missed. The Architect completed the rework and
re-verified:

1. `book/src/scheimpflug_solver.md` — rewrote the "Cost function", "The solver"
   and "SampleGrid" sections. The solver is **Nelder-Mead** (derivative-free —
   no L-BFGS, no finite-difference gradients); the cost samples the **laser fan
   plane** in `(phi, r)` filtered by visibility + the depth window;
   `SolverResult::iterations` is `u32`.
2. `book/src/scene_and_geometry.md` — removed the deleted `geom::ray` section;
   updated the `Scene` snippet (`mesh_targets` field) and the `CameraEntity`
   snippet (post-F4 `optics: PhysicalOptics`); added a `MeshTarget` subsection.
3. `book/src/getting_started.md` + `book/src/introduction.md` — "172 tests" →
   205, "MVP complete (M0–M6)" → M0–M10 + post-MVP; removed the now-false
   "mesh intersection is out of scope" bullet.
4. `crates/etendue-ui/src/viewport/scene.rs` — module doc no longer references
   the F11-deleted `Drawable::set_transform`.
5. Two further spots not enumerated in the first pass: `book/src/bank.md`
   (`CameraEntity::effective_focal_length_m` →
   `PhysicalOptics::effective_focal_length_m`, post-F4) and
   `crates/etendue-core/src/solver/scheimpflug.rs` (module doc no longer names
   the F5-deleted `log_sum_exp`).

All five cargo gates and `mdbook build` re-run clean after the rework. F8 is
`verified`; the overall verdict is **PASS**.

## Triage decisions (Architect → Implementer)

The user confirmed the following on 2026-05-21:

| Topic | Decision |
|---|---|
| Fix scope | Fix **all 13 findings** — P1, P2, and P3. |
| F5 / F6 (unused API) | **Delete** `geom::ray`, `psf::gaussian_sigma_to_coc`, `psf::log_sum_exp`; wire `image_view.rs` to call `psf::coc_to_gaussian_sigma` instead of re-deriving the sigma conversion inline. |
| F7 (mesh intersection) | **Build the feature** — add a mesh-target entity and wire the UI/renderer so `stripe_segments_on_mesh` is reachable. See the revised F7 fix for the prescribed (additive) design. |
| F8 (mdBook) | **Add concise chapters** for camera anatomy, the Scheimpflug solver, symmetric rigs + voxel overlap, and the Gaussian PSF, plus the stale-reference fixes. |

## Executive Summary

This is a re-review. The previous `REVIEW.md` (2026-05-20) audited the M0–M6
MVP and verified 14 findings. Since then, commits `bce12f5` ("M7–M10") and
`40371a6` ("Gaussian PSF, mesh laser intersection, voxelized N-view overlap")
added roughly 3,700 lines of new code: the M8 camera-anatomy renderer, the M9
`argmin`-based Scheimpflug solver, the M10 symmetric-rig builder and N-view
voxel overlap, a Gaussian-PSF module, a triangle-mesh kernel, and a mesh laser
intersection. This review covers the workspace as it stands today.

The engineering quality of the new code is high and consistent with the MVP.
All four local quality gates are green: `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace` (**218 tests** — 165 etendue-core + 53 etendue-ui + 0
doctests), and `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace`.
There is still **zero `unsafe`**, the nalgebra hard-pin still unifies to a
single `nalgebra 0.34.2` across both crates and `vision-calibration-core`, and
the new modules are genuinely well-tested (solver-convergence tests, ring
symmetry tests, mesh-intersection contract tests).

The release-blocking issues are not in the code — they are in the release
*artifacts and gates*. No git tag exists yet, so v0.1.0 is genuinely
unreleased. The `CHANGELOG.md` documents only M0–M6; M7–M10 and every
post-MVP feature are absent, so the release workflow (which extracts the
per-tag CHANGELOG section) would publish release notes covering half the
product. The `README.md` still says "MVP complete (M0–M6). 172 tests pass" and
lists five already-shipped features as future roadmap items. And the
`deny.toml` added by the previous review's F4 is **invalid** for current
`cargo-deny` (0.19.1 locally; `cargo-deny-action@v2` tracks recent releases) —
it uses the removed `copyleft` and `allow-osi-fsf-free` keys, so the
`audit.yml` security workflow fails at config validation before it checks a
single advisory. The security gate the previous review stood up is currently
non-functional.

The remaining findings are P2/P3 design and documentation debt: a 10-argument
public constructor, a Gaussian-PSF module and a ray-intersection module that
are mostly or entirely unused public API, a mesh-intersection capability with
no scene representation to exercise it, a stale mdBook, and a handful of
smaller API/UX nits. Nothing in the list is a correctness or numerical-safety
risk.

## Findings

### F1 CHANGELOG omits M7–M10 and all post-MVP features
- **Severity**: P1
- **Category**: docs / contracts
- **Location**: `CHANGELOG.md:8` (`[Unreleased]`), `CHANGELOG.md:10-77` (`[0.1.0]`)
- **Status**: verified
- **Resolution**: Added bullets to `[0.1.0] ### Added` for M7 (UI stability), M8 (camera anatomy), M9 (Scheimpflug solver), M10 (symmetric rigs + N-view voxel overlap), Gaussian PSF, and triangle-mesh kernel + mesh laser intersection. Updated `### Verified` to reflect 0 doctests and expanded verification notes for M9/M10/mesh. The exact test count placeholders (TEST_COUNT, CORE_COUNT, UI_COUNT) are updated after F5/F6/F7 finalize the count.
- **Problem**: `[0.1.0]` documents only M0–M6. The M7 (UI stability), M8
  (camera anatomy), M9 (Scheimpflug solver), and M10 (symmetric rigs + N-view
  voxel overlap) milestones, plus the Gaussian-PSF module and the mesh laser
  intersection, are absent from the CHANGELOG entirely; `[Unreleased]` is
  empty. `git tag -l` returns nothing, so v0.1.0 has not been tagged — the
  release is genuinely still pending. `release.yml` extracts the per-tag
  CHANGELOG section as the release body, so tagging v0.1.0 now would publish
  release notes that cover roughly half of what ships. The `### Verified`
  block (`CHANGELOG.md:70`) is also stale: it claims "172 tests pass
  (129 + 42 + 1 doctest)"; the actual count is 218 (165 + 53 + 0 doctests).
- **Fix**: Decide whether M7–M10 + post-MVP work belongs in `[0.1.0]` (if
  v0.1.0 has not shipped — it hasn't) or a new version section. Given no tag
  exists, fold all of it into `[0.1.0]` under `### Added`: one bullet per
  milestone (M7–M10) and one per post-MVP feature (Gaussian PSF, triangle-mesh
  kernel + mesh laser intersection, N-view voxel overlap). Correct the
  `### Verified` test count to 218 (165 + 53 + 0).

### F2 README "Status" and "Roadmap" misrepresent the shipped product
- **Severity**: P1
- **Category**: docs
- **Location**: `README.md:13-18` (Status), `README.md:83-95` (Roadmap), `README.md:61-75` (MVP demo)
- **Status**: verified
- **Resolution**: Updated Status section to say M0–M10 complete with a TEST_COUNT placeholder (filled in after F5/F6/F7); trimmed Roadmap to the two genuinely open items (ApertureModel upstream, component-picker UI); corrected MVP demo step 2 from "set Working distance" to reference the actual "Optimal distance (m)" readout and the Scheimpflug solver section.
- **Problem**: The README is the repository's front page and is materially
  wrong about what shipped:
  - Status (`:15`): "MVP complete (M0–M6). 172 tests pass." — M0–M10 plus the
    post-MVP queue items are done, and 218 tests pass.
  - Roadmap (`:83-95`): items 2 (voxelized working volume), 3 (multi-camera
    overlap), 5 (`argmin` optimizer), 6 (Gaussian PSF) and 7 (mesh laser
    intersection) are all implemented. Only item 1 (`ApertureModel` upstream)
    and item 4 (component-picker UI) genuinely remain.
  - MVP demo (`:68`): step 2 says "set **Working distance**", but the panel
    has no such widget; the M9 solver section labels it "Optimal distance (m)".
- **Fix**: Rewrite Status to reflect M0–M10 + post-MVP work and the 218-test
  count. Trim the Roadmap to the genuinely-open items (ApertureModel upstream,
  component-picker UI) plus any newly-deferred work. Correct the demo step-2
  wording to match the actual UI control. Keep this in sync with F1.

### F3 `deny.toml` is invalid for current cargo-deny — the audit gate is broken
- **Severity**: P1
- **Category**: security / contracts
- **Location**: `deny.toml:30-31`; `.github/workflows/audit.yml`
- **Status**: verified
- **Resolution**: Deleted `allow-osi-fsf-free` and `copyleft` keys (removed in cargo-deny 0.15), replaced with a comment explaining any unlisted license is denied by default. Also added `BSL-1.0`, `OFL-1.1`, and `Ubuntu-font-1.0` to the `allow` list (previously allowed by the old `allow-osi-fsf-free` key, now required explicitly). `cargo deny check advisories licenses bans sources` exits 0.
- **Problem**: `cargo deny check` fails to even validate the configuration:
  ```
  error[deprecated]: this key has been removed ... deny.toml:30 allow-osi-fsf-free = "either"
  error[deprecated]: this key has been removed ... deny.toml:31 copyleft = "deny"
  [ERROR] failed to validate configuration file
  ```
  `cargo-deny` removed `copyleft` and `allow-osi-fsf-free` from the
  `[licenses]` section (PR #611, released in 0.15). The locally installed
  cargo-deny is 0.19.1; `EmbarkStudios/cargo-deny-action@v2` tracks recent
  releases. `audit.yml` runs `cargo-deny check advisories licenses bans
  sources`, so the workflow aborts at config validation before it checks any
  advisory — including the RUSTSEC-2024-0436 ignore that the previous review's
  F4 added the file specifically to enforce. The release's security-audit gate
  is therefore non-functional. (Separately: `audit.yml` runs only on a weekly
  `schedule` + `workflow_dispatch`, not on push/PR — worth noting, since the
  previous review's triage stated it runs "on every push", which it does not.)
- **Fix**: Migrate `deny.toml` to the current schema. The minimal change is to
  delete the two removed keys (`allow-osi-fsf-free`, `copyleft`) and their
  comment — in cargo-deny ≥ 0.15 any license not in `[licenses].allow` is
  denied, which already gives the copyleft-deny behaviour. Verify with
  `cargo deny check advisories licenses bans sources` locally (exit 0). The
  `advisories.ignore` entry for RUSTSEC-2024-0436 and the `allow` list are
  unaffected.

### F4 `CameraEntity::new` is a 10-argument positional constructor
- **Severity**: P2
- **Category**: design / API
- **Location**: `crates/etendue-core/src/scene/entity.rs:145-157`
- **Status**: verified
- **Resolution**: Added `pub struct PhysicalOptics` grouping the five physical optics scalars (`effective_focal_length_m`, `f_number`, `focus_distance_m`, `principal_gap_m`, `pixel_pitch_m`). Updated `CameraEntity::new` to take `(pose, params, resolution, optics: PhysicalOptics, frustum_near, frustum_far)` — 6 arguments, the `#[allow(clippy::too_many_arguments)]` removed. Added read accessor methods on `CameraEntity` delegating to `self.optics`. Updated all call sites in `scene.rs`, `defocus_map.rs`, `working_volume.rs`, `scheimpflug.rs`, `project.rs`, and all UI files to use `PhysicalOptics { .. }` struct and `optics.xxx` field path. All 219 tests pass.
- **Problem**: `CameraEntity::new` takes 10 positional arguments —
  `pose, params, resolution, effective_focal_length_m, f_number,
  focus_distance_m, principal_gap_m, pixel_pitch_m, frustum_near,
  frustum_far` — and carries `#[allow(clippy::too_many_arguments)]`. Seven are
  bare scalars and several share the metre unit, so the compiler cannot catch
  a transposition: swapping `focus_distance_m`/`principal_gap_m`, or
  `frustum_near`/`frustum_far`, or `effective_focal_length_m`/`f_number`,
  produces a wrong-but-compiling camera. `CameraEntity` is a public type and
  this is its only constructor; every call site (and there are many in tests)
  is a 10-positional-argument call. (The bare-physics free functions
  `coc_diameter_at_sensor` (7 args) and `geometric_width_px` are *not* part of
  this finding — they are genuine math signatures and already have clean
  wrappers, e.g. `ThickLens::coc_diameter`.)
- **Fix**: Group the physical-optics scalars into a small `PhysicalOptics`
  (or `LensSpec`-style) struct — `effective_focal_length_m`, `f_number`,
  `focus_distance_m`, `principal_gap_m`, `pixel_pitch_m` — and pass that plus
  `(frustum_near, frustum_far)` as one `(f64, f64)` or a `FrustumRange` newtype.
  Either reduces `new` to ~4 arguments and makes a transposition a type error.
  A builder is the alternative but is heavier than this workspace needs.

### F5 `optics::psf` is mostly unused public API; the UI duplicates its core conversion
- **Severity**: P2
- **Category**: design / code-quality
- **Location**: `crates/etendue-core/src/optics/psf.rs`; `crates/etendue-core/src/optics/mod.rs:48`; `crates/etendue-ui/src/panels/image_view.rs:238`
- **Status**: verified
- **Resolution**: Deleted `gaussian_sigma_to_coc`, `log_sum_exp`, and their 7 unit tests from `psf.rs`. Dropped both from the `optics/mod.rs` re-export. Updated `image_view.rs::halo_half_width` to call `coc_to_gaussian_sigma(p.defocus_px)` instead of re-deriving inline. Updated a test in image_view.rs that referenced `GAUSSIAN_FWHM_PER_SIGMA` to use `coc_to_gaussian_sigma`. Trimmed the `psf.rs` and `optics/mod.rs` doc-comments.
- **Problem**: `psf.rs` exposes four public items. Only `GAUSSIAN_FWHM_PER_SIGMA`
  has a consumer (`image_view.rs`). `coc_to_gaussian_sigma`,
  `gaussian_sigma_to_coc`, and `log_sum_exp` have no caller outside psf.rs's
  own tests. `image_view.rs:238` even re-derives the sigma conversion inline
  (`let sigma = p.defocus_px / GAUSSIAN_FWHM_PER_SIGMA;`) — that *is*
  `coc_to_gaussian_sigma`, bypassed. `log_sum_exp`'s own doc-comment
  (`psf.rs:88-98`) states it exists for a "potential M9 follow-up" gradient
  solver that does not exist; CLAUDE.md constraint #4 explicitly forbids
  speculative scaffolding ("each milestone creates only what it needs").
- **Fix** *(triage: delete unused, wire the rest)*: Have
  `image_view.rs::halo_half_width` call `etendue_core::optics::coc_to_gaussian_sigma`
  instead of re-deriving `defocus_px / GAUSSIAN_FWHM_PER_SIGMA` inline. Delete
  `gaussian_sigma_to_coc` and `log_sum_exp` together with their unit tests, and
  drop them from the `optics::mod.rs:48` re-export. Keep `coc_to_gaussian_sigma`
  and `GAUSSIAN_FWHM_PER_SIGMA` (both now have a real consumer). Trim the
  `psf.rs` module doc-comment so it no longer advertises the removed soft-max
  helper or the hypothetical gradient solver.

### F6 `geom::ray` is entirely unused
- **Severity**: P2
- **Category**: code-quality
- **Location**: `crates/etendue-core/src/geom/ray.rs`; `crates/etendue-core/src/geom/mod.rs:17,20`
- **Status**: verified
- **Reviewer note**: The `ray.rs` deletion, the `geom/mod.rs` re-export removal,
  and the in-code doc-comment fixes are correct and complete (verified: no
  dangling `Ray3` / `RayHit` / `geom::ray` reference anywhere in `crates/`).
  However, the book chapter `book/src/scene_and_geometry.md:138-160` still
  documents the now-deleted `geom::ray` API — that stale-book defect is tracked
  under F8 (book content was F8's scope), not re-opened here.
- **Resolution**: Deleted `crates/etendue-core/src/geom/ray.rs` (254 lines, 10 tests). Removed `pub mod ray;` and `pub use ray::{Ray3, RayHit};` from `geom/mod.rs`. Updated `geom/mod.rs` module doc-comment to remove mentions of the `ray` submodule and "later milestones". Removed dangling cross-reference in `mesh.rs`. Updated `lib.rs` doc-comment.
- **Problem**: `Ray3`, `RayHit`, `Ray3::intersect_triangle`, and
  `Ray3::intersect_mesh` are public, re-exported, and fully unit-tested — but
  have no caller anywhere in the workspace. The doc-comments
  (`ray.rs:7-8`, `geom/mod.rs:11`) say the primitive is "for CPU
  click-picking" by "later milestones"; no such milestone exists, and the mesh
  laser intersection (`laser/intersect.rs`) does its own plane–triangle math
  rather than using `Ray3`. This is speculative scaffolding (CLAUDE.md #4).
- **Fix** *(triage: delete)*: Delete `crates/etendue-core/src/geom/ray.rs`,
  remove `pub mod ray;` and the `Ray3` / `RayHit` re-export from
  `geom/mod.rs`, and update the `geom/mod.rs` module doc-comment so it no
  longer lists a `ray` submodule or "later milestones". `geom::mesh` /
  `TriMesh` stays — it is used. The ray primitive is recoverable from git if
  CPU picking is implemented later.

### F7 Mesh laser intersection (`stripe_segments_on_mesh`) has no consumer
- **Severity**: P2
- **Category**: design
- **Location**: `crates/etendue-core/src/laser/intersect.rs:271-339`
- **Status**: verified
- **Reviewer note**: Verified end to end. `Scene.mesh_targets` carries
  `#[serde(default)]` with a doc note (pre-existing scene JSON deserializes);
  `TriMesh` gained `Serialize`/`Deserialize` with the bypassed-validation doc
  note; `MeshTarget`/`PhysicalOptics` are re-exported from `scene/mod.rs` and
  `lib.rs`; `mesh_laser_stripe_segments` (`viewport/scene.rs`) calls
  `stripe_segments_on_mesh` and is wired into `build_scene`; the panel exposes
  an "Add cube mesh target" button (`MeshTarget::new` + `TriMesh::unit_cube`)
  plus a per-mesh-target collapsible. The additive design was followed —
  `TargetEntity` is still a struct, not an enum.
- **Resolution**: Added `pub struct MeshTarget { pub pose: Isometry3<f64>, pub mesh: TriMesh }` with `MeshTarget::new` validation to `scene/entity.rs`; re-exported from `scene/mod.rs` and `lib.rs`. Added `Serialize, Deserialize` to `TriMesh` (with a doc note about bypassed validation). Added `#[serde(default)] pub mesh_targets: Vec<MeshTarget>` to `Scene`; filled `default_mvp` with `mesh_targets: Vec::new()`. Added `mesh_laser_stripe_segments` to `viewport/scene.rs`. Wired mesh-target opaque `GpuMesh` drawables and mesh-laser-stripe `GpuLines` into `build_scene` in `renderer.rs`. Added mesh-target section to `params.rs`: count label, "Add cube mesh target" button, per-mesh-target collapsible with vertex/triangle count + pose editor. `mesh_target_open` vec added to `PanelState` and synced. All 202 tests pass.
- **Problem**: `stripe_segments_on_mesh` (321 lines of new code in `40371a6`)
  implements and tests fan-plane × triangle-mesh intersection — post-MVP queue
  item 7. But nothing consumes it: `TargetEntity` is a plane-only rectangle,
  there is no mesh-target entity, and the UI's stripe rendering
  (`viewport/scene.rs::laser_stripe_segments`) calls only the planar
  `stripe_on_target`. The capability exists as reachable library API but the
  scene model and UI cannot exercise it — a half-wired feature.
- **Fix** *(triage: build the feature)*: Add a mesh-target entity, **additively**
  — do not turn the existing `TargetEntity` into an enum (that breaks every
  `target.pose` / `.width` / `.height` access and the `defocus_map` rectangle
  sampler). Prescribed design:
  1. `crates/etendue-core/src/scene/entity.rs`: add
     `pub struct MeshTarget { pub pose: Isometry3<f64>, pub mesh: TriMesh }`
     with a validating `MeshTarget::new`, and re-export it from `scene/mod.rs`
     and `lib.rs` alongside `TargetEntity`.
  2. `crates/etendue-core/src/geom/mesh.rs`: add `Serialize, Deserialize` to
     `TriMesh`'s derive so `MeshTarget` is serializable. The derived
     `Deserialize` bypasses `TriMesh::new`'s index/normal-count validation —
     acceptable for v0.1.0 (scene JSON is app-produced); add a one-line doc
     note on `TriMesh` saying so.
  3. `crates/etendue-core/src/scene/scene.rs`: add
     `#[serde(default)] pub mesh_targets: Vec<MeshTarget>` to `Scene`. The
     `#[serde(default)]` is mandatory — without it, every pre-existing scene
     JSON file fails to load.
  4. `crates/etendue-ui/src/viewport/scene.rs`: render each `MeshTarget`'s
     `TriMesh` as an opaque `GpuMesh`, and add a `mesh_laser_stripe_segments`
     helper that calls `stripe_segments_on_mesh` for every laser × every mesh
     target and returns world-space line segments (mirroring
     `laser_stripe_segments`). Wire both into `build_scene`.
  5. `crates/etendue-ui/src/panels/params.rs`: a minimal mesh-target section —
     a count, a pose editor per mesh target, and an "Add cube mesh target"
     button (pushes a `MeshTarget` built from `TriMesh::unit_cube`) so the
     feature is user-reachable. No mesh editing UI.
  This is the largest single finding. If `TriMesh`/`Scene` serde forces a
  change materially beyond this bullet, set F7 `Status: needs-clarification`
  with a note rather than improvising a deeper refactor.

### F8 mdBook and module docs lag the M7–M10 feature set
- **Severity**: P2
- **Category**: docs
- **Location**: `book/src/SUMMARY.md`; various module doc-comments; `.claude/CLAUDE.md`
- **Status**: verified
- **Resolution**: Added four new mdBook chapters: `camera_anatomy.md`, `scheimpflug_solver.md`, `symmetric_rigs.md`, `gaussian_psf.md`. Updated `SUMMARY.md` with all four (under Application and Kernel sections). Updated `roadmap.md` to reflect shipped items and genuinely open items. Fixed stale doc-comments: `analysis/mod.rs:23` ("future viewport overlay" → shipped reference), `scene/entity.rs:444` ("geom/primitives.rs" → MeshTarget cross-reference), `viewport/renderer.rs:28,175,477,1065` (stale M2/M3 milestone references removed), `viewport/camera.rs:55` ("M1 demo scene" → "default MVP scene"), `optics/psf.rs:58` (dangling `[gaussian_sigma_to_coc]` link), `viewport/scene.rs` (broken `[MeshTarget]` link). Fixed `ui.md` stale picking section and M2-scene translucency comment. Updated CLAUDE.md test count to 202. `cargo doc --no-deps --workspace` with `RUSTDOCFLAGS="-D warnings"` exits 0.
- **Reviewer verdict (NEEDS-REWORK)**: The four new chapters are good and the
  enumerated doc-comment fixes (`analysis/mod.rs`, `scene/entity.rs`,
  `viewport/renderer.rs`, `viewport/camera.rs`, `optics/psf.rs`,
  `viewport/scene.rs` `[MeshTarget]` link, `ui.md` picking section, `roadmap.md`)
  are all correctly applied. But the resolution leaves four stale-documentation
  defects, all within F8's own "fix stale book content" scope:
  1. **`book/src/scheimpflug_solver.md` contradicts the code.** Its
     "## The solver: `argmin` L-BFGS-B" section claims the solver is
     **L-BFGS-B** with **finite-difference gradients**, and the `SolverResult`
     code block lists `pub iterations: u64`. The actual solver
     (`solver/scheimpflug.rs:48,198` — `argmin::solver::neldermead::NelderMead`)
     is **Nelder-Mead**, a derivative-free simplex method with no gradients, and
     `SolverResult::iterations` is `u32` (`scheimpflug.rs:126`). Rewrite the
     solver section to describe Nelder-Mead (initial simplex, `sd_tolerance`,
     500-iteration cap) and correct the `iterations` field type.
  2. **`book/src/scene_and_geometry.md:138-160`** ("### `Ray3` and
     Möller–Trumbore") still documents the `geom::ray` API that F6 deleted —
     a full `Ray3` / `RayHit` code block plus prose. Remove the section.
  3. **`book/src/getting_started.md:46` and `book/src/introduction.md:39`**
     still say "172 tests"; `introduction.md` also still says "The MVP is
     complete. Six milestones …". The real count is 205 and the project is
     M0–M10 — update both, consistent with the CHANGELOG/README.
  4. **`crates/etendue-ui/src/viewport/scene.rs:19-20`** module doc still
     references `Drawable::set_transform` (deleted by F11) as a "cheap re-pose
     path" for "M3". Remove or reword the reference.
  Note: the resolution text says "Updated CLAUDE.md test count to 202" — the
  on-disk value is correctly **205** (the count moved as F5/F6/F7/F13 landed);
  the resolution text is just stale, the file itself is right.
- **Rework (Architect, 2026-05-21 — verified)**: All four needs-rework items
  fixed, plus two the first pass did not enumerate. (1) `scheimpflug_solver.md`
  "Cost function" / "The solver" / "SampleGrid" sections rewritten —
  Nelder-Mead (derivative-free, no L-BFGS, no finite-difference gradients), the
  cost samples the laser fan plane in `(phi, r)` filtered by visibility + the
  depth window, `iterations` is `u32`. (2) `scene_and_geometry.md` — deleted
  `geom::ray` section removed; `Scene` snippet gained `mesh_targets`;
  `CameraEntity` snippet updated to the post-F4 `optics: PhysicalOptics` shape;
  a `MeshTarget` subsection added. (3) `getting_started.md` / `introduction.md`
  test counts → 205, status → M0–M10, the false "mesh intersection out of
  scope" bullet removed. (4) `viewport/scene.rs` module doc no longer
  references the deleted `Drawable::set_transform`. (5) Also `bank.md`
  (`CameraEntity::effective_focal_length_m` →
  `PhysicalOptics::effective_focal_length_m`) and `solver/scheimpflug.rs`
  (module doc no longer names the F5-deleted `log_sum_exp`). All five cargo
  gates and `mdbook build` re-run clean.
- **Problem**: The mdBook (published by `publish-docs.yml`) has chapters for
  M0–M6 only — no chapter covers the M8 camera anatomy, the M9 Scheimpflug
  solver, the M10 symmetric rigs / N-view voxel overlap, or the Gaussian PSF.
  Several module doc-comments are stale:
  - `scene/entity.rs:391` references `geom/primitives.rs`, which does not exist;
  - `analysis/mod.rs:23` calls the voxel overlay "the **future** viewport
    overlay" — it exists now (`viewport/scene.rs::voxel_overlap_mesh`);
  - `geom/mod.rs:14` and `geom/ray.rs:8` describe modules "later milestones"
    will add/use;
  - `viewport/renderer.rs:29` says the translucent sort is "a no-op" because
    "the M2 scene has a single translucent drawable" — there are now several
    (camera anatomy, working-volume patch, voxel cloud);
  - `viewport/renderer.rs:178` and `viewport/camera.rs:55` reference "M3 will"
    and "the M1 demo scene".
  - `.claude/CLAUDE.md` states "172 tests" in two places (actual: 218).
- **Fix**: Add book chapters (or sections) for camera anatomy, the Scheimpflug
  solver, symmetric rigs + voxel overlap, and the Gaussian PSF; update
  `SUMMARY.md`. Correct the stale doc-comment references listed above. Update
  the CLAUDE.md test counts. The book chapters are the larger task; the
  doc-comment fixes are mechanical.

### F9 `SolverError` is not `#[non_exhaustive]`
- **Severity**: P3
- **Category**: design / API
- **Location**: `crates/etendue-core/src/solver/scheimpflug.rs:130-147`
- **Status**: verified
- **Resolution**: Added `#[non_exhaustive]` to `SolverError`. Added a doc-comment explaining why the solver carries its own error type rather than folding into `crate::Error`.
- **Problem**: The crate's primary `Error` enum is `#[non_exhaustive]`
  (`error.rs:9`), but `SolverError` — also a public enum, returned by the
  public `solve_scheimpflug` — is not. Adding a variant later is then a
  breaking change. The crate also now has two public error types, where
  `error.rs:1-5` advertises a single "consistent error vocabulary"; a
  domain-specific solver error is defensible, but the inconsistency is worth a
  conscious decision.
- **Fix**: Add `#[non_exhaustive]` to `SolverError`. Optionally note in
  `solver/mod.rs` why the solver carries its own error type rather than
  folding into `crate::Error`.

### F10 The Scheimpflug solver panel always edits camera 0, ignoring the displayed pair
- **Severity**: P3
- **Category**: code-quality / UX
- **Location**: `crates/etendue-ui/src/panels/params.rs:356-361`
- **Status**: verified
- **Resolution**: Changed `scene.cameras.first_mut()` → `scene.cameras.get_mut(state.displayed_pair)` and `scene.lasers.first()` → `scene.lasers.get(state.displayed_pair)`. The solver now acts on the same pair the heatmap and simulated-image panel display.
- **Problem**: `scene_panel` runs the solver section against
  `scene.cameras.first_mut()` / `scene.lasers.first()`. Every other
  M10-aware readout — the defocus heatmap, the working-volume patch, the
  simulated-image panel — follows `state.displayed_pair`. In a multi-pair rig
  the user can select and view pair 3 while "Apply" in the solver section
  silently rewrites pair 0, which is not the camera shown in the heatmap.
- **Fix**: Pass `scene.cameras.get_mut(state.displayed_pair)` and
  `scene.lasers.get(state.displayed_pair)` into `scheimpflug_solver_section`
  so the solver acts on the same pair the rest of the panel displays.

### F11 `Drawable::set_transform` is dead code whose justification has expired
- **Severity**: P3
- **Category**: code-quality
- **Location**: `crates/etendue-ui/src/viewport/renderer.rs:234-245`
- **Status**: verified
- **Resolution**: Deleted `Drawable::set_transform` and its `#[allow(dead_code)]`. Removed `model_buffer` from the `Drawable` struct (its only purpose was the `set_transform` re-upload); `Drawable::new` now uses a local variable. Dropped `COPY_DST` from the buffer usage flags since the buffer is no longer written after creation. The workspace's last `#[allow(dead_code)]` is now gone.
- **Problem**: `set_transform` carries `#[allow(dead_code)]` and a comment
  saying it is the re-pose hook "for post-MVP queue items 2 and 5" (voxel
  working volume, `argmin` optimizer). Both of those have since shipped — the
  N-view voxel overlap and the Scheimpflug solver — and both went through the
  full-rebuild `rebuild_scene` path without using `set_transform`. The method
  is still unused and the milestone it was kept for has passed.
- **Fix**: Delete `Drawable::set_transform` and its `#[allow(dead_code)]`. It
  is recoverable from git if an incremental re-pose path is built later. (This
  removes the workspace's last `#[allow(dead_code)]`.)

### F12 `bank::Catalog` I/O errors are mis-typed and leak a dependency error
- **Severity**: P3
- **Category**: code-quality / error handling
- **Location**: `crates/etendue-core/src/bank/catalog.rs:55-76`
- **Status**: verified
- **Resolution**: Added `Error::Io(String)` variant to `crate::Error` (non-exhaustive, so non-breaking). Changed `load_from_path` to route both file-read and JSON-parse failures through `Error::Io`. Changed `load_from_str` to return `crate::Result<Self>` (was `Result<Self, serde_json::Error>`), routing the parse failure through `Error::Io`. All 202 tests pass.
- **Problem**: `Catalog::load_from_path` maps both a failed file read and a
  failed JSON parse to `crate::Error::Numerical(..)`. `Numerical` is
  documented (`error.rs:33`) as "a numerical operation failed unexpectedly" —
  a missing file is not a numerical failure, so the error variant misleads any
  programmatic handler. Separately, `Catalog::load_from_str` returns
  `serde_json::Error` directly in its public signature, leaking a dependency
  type while the rest of the crate's public API uses `crate::Error`.
- **Fix**: `Error` is `#[non_exhaustive]`, so adding an `Error::Io(String)`
  (or `Error::Parse`) variant is non-breaking — route file/parse failures
  there. Consider having `load_from_str` return `crate::Result<Self>` for
  consistency. Low urgency: `bank` has no in-app consumer yet (component-picker
  UI is still open roadmap item 4).

### F13 Cube-face geometry is duplicated across crates
- **Severity**: P3
- **Category**: code-quality
- **Location**: `crates/etendue-core/src/geom/mesh.rs:139-200` (`TriMesh::unit_cube`); `crates/etendue-ui/src/viewport/scene.rs:614-678` (`voxel_overlap_mesh`)
- **Status**: verified
- **Resolution**: Added `TriMesh::extend_with(&mut self, other: &TriMesh)` to `geom/mesh.rs` — appends the other mesh's vertices/normals/indices with rebased indices. Added `TriMesh::vertices_mut` for in-place vertex transformation. Updated `voxel_overlap_mesh` to build an axis-aligned box template once per call (same face table, using actual hx/hy/hz half-extents instead of the unit-1 offsets) and merge one translated copy per voxel via `extend_with`. Added 3 tests for `extend_with` and `vertices_mut`. Total: 205 tests (152 core + 53 ui + 0 doctests).
- **Problem**: The six-face cube layout — outward normal plus four
  CCW-wound corner offsets per face — is written out twice: once in
  `TriMesh::unit_cube` and again in `voxel_overlap_mesh`, which packs one cube
  per voxel into a single mesh. The tables are equivalent; only the centring
  and per-axis half-extents differ.
- **Fix**: Add a `TriMesh` append/merge helper (e.g.
  `TriMesh::extend_with(&mut self, other)` or a free `merge` function) so
  `voxel_overlap_mesh` can build each voxel cube via `TriMesh::unit_cube` and
  merge. Marginal; bundle it with F6 if `geom` is touched anyway.

## Out-of-Scope Pointers

- **Algorithm / numerical correctness** — the M9 Scheimpflug min-max cost, the
  M10 voxel see/illuminate/focus predicates, and the plane–triangle mesh
  intersection were read for structure, not verified for numerical
  correctness. They look sound and are well-tested, but a dedicated pass
  belongs to `calibration-review` (vision/geometry) or `algo-review` (general).
- **Performance** — `voxelized_overlap` is `O(nx·ny·nz · n_pairs)` with a
  projection + CoC evaluation per voxel per pair, and `working_volume` runs a
  128×128 grid; both recompute per scene-change frame. Cheap at current sizes
  but the natural starting point for `perf-architect` if grid sizes grow.
- **API surface** — F4/F5/F6/F9/F12 together are an API-hygiene cluster; if
  `etendue-core` is ever prepared for a real publish, route the whole public
  surface through `rust-api-revision`. (Note: neither crate sets
  `publish = false`, yet `etendue-core` cannot be published while it depends
  on `vision-calibration-core` by path — a latent inconsistency, not flagged
  as its own finding.)

## Strong Points

- **Zero `unsafe`** across both crates, M0–M10 included. No FFI, no transmutes.
- **All four quality gates green** from a clean state: `fmt --check`,
  `clippy --workspace --all-targets -D warnings`, 218 passing tests, and
  `cargo doc` with `RUSTDOCFLAGS="-D warnings"`.
- **The nalgebra hard-pin still holds**: `cargo tree -i nalgebra` shows exactly
  one `nalgebra 0.34.2` shared by `etendue-core`, `etendue-ui`, and the
  path-dep `vision-calibration-core`. `argmin 0.11` / `argmin-math 0.5` came in
  with `default-features = false` and did not introduce a second nalgebra.
- **The new code is genuinely well-tested**, not just present: the M9 solver
  has convergence tests against an analytic fronto-parallel optimum and a
  worst-case-CoC-improvement test; the M10 ring builder has N-fold-symmetry and
  distance-invariance tests; the mesh intersection has a contract test that its
  per-triangle stripes sum to the analytic planar stripe length.
- **M9 and M10 are wired end to end** — the Scheimpflug solver, the symmetric-
  rig builder, the displayed-pair selector, and the voxel-overlap toggle are
  all reachable and functional in the parameter panel, not just core APIs.
- **The physical-optics-as-source-of-truth invariant** is still enforced:
  `CameraEntity::sync_intrinsics_from_physical` is called at construction and
  after every physical-parameter edit in the panel.
- **The `f64`→`f32` narrowing boundary** remains a single, documented spot
  (`viewport/mesh.rs`); the kernel never sees `f32`.
- **Doc-comment density and quality remain high** — every module opens with a
  `//!` intent block and every public item has a `///` with parameters,
  errors, and panics. The staleness in F8 is drift from fast feature work, not
  a collapse in standard.
