# ADR 0010: A Layered Component Bank

- Status: Proposed (2026-11-01)
- Date: 2026-11-01
- Amends: [ADR 0009](0009-project-folder.md) §1 and its rejected alternative "a per-user app-data
  library"

## Context

ADR 0009 keeps the library inside each project: `library/<kind>s/<id>.json`, plus the project's
`robots/`. A new project copies the starter's library. Nothing is shared between projects:

- A lens measured once has to be copied by hand into every project that uses it.
- Robots come from the app's built-in library or the project's `robots/`. Neither the studio nor
  the CLI imports a new one.
- The studio creates only head and rig items, from the Optics and Rigs workspaces. Sensors,
  lenses, lasers, targets and camera designs are written as JSON by hand. Nothing renames or
  deletes an item.
- Placing a camera, laser or target uses the project's *default* item. There is no picker.

The user wants an **extendable, persistent bank** of optical devices, robots and calibration
targets, which every project draws on. They chose the layered model (2026-11-01).

## Decision

### 1. Three layers, resolved by kind and id

| Layer | Where | Writable |
|---|---|---|
| **Built-in** | Shipped with the app: `examples/library`, `assets/robots`, the procedures (ADR 0011). In a release build, the bundle's resources | Never |
| **User bank** | Desktop: `Documents/etendue/bank` (beside the datasets folder ADR 0007 uses). Browser: a folder picked through File System Access, its handle remembered in IndexedDB as ADR 0009 §5 does for projects | Yes |
| **Project** | The project's `library/` and `robots/` (ADR 0009) | When the project is open for writing |

- **Precedence.** An item is found by kind and id: project first, then the user bank, then
  built-in.
- **Shadowing.** When the same id exists in more than one layer, the studio and
  `etendue library list` mark the shadowed copies.

### 2. The bank is a folder shaped like a project's library

```
<bank>/
  etendue-bank.json                      {"version": 1, "name": "…"}
  library/{sensors,lenses,lasers,targets,cameras,heads,rigs,mounts}/<id>.json
  robots/<id>/{robot.json, robot.urdf, meshes/}
  procedures/<id>.json                   (ADR 0011)
```

- The documents are the existing `LibraryItem` and `RobotManifest`, checked by the same code as
  a project's (`etendue project check`'s item checks).
- A bank has no cells, datasets or runs.

### 3. Projects copy what they use

Placing an item from the bank or the built-ins copies it into the project's `library/` (or
`robots/<id>/`). The same happens when an item becomes a project default.

- The copy records where it came from: `origin: {layer, id, sha256}`. `sha256` is the hash of
  the canonical JSON of the source.
- **Self-contained projects.** A project therefore needs no bank to load, and it can be zipped
  or committed as ADR 0009 intends.
- **Updates are never silent.** When a bank item later differs from a project's copy (by hash),
  the studio says so and offers to update the copy.

### 4. Items carry their provenance

`LibraryItem` gains an optional `source`: vendor, part number, datasheet URL and notes. It also
gains the `origin` of §3.

- These are additive, optional fields. The library format moves to version 2, and version 1
  still loads.
- Cameras, lenses and lasers are usually entered from a datasheet; `source` is where that
  datasheet is.

### 5. Two new kinds; robots are imported

- **`mount`** is a rigid adapter: a flange-to-device transform, with an optional mesh. P15 uses
  it to put a rig or a target on a robot's tool flange (eye-in-hand, eye-to-hand).
- **`robot`** items are the existing robot folders. They can now be imported into the user bank
  or a project:
  - `etendue library import --urdf <file> --manifest <robot.json>`;
  - the same from the studio.

  The import validates the manifest against the URDF, as `etendue project check` does.
- **Targets already cover every target.** A library target holds a `TargetGeometry`, which
  covers every board and every `PrintedTarget`. The new part is the studio's editor for them.

### 6. Tools

- **CLI:**
  - `etendue library list [--project P] [--bank B]`, which shows layers and shadowing;
  - `etendue library check <dir>`;
  - `etendue library import …`;
  - `etendue project check --bank B`, which also checks that every item the project names
    resolves without the bank.
- **Studio:** a **Library** workspace.
  - Browse and filter by kind, layer and text.
  - Create, edit, duplicate, rename and delete, with forms per kind generated from
    `schemas/library.schema.json`.
  - Import and export.
  - "Use in project".
  - Every Setup picker (camera, laser, target, robot, mount) lists the three layers.

## Consequences

- **The bank is shared.** A lens, sensor, target or robot entered once is available in every
  project.
- **Projects stay reproducible**, because they hold copies (§3). G14.2 checks that a project
  still loads after the bank is removed.
- **A second writable folder.** The desktop shell gains a writable scope for the bank folder,
  with the same canonicalised path checks as the project's (ADR 0009 §4). ADR 0007 is amended
  when this lands.
- **Version 2 documents.** Library documents gain a version; readers accept 1 and 2.
- **ADR 0009's rejection is revisited.** It rejected a per-user library because it splits a
  design from its cells. Copying into the project (§3) removes that objection: the bank is the
  source, and the project owns its copy.

## Alternatives considered

- **Projects only (ADR 0009 as is).** Rejected by the user: nothing is shared.
- **A bank only, with projects referring to it by id.** Rejected: a project would then depend
  on the machine it is opened on.
- **IndexedDB as the browser bank.** Rejected for the reason ADR 0009 gives: the files would be
  hidden from the user and from git.

## References

- `docs/pivot/PLAN.md` P14
- `crates/etendue-scene/src/library.rs` (`LibraryItem`, `KINDS`), `src/project.rs`
- `crates/etendue-cli/src/project.rs` (`project check`)
- `web/apps/studio/src/io/project.ts` (`loadProject`, `saveLibraryItem`), `src/io/load.ts` (robot
  resolution)
