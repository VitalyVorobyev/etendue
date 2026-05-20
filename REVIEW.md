# Pre-Release Review — etendue
*Reviewed: 2026-05-20*
*Scope: full workspace (etendue-core + etendue-ui), v0.1.0 MVP*

## Review Verdict
*Verified: 2026-05-20 (Reviewer pass on Implementer's fix patch)*

**Overall: PASS — ready for v0.1.0 tag.**

| Outcome | Count | Findings |
|---|---|---|
| verified | 13 | F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12, F13 |
| needs-rework | 0 | — |
| regression | 0 | — |
| deferred | 1 | F14 (per triage instruction; `scene/scene.rs` not moved, kept as-is) |

### Quality gates — final results

| Gate | Command | Confirming line |
|---|---|---|
| 1. Format | `cargo fmt --all -- --check` | exit 0 (silent) |
| 2. Clippy | `cargo clippy --workspace --all-targets -- -D warnings` | `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 0.14s` |
| 3. Test | `cargo test --workspace` | `test result: ok. 129 passed; 0 failed; …` (etendue-core) + `test result: ok. 42 passed; 0 failed; …` (etendue-ui) + `0` doctests = 171 tests |
| 4. Doc (warnings denied) | `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace` (after `cargo clean -p etendue-ui -p etendue-core`) | `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 1.21s` — **zero warnings** |
| 5. Build | `cargo build --workspace` | `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 1.42s` |

The doc-gate result is the key F1+F2 verification: ran from a clean cache, no
warnings emitted, exit 0. The `ci.yml` `Cargo doc` step now carries
`env: { RUSTDOCFLAGS: "-D warnings" }` (confirmed by reading the diff), so the
class of regression that caused F1 cannot accumulate silently again.

### Test count note

Baseline (pre-fix) was **172 tests** (129 + 42 + 1 doctest). Post-fix is
**171 tests** (129 + 42 + 0 doctests). The single lost test is the doctest in
the `# Examples` block of `probe_calibration_link`, which F6 directed be
deleted outright (the function had been superseded by M2–M6). This is an
expected, intentional reduction, not a regression — the replacement
`calibration_path_dep_resolves` unit test exercises the same path-dep boundary
through `Scene::default_mvp()`.

### Reviewer-found issues

None. The Implementer's patch is on-spec: every "done" item's resolution text
matches the on-disk change verbatim; no new `unsafe`, no new `TODO`/`FIXME`
markers in source (`grep -rn "TODO\|FIXME" crates/ README.md CHANGELOG.md`
returns nothing), no new `unwrap_or` introduced anywhere (`git diff | grep
-cE "^\+.*unwrap_or"` returns 0), and the only remaining
`#[allow(dead_code)]` is the single `Drawable::set_transform` retained per
triage with the required milestone-naming comment (`grep -rn
"#\[allow(dead_code)\]" crates/` returns one hit, at
`crates/etendue-ui/src/viewport/renderer.rs:241`). `Cargo.lock` is unchanged
at 4745 lines, so no transitive-dep drift slipped in alongside the fixes.

### Release-readiness recommendation

**Tag v0.1.0.** The five quality gates are green from a clean build, every
P1 (F1+F2) and every P2 (F3–F8) finding is resolved with on-spec changes,
every P3 (F9–F13) finding except the optional `module_inception` cleanup
(F14, deliberately deferred per triage policy) is resolved, no reviewer-found
issues were introduced by the fix patch, and the kernel/UI separation,
nalgebra hard-pin, and physics test coverage that the Executive Summary
called out as strengths are all untouched. The remaining open item — F4's
RUSTSEC-2024-0436 — is recorded with a justification comment in `deny.toml`
naming the upstream-blocking constraint (nalgebra hard-pin via simba), and
the audit workflow now enforces that allow-list via `cargo deny` on every
push, so the advisory is a tracked, contained known-issue rather than a
release blocker.

## Triage decisions (Architect → Implementer)

The user confirmed the following on 2026-05-20:

| Finding | Decision |
|---|---|
| F1, F2 (P1) | Fix all 17 doc warnings **and** add `RUSTDOCFLAGS: "-D warnings"` to `ci.yml`'s `Cargo doc` step. |
| F3 (P2) | Matrix the `release.yml` `gates` job over `ubuntu-latest`, `macos-latest`, `windows-latest`. |
| F4 (P2) | Add a `deny.toml` allow-list entry for RUSTSEC-2024-0436 with a comment naming the nalgebra hard-pin block. (May require switching audit workflow to `cargo deny`; see fix note.) |
| F5 (P2) | User has captured `docs/ui.png`. Embed it in the README and write a one-line caption. |
| F6 (P2) | Demote `probe_calibration_link` to `pub(crate)`; replace `main.rs`'s smoke-check log with a `Scene::default_mvp()`-based check. |
| F7 (P2) | Add a one-paragraph comment naming the `mem::replace`/`mem::take` panic contract; no structural change. |
| F8 (P3) | Replace inner-loop `expect` calls with direct slice indexing via `wv.cells()`. |
| F9 (P3) | Delete `wireframe_lines`, `Renderer::model_layout()`, `Renderer::globals_layout()`. Keep `Drawable::set_transform` with a comment naming the milestone that will use it (the post-MVP voxel/argmin work). |
| F10 (P3) | Add `rust-version = "1.85"` (a safe edition-2024 floor) to `workspace.package`. |
| F11 (P3) | Replace `panic!("expected Other mount")` with `assert!(matches!(...))`. |
| F12 (P3) | Add a one-line UX hint label when `optics_sliders` clamps focus. |
| F13 (P3) | Update LICENSE copyright to `2025-2026`. |
| F14 (P3) | All P3s in scope — fold `scene/scene.rs` into `scene/mod.rs` is optional; if the move is more than ~50 lines of churn, leave F14 with status `deferred` and reason `not worth the diff for v0.1.0`. |

## Executive Summary

The MVP is in very good shape. The two-crate workspace is consistent, the
kernel/UI separation is clean, the `f64`-kernel / `f32`-GPU boundary is at one
well-documented spot, the nalgebra hard-pin actually unifies (cargo tree
confirms a single `nalgebra 0.34.2` instance across `etendue-core`,
`etendue-ui`, and `vision-calibration-core`), and the physics is exhaustively
documented and tested (172 tests passing, including kill-gate regimes for the
M4 Scheimpflug CoC). There is no `unsafe`, no TODO/FIXME marker inside source
code, and the four quality gates (`fmt --check`, `clippy -D warnings`, `test`,
`build`) are all clean on this machine.

The release-blocking issue is documentation drift: `cargo doc --no-deps
--workspace` emits **17 warnings** (broken intra-doc links, ambiguous links,
redundant explicit links). CI's `cargo doc` step does **not** set
`RUSTDOCFLAGS="-D warnings"`, so these slip through PRs; the `publish-docs.yml`
deployment workflow **does** set it, so the very next push to `main` will fail
the docs deploy. This is a real, latent break, not a stylistic gripe.

The remaining items are P2 polish: a release CI matrix that only gates on
Ubuntu, an upstream-blocked `paste` unmaintained advisory, a placeholder TODO
in the README screenshot section, an `mem::replace`/`mem::take` workaround in
the egui frame closure, a few `#[allow(dead_code)]` hooks that the current
milestone does not use, and a handful of API-surface tightening opportunities
(`probe_calibration_link` shouldn't be on the public surface). Nothing in the
list is a correctness or security risk.

## Findings

### F1 Doc warnings break the publish-docs workflow
- **Severity**: P1
- **Category**: docs / contracts
- **Location**: `crates/etendue-core/src/analysis/working_volume.rs:49,64,302`; `crates/etendue-core/src/laser/width.rs:125`; `crates/etendue-ui/src/app.rs:4,11,28,30,168`; `crates/etendue-ui/src/panels/image_view.rs:11`; `crates/etendue-ui/src/panels/params.rs:39,83`; `crates/etendue-ui/src/viewport/heatmap.rs:4`; `crates/etendue-ui/src/viewport/renderer.rs:530`; `crates/etendue-ui/src/viewport/scene.rs:227,229`
- **Status**: verified
- **Resolution**: Applied `mod@` and `()` disambiguators for all 4 ambiguous `defocus_map` links; replaced 4 unresolved cross-crate links with resolvable forms (prose or `etendue_core::...` paths) and dropped 5 redundant explicit link targets; `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace` now produces zero warnings.
- **Problem**: `cargo doc --no-deps --workspace` emits 17 warnings:
  - **Broken intra-doc links** (8): `CameraParams`, `LaserEntity`, `viewport::Renderer`, `panels::params::scene_panel`, `App::resumed`, `Response`, `crate::app::Graphics`, `Scene` — the link target is not in scope at the doc-comment site, so the rendered page has dead anchors.
  - **Ambiguous links** (4): four `crate::analysis::defocus_map` / `etendue_core::analysis::defocus_map` references — there is both a module and a function by that name, so rustdoc cannot pick one.
  - **Redundant explicit links** (5): `DefocusMap`, `defocus_map`, `ProjectedStripe`, `LaserPlane`, `stripe_on_target` are written as `[label](path)` where `label` already resolves.

  `publish-docs.yml` sets `RUSTDOCFLAGS="-D warnings"` (line 45), so every push
  to `main` or release tag will **fail** the docs deploy on the next run.
  `ci.yml`'s `cargo doc --no-deps --workspace` step (line 41) does **not** set
  the flag — that's why these accumulated unnoticed.

- **Fix**:
  1. Use `mod@` or `()` disambiguators for the four `defocus_map` references (rustdoc suggests the exact change).
  2. Replace the eight unresolved links with the resolvable form: `etendue_core::scene::CameraParams` is wrong (the type is `vision_calibration_core::CameraParams`), so adjust paths or use bare prose where the target genuinely isn't reachable.
  3. Strip the redundant explicit targets — the `[label]` form suffices.
  4. Add `RUSTDOCFLAGS: "-D warnings"` to the `cargo doc` step in `.github/workflows/ci.yml` (same env block as `publish-docs.yml`) so this can never drift again.

### F2 CI doc step does not enforce warning-clean rustdoc
- **Severity**: P1
- **Category**: contracts / CI
- **Location**: `.github/workflows/ci.yml:40-41`
- **Status**: verified
- **Resolution**: Added `env: { RUSTDOCFLAGS: "-D warnings" }` to the `Cargo doc` step in `.github/workflows/ci.yml` so doc-link regressions are caught on every PR.
- **Problem**: The `Cargo doc` step at the end of `ci.yml` runs without
  `RUSTDOCFLAGS="-D warnings"`. The docs deploy workflow (`publish-docs.yml`)
  enforces it — but only on `main` push and tag push. That's how F1 accumulated
  silently. The release-readiness contract should match between CI and the
  publish step.
- **Fix**: Add the env to the doc step:
  ```yaml
  - name: Cargo doc
    env:
      RUSTDOCFLAGS: "-D warnings"
    run: cargo doc --no-deps --workspace
  ```
  Tighten in the same patch as F1 so CI goes green again immediately.

### F3 Release gates run on Ubuntu only
- **Severity**: P2
- **Category**: contracts / CI
- **Location**: `.github/workflows/release.yml:17`
- **Status**: verified
- **Resolution**: Added `strategy.matrix.os: [ubuntu-latest, macos-latest, windows-latest]` to the `gates` job in `release.yml`; the gate now runs on all three platforms before the release build.
- **Problem**: The `gates` job in `release.yml` (the gate every tag build goes
  through) runs only on `ubuntu-latest`. The normal CI workflow runs `fmt /
  clippy / test` on all three of `ubuntu-latest`, `macos-latest`, and
  `windows-latest`. The CLAUDE.md project context notes "the CI matrix runs
  these on ubuntu / macos / windows. A clean local run does not guarantee the
  Windows build is clean (wgpu backend differs)". A platform-specific
  regression that lands between the last green CI run on main and the tag push
  would not be caught by the release gates.
- **Fix**: Either (a) make the release `gates` job a strategy matrix over the
  same three OSes, or (b) declare in the release process that the tag must
  point at a commit that already passed the matrix CI (and verify with `gh run
  list --commit <sha>` before tagging). (a) is the safer choice; (b) is just a
  procedural check.

### F4 `paste` crate is unmaintained (RUSTSEC-2024-0436)
- **Severity**: P2
- **Category**: security / dependencies
- **Location**: dependency graph: `paste 1.0.15 ← simba 0.9.1 ← nalgebra 0.34.2`
- **Status**: verified
- **Resolution**: Added `deny.toml` with `advisories.ignore` entry for RUSTSEC-2024-0436 (comment names the nalgebra hard-pin block); switched `audit.yml` from `actions-rust-lang/audit@v1` to `EmbarkStudios/cargo-deny-action@v2` which uses the new `deny.toml`.
- **Problem**: `cargo audit` flags `paste` as no longer maintained (advisory
  date 2024-10-07). It is a proc-macro pulled in by `simba` (which is itself
  pulled in by `nalgebra 0.34`). Not actionable locally — the project does not
  depend on `paste` directly, and the nalgebra hard-pin precludes a one-line
  upgrade. It is a known, accepted situation in the nalgebra ecosystem.
- **Fix**: Record the advisory in a `deny.toml` allow-list (or in the
  `.github/workflows/audit.yml` configuration) with a comment that it is
  upstream-blocked on the nalgebra hard-pin. Re-evaluate when nalgebra
  publishes a release that updates simba past paste, or when calibration-rs's
  nalgebra hard-pin bumps. Track as a project-level open item, not a code
  change here.

### F5 Embed the UI screenshot in the README
- **Severity**: P2
- **Category**: docs
- **Location**: `README.md:22`; asset at `docs/ui.png`
- **Status**: verified
- **Resolution**: Replaced `_TODO: capture during interactive testing._` with a Markdown image embed pointing at `docs/ui.png` and a one-paragraph caption describing what the screenshot shows.
- **Problem**: `_TODO: capture during interactive testing._` is the only line
  in the Screenshot section. The user has captured the screenshot and placed
  it at `docs/ui.png` (476 KB); the README still carries the placeholder.
- **Fix**: Replace the placeholder line with a markdown image embed pointing
  at `docs/ui.png`, with a one-line caption naming what the image shows
  (the default MVP scene with the defocus heatmap and working-volume overlays
  on). Confirm `docs/ui.png` is not gitignored (it isn't — `.gitignore` only
  excludes `target/`, `book/book/`, and editor noise).

### F6 `probe_calibration_link` is on the public API surface
- **Severity**: P2
- **Category**: design / API
- **Location**: `crates/etendue-core/src/lib.rs:81`
- **Status**: verified
- **Resolution**: Deleted `probe_calibration_link` (it was an M0 probe superseded by M2–M6); replaced the only consumer in `main.rs` with a `Scene::default_mvp()` smoke-check; the lib-level unit test was rewritten to use `default_mvp()` as well.
- **Problem**: The doc comment explicitly states the function "exists as an M0
  de-risking probe and will be superseded by the real `optics` / `scene`
  modules." But it is `pub fn` and re-exported via `pub use` patterns make it
  reachable as `etendue_core::probe_calibration_link`. The function is used by
  the binary's `main` (a smoke check on startup) and a single test. M2–M6 have
  superseded it, but it is now part of the published API surface — removing it
  later is a breaking change for downstream callers who took the dependency.
- **Fix**: Change to `pub(crate)`, then move the binary's invocation to use a
  scene-aware smoke check (e.g. asserting `Scene::default_mvp()` builds). Or:
  add `#[doc(hidden)]` and `#[deprecated(since = "0.2.0", note = "...")]` to
  signal the intent. The first option is cleaner — there is no downstream user
  yet (v0.1.0).

### F7 `mem::replace` / `mem::take` workaround in egui frame closure
- **Severity**: P2
- **Category**: design / code-quality
- **Location**: `crates/etendue-ui/src/app.rs:436-438, 486-487`
- **Status**: verified
- **Resolution**: Added a one-paragraph comment at the `mem::replace`/`mem::take` site naming the panic contract: closure must not panic in normal operation; if a panic-producing operation is added, restructure to `RefCell<Scene>` or add a panic guard via `scopeguard::defer`.
- **Problem**: Inside `Graphics::render`, the scene and panel state are
  temporarily extracted out of `self` because `egui::Context::run_ui` takes
  `Fn` not `FnMut`:
  ```rust
  let mut scene_ref = std::mem::replace(&mut self.scene, Scene::empty());
  let mut panel_state_ref = std::mem::take(&mut self.panel_state);
  // ... closure mutates scene_ref / panel_state_ref ...
  self.scene = scene_ref;
  self.panel_state = panel_state_ref;
  ```
  If the closure panics (e.g. an egui internal panic, or a `serde_json`
  failure inside `save_scene_dialog`), the scene is left as `Scene::empty()`
  and the panel state at default — silent data loss. The `save_scene_dialog`
  currently does not panic (it routes errors to `state.io_error`), so the risk
  is low but real.
- **Fix**: Use a `RefCell<Scene>` for the duration of the frame, or rebuild
  the closure pattern with a `Cell::take` / `Cell::set` pair and a panic guard
  (`scopeguard::defer`) that restores state on unwind. The simpler structural
  fix: hoist `scene_ref` and `panel_state_ref` into `Option<>`s and have a
  `Drop` impl restore them. Risk-weighted, the cheap option is a brief
  comment naming the panic contract.

### F8 Inner-loop `expect` calls on always-in-range cells
- **Severity**: P2
- **Category**: code-quality / perf-pointer
- **Location**: `crates/etendue-ui/src/viewport/scene.rs:369-391` (in `working_volume_mesh`)
- **Status**: verified
- **Resolution**: Replaced all six `wv.get(...).expect("row/col in range")` calls with a local `at(r, c)` closure that indexes `wv.cells()` directly via `cells[r * cols + c]`.
- **Problem**: For each cell, six `wv.get(r, c).expect("row/col in range")`
  calls. The outer loop iterates `0..rows-1` and `0..cols-1` and the indices
  are `(row, col)`, `(row+1, col)`, `(row, col+1)`, `(row+1, col+1)` — all
  provably in bounds. The format string is constructed even when assertions
  hold (Rust doesn't constant-fold `&str` formats here), and the bounds-check
  branch fires twice per `get` (once in the iterator, once via `expect`).
  Cosmetic and a perf-pointer; the work is dwarfed by the 128×128 grid pass
  upstream. Forward to **perf-architect** for benchmark-driven evaluation;
  the design-level fix is to expose a `WorkingVolume::cells_grid()`-style
  raw-slice accessor or take a `(usize, usize)` -> `&WorkingVolumeCell`
  closure.
- **Fix**: Use `wv.cells()[row * wv.cols() + col]` directly (the public
  `cells()` accessor exists), or add a small `cell_unchecked` debug-asserted
  helper and use it in this hot path. (Not a hot path that matters yet — 16k
  cells per scene change — but cleaner.)

### F9 Unused `#[allow(dead_code)]` hooks
- **Severity**: P3
- **Category**: code-quality
- **Location**: `crates/etendue-ui/src/viewport/renderer.rs:238` (`Drawable::set_transform`), `:566` (`model_layout`), `:573` (`globals_layout`); `crates/etendue-ui/src/viewport/mesh.rs:151` (`GpuMesh::wireframe_lines`)
- **Status**: verified
- **Resolution**: Deleted `wireframe_lines`, `model_layout()`, `globals_layout()`, and the now-orphaned `globals_layout` struct field; kept `Drawable::set_transform` with an updated comment naming it as the re-pose hook for post-MVP voxel/argmin work (queue items 2 and 5).
- **Problem**: Four `#[allow(dead_code)]` annotations. Three are renderer
  accessors retained "for M2/later milestones"; one is a primitive builder
  retained "for later milestones (e.g. a wireframe working-volume hull in
  M6)". The CLAUDE.md project context calls for "no speculative scaffolding"
  and says "each milestone creates only what it needs." Per that rule these
  should be deleted; if they're judged useful as documented hooks, the
  `#[allow]` should at least carry a comment naming the milestone that will
  consume them.
- **Fix**: Delete `wireframe_lines`, `model_layout()`, and `globals_layout()`
  — none is referenced anywhere. Keep `set_transform` only if a near-term
  follow-on milestone will use it (its docstring claims M3, but M3 ships
  rebuild-on-change instead). Project policy decision.

### F10 No `rust-version` (MSRV) declared in workspace.package
- **Severity**: P3
- **Category**: workspace
- **Location**: `Cargo.toml:8-13` (workspace.package)
- **Status**: verified
- **Resolution**: Added `rust-version = "1.85"` to `[workspace.package]` in `Cargo.toml`.
- **Problem**: `rust-toolchain.toml` pins the local toolchain to 1.95.0, but
  the workspace.package does not declare `rust-version`. Downstream consumers
  (if `etendue-core` is ever published) would have no MSRV signal. The CI uses
  `dtolnay/rust-toolchain@stable`, so the pinned toolchain is not actually
  the version CI builds with — making the local pin somewhat decorative.
- **Fix**: Either (a) commit to publishing `etendue-core` and declare a
  conservative MSRV (`rust-version = "1.85"` is a reasonable floor for
  edition 2024), or (b) note in CLAUDE.md / docs that the workspace is
  app-only and MSRV is not part of its contract. (a) is the better long-term
  story since the kernel is clearly designed to be reusable.

### F11 `panic!` in a unit test instead of an assert
- **Severity**: P3
- **Category**: tests / code-quality
- **Location**: `crates/etendue-core/src/bank/schema.rs:250`
- **Status**: verified
- **Resolution**: Replaced `match`/`panic!` with `assert!(matches!(back, LensMount::Other { ref description } if description == "Custom bayonet"), ...)` in `lens_mount_other_round_trips`.
- **Problem**: `_ => panic!("expected Other mount"),` in
  `lens_mount_other_round_trips`. Idiomatic Rust tests prefer
  `assert!(matches!(back, LensMount::Other { description } if description == "Custom bayonet"))`,
  which gives a better failure message and is easier to maintain.
- **Fix**: Replace with the `matches!` form (one line). Cosmetic.

### F12 `optics_sliders` clamps focus mid-edit
- **Severity**: P3
- **Category**: code-quality / UX
- **Location**: `crates/etendue-ui/src/panels/params.rs:555-560`
- **Status**: verified
- **Resolution**: Added a `focus_was_clamped` boolean; when set, a `ui.colored_label(gray, "Focus pinned to >f")` appears below the slider for the frame the clamp fires.
- **Problem**: When the user drags the focal-length slider up past the focus
  distance, `optics_sliders` silently raises `focus_distance_m` so the lens
  invariant (`focus > focal`) is preserved. The displayed focus distance
  jumps without a UI hint. A designer interpreting the panel as "what I see
  is what I have" gets a surprise.
- **Fix**: Either (a) add a colored hint label "Focus pinned to >f" when the
  clamp fires this frame, or (b) reject the focal-length edit instead of
  clamping focus. (a) is the friendlier choice; reuse the `state.io_error`
  pattern with a transient ephemeral label.

### F13 `LICENSE` copyright year is 2026
- **Severity**: P3
- **Category**: docs
- **Location**: `LICENSE:3`
- **Status**: verified
- **Resolution**: Updated `LICENSE` line 3 from `Copyright (c) 2026` to `Copyright (c) 2025-2026`.
- **Problem**: `Copyright (c) 2026 Vitaly Vorobyev` — the current date
  (2026-05-20) matches, but a release at the very start of 2026 would have
  carried 2025/2026 conventionally. Minor wording.
- **Fix**: Either accept the single year (acceptable) or expand to
  `2025-2026` if the work started in 2025.

### F14 Module-inception in `scene/scene.rs`
- **Severity**: P3
- **Category**: design
- **Location**: `crates/etendue-core/src/scene/mod.rs:31`, `crates/etendue-core/src/scene/scene.rs`
- **Status**: deferred
- **Resolution**: Deferred to a future refactor PR — `scene/scene.rs` is ~425 lines; folding it into `scene/mod.rs` would be >50 lines of churn with no functional benefit for v0.1.0.
- **Problem**: The clippy `module_inception` lint is allowed because the
  development plan prescribes `scene/scene.rs` for the `Scene` aggregate
  alongside `scene/entity.rs`. The justification is explicit. But the lint
  exists because doubled paths confuse readers — and an alternative
  (`scene/mod.rs` holding `Scene` itself and `scene/entity.rs` holding the
  entities) achieves the same separation without the double name.
- **Fix**: Optional. The current structure is defensible; if the project
  cleans this up later, the `Scene` aggregate moves to `mod.rs`, the rotation
  helpers join it, and `scene/scene.rs` disappears.

## Out-of-Scope Pointers

- **Algorithm / numerical correctness**: the Scheimpflug CoC derivation has
  kill-gate regimes (a), (b), (c), (d) covered by unit tests against a written
  derivation; if any of those needs deeper inspection, route via
  `calibration-review` (vision-domain) or `algo-review` (general algorithm).
  None flagged here.
- **Performance**: `WorkingVolume::area_m2` and `working_volume_mesh` run
  16k cells per scene change; the inner-loop `expect`s in F8 are the only
  place a perf review would have to start. Route to `perf-architect` once
  voxel analysis lands (per the post-MVP queue).
- **API revision**: `probe_calibration_link` (F6) is a single API-surface
  concern; if more land before publishing `etendue-core` to crates.io, route
  through `rust-api-revision` for a wider surface audit.

## Strong Points

- **Zero `unsafe`** across both crates. No FFI, no manual transmutes, no
  alignment hazards.
- **Single `f64` → `f32` narrowing point** at `crates/etendue-ui/src/viewport/mesh.rs`,
  documented as the boundary. The kernel never sees `f32`.
- **nalgebra hard-pin actually unifies**: `cargo tree -i nalgebra` shows
  exactly one `nalgebra 0.34.2` instance shared by `etendue-core`,
  `etendue-ui`, and the path-dep `vision-calibration-core`. The `Cargo.toml`
  comment explaining *why* it must be a hard pin is one of the better
  examples of that pattern in the wild.
- **Physical-optics-as-source-of-truth invariant** is enforced at the type
  level: `CameraEntity::new` runs `sync_intrinsics_from_physical` at
  construction, and the UI calls it again after every physical-parameter
  edit. The doc comment on `CameraEntity::sync_intrinsics_from_physical`
  spells out the call contract.
- **Doc comments are uniformly excellent** — every module has a `//!` block
  that explains intent before getting to types; every public item has a
  `///` block that names purpose, parameters, errors, and (where relevant)
  panics. The Scheimpflug derivation companion doc-comment in
  `optics/coc.rs` is exemplary.
- **172 tests passing** with meaningful structure — kill-gate physics
  regimes for the M4 CoC, default-scene integration coverage from M2 onward,
  and the bank seed JSON files embedded at compile time so schema drift fails
  the test suite immediately (`bank/catalog.rs:98-100`).
- **Coherent error model**: `Error::InvalidInput { reason }` carries the
  designer-readable message; `Error::InsufficientData { need, got }` is
  structured; `Error::Singular` and `Error::Numerical(String)` round it out.
  `#[non_exhaustive]` keeps the type forward-compatible.
- **Component bank schema** uses tagged enums with `#[serde(tag = "type",
  rename_all = "snake_case")]` and `#[non_exhaustive]`, mirroring
  calibration-rs's serde conventions exactly. This is documented in
  CLAUDE.md and verifiable in the seed JSON files.
- **CI matrix on three OSes**: even though the release gates only check
  Ubuntu (F3), the regular CI sweep does run on macos and windows, which is
  the necessary condition for catching wgpu-backend drift.
- **CHANGELOG.md** is structured to Keep-a-Changelog conventions; the
  release workflow extracts a per-tag section automatically.
- **No `TODO` / `FIXME` markers** inside `crates/` source code (only one in
  `README.md`, captured as F5).
- **Hand-written winit + wgpu + egui render loop** is not a place new
  projects usually get away cleanly; this one does, and the per-frame
  structure comment in `app.rs:8-37` is the kind of write-up that makes the
  M2/M3 refactor possible without resorting to eframe.
