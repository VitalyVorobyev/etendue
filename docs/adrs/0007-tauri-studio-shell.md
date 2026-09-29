# ADR 0007: A Tauri Shell for the Studio, With the Kernel Kept in wasm

- Status: Proposed
- Date: 2026-09-29

## Context

The studio (P2-5, `web/apps/studio`) is a Vite app that runs its kernel as
`@etendue/wasm`: it bakes, projects and remaps every frame in process. PLAN §8
decision D4 kept it a plain web app. Tauri was to be added only when a native
trigger fired, and the first trigger named was "launching renders from the UI".

That trigger has fired. With P4 and the image-level closed loop (P5-1b), a
dataset is the result of three native steps: `etendue gt`, `etendue render`
(which runs Blender) and `etendue detect`. A browser can do none of them: it
cannot start Blender, write gigabytes of EXRs, or read the rendered images.
calibration-rs's `calibration-diagnose`, which shares the 3D packages, is a
Tauri + React app too. The user expects the same shape: a React UI wired to the
Rust algorithms.

## Decision

### Two kernels, split by what the work is

- **In process (`@etendue/wasm`)**: everything the UI does per frame or per
  edit. That is baking a scenario, projecting targets, back-projecting a
  camera's field of view, and the remap LUT of a camera's live image.
  - These are synchronous, small, and needed at 60 Hz or on every keystroke.
  - An IPC round trip per call would make them asynchronous.
  - A remap LUT is 2 × W × H floats (≈ 10 MB at 1280 × 1024) and would cross
    IPC as JSON.
  - It also keeps the studio and the 3D packages working in a plain browser,
    which is what `calibration-diagnose` and lab-ui need.
- **Native (Tauri commands)**: work that needs the operating system or runs
  for minutes. That is dataset generation (ground truth → Blender → detection)
  with progress and cancel, Blender's status, and scenarios from tool-pose
  files. It also covers file access by path, which gives a scene the absolute
  path the native steps need.

The math lives in one place either way. Both kernels are the same Rust crates
compiled twice. No etendue math moves into TypeScript or into the shell.

### The pipeline becomes a library

`etendue-cli` gets a `[lib]` target, `etendue_cli`, holding `load`, `gt`,
`render`, `detect` and `poses`. Long steps report through
`progress::Control`: log lines and step counts. They can also be cancelled:
Blender's output is streamed, and Blender is stopped on cancel. The `etendue`
binary is its command line, and the shell calls the same functions.

The Blender pin is looked up from explicit directories: the scene's
directory, and for the CLI also the working directory.

`cargo xtask check-layering` forbids any workspace crate from depending on
`etendue-cli`, which stays the top of the stack.

### The shell is its own cargo workspace

`web/apps/studio/src-tauri` declares its own `[workspace]`, as calibration-rs
does with `app/src-tauri`. This keeps tauri-build's dependency tree out of
`cargo build --workspace` and out of the root `Cargo.lock`.

Because it is outside the workspace, it restates what the root workspace
gives its members:
- the calibration-rs `[patch.crates-io]`;
- the dev profile.

Its graph is checked for the single-nalgebra pin by
`cargo xtask check-layering --manifest-path …`. The pin had already caught a
second nalgebra: calib-targets allows 0.35, and a fresh lockfile took it.

### Files and assets

- The shell reads files for the webview (`read_text`, `path_exists`). Reading a
  file adds its directory to the asset-protocol scope, because a robot
  manifest's meshes sit below it.
- A dataset's output directory is added when it is generated.
- The scope starts with the checkout's `examples/` and `assets/robots/`.
- In the shell, the studio reads examples from the checkout on disk rather than
  over HTTP, so every scene has a filesystem path.
- Dropped files have no path. They still load, but they cannot generate a
  dataset.

### Scope of this change

The shell adds the Dataset panel, "Open scene…", "Import poses…", and the
dataset image in a camera's view.

`etendue-ui` stays frozen until gate G6.3 (ADR 0001). Its release job is
unchanged. Bundling, signing and releasing the shell are not part of this
change.

## Consequences

- There are two build paths for the studio: `bun run dev` (a browser) and
  `bun run tauri dev` (the shell).
  - The e2e suite runs the browser build.
  - The shell's frontend wiring is tested against a mocked
    `__TAURI_INTERNALS__` (`e2e/tauri.spec.ts`).
  - Its Rust is tested with `cargo test` in the `studio-tauri` CI job
    (ubuntu, WebKitGTK).
- Panels that need the shell check `isTauri()` and are absent in a browser.
- The shell's `Cargo.lock` is separate. A dependency bump is made in both
  lockfiles, and the nalgebra check guards the pin.
- P6-2's escalation path (an analysis too slow in wasm) now has a place to go:
  a Tauri command.

## Alternatives considered

- **Tauri only, retiring `@etendue/wasm`.** Rejected (user decision
  2026-09-29).
  - Every per-frame call would become an asynchronous IPC round trip.
  - Large LUTs would have to cross as JSON.
  - The browser build that the shared 3D packages and `calibration-diagnose`
    rely on would be lost, along with gates G0.1 and G2.1.
- **Keep plain Vite, with renders from the CLI.** Rejected: it is exactly the
  D4 trigger. It also leaves the dataset workflow, the reason for the
  photometric tier, outside the studio.
- **Run the `etendue` binary as a subprocess from the shell** (a sidecar with
  JSON-lines progress). Not chosen:
  - it needs the binary bundled and versioned beside the app;
  - the library call is simpler and shares the tests.

## References

- `docs/pivot/PLAN.md` §8 D4, P2-5, P2-7, P5-1
- [ADR 0001](0001-pivot.md) (package split, `etendue-ui` frozen)
- [ADR 0005](0005-blender-renderer.md) (Blender backend)
- `docs/measurements/closed_loop_images.md` (the pipeline the Dataset panel runs)
- calibration-rs `app/src-tauri` (the pattern this follows)
