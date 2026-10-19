# ADR 0009: The Project Folder

- Status: Proposed
- Date: 2026-10-19

## Context

The studio's artifacts are scattered, and some are not artifacts at all:

- **Scenes and scenarios** are pairs of files wherever the user saved them.
- **Datasets** go to `target/` in a development build and `Documents/etendue` in a
  release build (ADR 0007).
- **Robots** are read from the app's library, `assets/robots`.
- **The component bank** sits in `crates/etendue-core/assets/bank`, where nothing reads it.
- **Defaults are hard-coded.** New entities take their values from code: `newEntity` in
  `scene/edit.ts` (a 1280×1024 camera with fx = 2400, a 6×9 board) and the rig builder's
  presets.
- **Examples** are compiled into the frontend as a virtual module.

The roadmap (PLAN P8–P13) adds three kinds of artifact:

- library items: sensors, lenses, lasers, camera and head designs, and rigs (ADR 0008);
- calibration runs;
- user-made rigs and heads, which must persist across sessions and be shared.

The user asked for one place that holds every serialised artifact, with nothing (or as
little as possible) hard-coded in the app.

## Decision

### 1. A project is a folder with a manifest

```
<project>/
  etendue-project.json
  library/{sensors,lenses,lasers,targets,cameras,rigs}/<id>.json   (heads: P10)
  robots/<id>/{robot.json, robot.urdf, meshes/}     (optional)
  cells/<name>/{scene.json, scenario.json}
  datasets/<name>/…                                 (gt, renders, features)
  runs/<name>/{config.json, result.json, report.json}  (calibration lab)
```

`etendue-project.json` is a versioned document defined in `etendue-scene/src/project.rs`.
Its schema is emitted to `schemas/project.schema.json`.

```json
{
  "version": 1,
  "name": "laser heads",
  "cells":  ["cells/ring3"],
  "robots": [],
  "defaults": { "camera": "cameras/imx174_lm25hc", "laser": "lasers/stingray_660",
                "target": "targets/chess_9x6_25mm" }
}
```

- **Every path is relative to the project folder** and uses `/`.
- **`cells` lists the cell folders explicitly.** A cell may live anywhere inside the
  project. This lets `examples/` become a project without moving any folder that CI or a
  gate names.
- **`defaults` names library items.** New entities in the studio are built from them,
  not from code. The code keeps one minimal fallback per kind, for a project without
  defaults.
- **A library item's id is its file stem.** `etendue project check` checks that they match,
  that each item's kind matches its folder, that the defaults name existing items, and that a
  camera design's params match its optics.

### 2. Robots resolve in a fixed order

A scene's `robots[].manifest` stays a path relative to the scene file. When that path
does not exist, the robot is found by id in this order:

1. the project's `robots/<id>/`;
2. the app's built-in robot library.

The shell's `save_scene` already falls back to the library. This makes the fallback a
rule and adds the project's own robots in front of it.

### 3. The repository's examples are the reference project

- `examples/etendue-project.json` lists the six example folders as cells.
- `examples/library/` is seeded from today's bank: three sensors, three lenses and
  three lasers. To these it adds:
  - the examples' two boards;
  - the examples' pinhole camera;
  - a physical camera (IMX174 behind a 25 mm lens at f/4, focused at 0.5 m), which is the
    project's default camera;
  - the three-camera laser ring.
- `crates/etendue-core/assets/bank` is removed. Core tests keep small inline fixtures, so
  the published crate reads no file outside itself.
- The app bundles this project read-only as the starter. **New project** creates a
  folder with a manifest and a copy of the starter's library. It can also copy cells.

### 4. The shell works inside the open project

- **Opening a project** (`project_open`, by dialog or by recent path) makes its folder
  the readable and **writable** scope, and the asset-protocol scope.
- **Bundled resources stay read-only.** They are the starter project, the robot library
  and `etendue.toml`. A dataset or a calibration run writes under the project's
  `datasets/` or `runs/`.
- **What it replaces.** It replaces ADR 0007's "data root" as the working location, and
  `Documents/etendue` as the default dataset folder. Files picked outside the project
  keep today's per-pick scope.
- **New commands** for library items and cells: `project_open`, `project_new`,
  `project_list`, `project_read`, `project_write`, `project_delete`.
- **Path checks.** Every path is checked to resolve inside the project after
  canonicalisation.
- **The last project is reopened on start.** With none, the starter opens read-only.

### 5. The browser uses the same layout

- **Read and write:** in Chromium, through the File System Access API
  (`showDirectoryPicker`). The handle is remembered in IndexedDB, and permission is
  re-requested on reload.
- **Read only:** elsewhere, a project served over HTTP; the development server serves
  `examples/`. Saving downloads files, as today.
- **The virtual examples module is replaced** by reading the reference project's
  manifest.

## Consequences

- **Everything a user makes is a file in one folder.** That folder can be a git
  repository, a shared drive, or a zip sent to a colleague.
- **Scenes stay self-contained** (ADR 0008 copies by value). A cell copied out of its
  project still loads, with robots from the app's library.
- **The `etendue` CLI is unchanged.** It still takes scene and scenario paths. A
  `--project` option can follow when a command needs the library.
- **The CI paths of the examples are unchanged.**
- **The shell's scope logic moves** from "the data root plus picked folders" to "the
  project plus picked folders". ADR 0007 is amended when this lands.
- **Development builds stop reading the checkout as the data root.** They open
  `examples/` as a project instead.

## Alternatives considered

- **Library in the repository only.** Rejected: a release app cannot write there, and
  user designs would need a source checkout.
- **A per-user app-data library beside scene files.** Rejected (user decision
  2026-10-19, "Project folder"). It splits a design from the cells that use it, and
  sharing would need two exports.
- **IndexedDB as the browser's store.** Not chosen as the primary store. It hides the
  files from the user and from git. It is kept only for remembering the folder handle.
- **Moving the examples into `cells/`.** Rejected: gates, CI and the book name the
  current paths, and `cells` listing makes the move unnecessary.

## References

- `docs/pivot/PLAN.md` P9 (project I/O), P13 (`runs/`)
- [ADR 0007](0007-tauri-studio-shell.md) (shell scopes and writable folders, to be amended)
- [ADR 0008](0008-physical-optics.md) (library documents, copy by value)
- `web/apps/studio/src/scene/edit.ts` (`newEntity`), `src/panels/RigBuilder.tsx` (presets)
