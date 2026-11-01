# Projects and the library

A **project** is a folder that holds everything you make: cells, the component library, and the
datasets and calibration runs made from them. The repository's `examples/` folder is one. At start the
studio opens the project you had open last, or else its built-in copy of the examples. That copy
is read-only: open or create a project of your own to save into.

```text
<project>/
├── etendue-project.json           the manifest
├── library/
│   ├── sensors/<id>.json
│   ├── lenses/<id>.json
│   ├── lasers/<id>.json
│   ├── targets/<id>.json
│   ├── cameras/<id>.json
│   ├── heads/<id>.json
│   ├── rigs/<id>.json
│   └── mounts/<id>.json
├── robots/<id>/robot.json …       robots that come with the project (optional, listed in the manifest)
├── <cell>/scene.json              a cell: its scene …
├── <cell>/scenario.json           … and usually its scenario
├── datasets/<cell>/               datasets the studio generates for a cell
└── runs/<name>/                   calibration runs
```

In the studio, **File › Open project…** opens a project, and **File › New project…** makes one
in an empty folder, with a copy of the examples' library and defaults. **Save as a cell of
_project_…** adds the open scene as a new cell. The Optics and Rigs workspaces read their parts
from the library, and their **Save to library** writes `library/heads/<id>.json` and
`library/rigs/<id>.json`. See [The studio](studio.md).

## `etendue-project.json`

```json
{
  "version": 1,
  "name": "etendue examples",
  "cells": ["closed_loop_ur5e", "eye_in_hand_ur5e"],
  "defaults": {
    "camera": "cameras/imx174_lm25hc",
    "laser": "lasers/stingray_660",
    "target": "targets/chess_9x6_20mm"
  }
}
```

| Field | Meaning |
|---|---|
| `version` | The format version, 1. |
| `name` | What the project is. |
| `cells` | The cell folders. Each holds a `scene.json` and usually a `scenario.json`. |
| `robots` | Robot folders that come with the project, each holding a `robot.json`; `etendue project check` checks them. A scene names each robot by the path to its `robot.json`, relative to the scene (for example `../robots/my_arm/robot.json`). When that file is missing, the studio looks for the robot in its own robot library; the command line does not. |
| `defaults` | The library items new cameras, lasers and targets are made from in the studio, as `<folder>/<id>` references. |

Paths are relative to the project folder and use `/`. The schema is
[`project.schema.json`](https://github.com/VitalyVorobyev/etendue/tree/main/schemas/project.schema.json).

## Datasets and runs

In a project the studio can write to:
- **Generate** in the Calibrate workspace writes the open cell's dataset to `datasets/<cell>/`
  by default;
- every calibration is kept in `runs/<name>/`, named after the dataset, the time and the
  observations (for example `closed_loop_ur5e-20261019-181522-noise-s1x8`).

A dataset is the folder [Datasets](datasets.md) describes; a run is the folder
[Calibration](calibration.md#runs) describes. On the command line,
`etendue calibrate <dataset> --run runs/<name>` keeps a run the same way.

## Library items

Each item is one file, `library/<folder>/<id>.json`, holding exactly one part, under the key that
names its kind:

```json
{
  "version": 1,
  "id": "lm25hc",
  "name": "Kowa LM25HC (25 mm C-mount)",
  "lens": {
    "focal_length_m": 0.025,
    "min_f_number": 1.4,
    "principal_gap_m": 0.0,
    "entrance_pupil_m": 0.0,
    "mount": "c_mount"
  }
}
```

Every item has `version`, `id`, `name` and an optional `description`. Version 2 items may also
carry:
- `source`: where the data comes from (`vendor`, `part_number`, `datasheet_url`, `notes`);
- `origin`: on a project's copy of a bank or built-in item, where it came from (`layer`,
  `reference`) and the source's content hash when it was copied (`sha256`). See
  [The bank](#the-bank).

| Kind | Folder | What it holds |
|---|---|---|
| `sensor` | `sensors` | `pixel_pitch_m`, `resolution`; when known, `well_depth_e`, `read_noise_e` and `peak_qe` |
| `lens` | `lenses` | `focal_length_m`, `min_f_number` (the widest aperture), `mount` (`c_mount`, `cs_mount`, `f_mount`, `m12`, `other`); `principal_gap_m` and `entrance_pupil_m` (0 for a thin lens); `focusing`; when known, `fov_diagonal_deg` and `mtf50_lp_mm` |
| `laser` | `lasers` | `wavelength_nm`, `fan_half_angle`; when known, `beam_waist_m`, `waist_distance_m` (where it is focused; absent, the laser is focusable and a head design focuses it) and `power_mw` |
| `target` | `targets` | `geometry`: a target as a scene holds it ([Targets](scenes.md#targets)) |
| `camera` | `cameras` | a camera design: its model, image size and, when physical, its optics ([Physical cameras](physical_cameras.md)) |
| `head` | `heads` | a laser-triangulation head: a physical camera, a laser, the laser's pose on the camera's body (`camera_se3_laser`), its fan length, and the window it must work in (`near_m`, `far_m`, `width_m`, `max_blur_px`) |
| `rig` | `rigs` | a rig layout, its camera and lasers included ([Rig layouts](scenes.md#rig-layouts)) |
| `mount` | `mounts` | an adapter on a robot's tool flange: the device's pose on the flange (`flange_se3_device`) and, when known, its `mass_kg` |

- **The id is the file name.**
- **`version` is read first.** etendue reads versions 1 and 2 and writes 2. An older etendue
  refuses a newer file because of its version, not because of a field it does not know.
- **Designs embed their parts by value,** so a library file is complete on its own.
- **A scene copies a design by value.** It never depends on a library being present, and a
  dataset made from it never points at a missing file.

The schema is
[`library.schema.json`](https://github.com/VitalyVorobyev/etendue/tree/main/schemas/library.schema.json).
The values in `examples/library` come from datasheets and are illustrative: check them before a
real design.

## Checking a project

```bash
etendue project check examples
```

```text
project ok: etendue examples: 6 cell(s), library 2 cameras, 3 lasers, 3 lenses, 1 rigs, 3 sensors, 2 targets, 0 robot(s)
```

The check covers:
- the manifest;
- every cell: the scene with its robots and optics, and the scenario compiled in full;
- every library item: its version, its id against its file name, its kind against its folder,
  and a camera design's model against its optics;
- the defaults: they name existing items;
- the project's robots.

Every problem is reported, one per line, and the command fails. With `--bank <dir>` it also
notes the items copied from that bank whose source has changed since (a note, not a problem: the
project still has its copy).

## The bank

A **bank** is your own library, shared by every project. It has the same `library/` and
`robots/` folders as a project, and a manifest instead of cells:

```text
<bank>/
├── etendue-bank.json              {"version": 1, "name": "My bank"}
├── library/<folder>/<id>.json
└── robots/<id>/robot.json …
```

The studio reads three layers, in this order: the open project, your bank, and the built-in
library that ships with etendue. An item found in a higher layer hides one with the same
reference below it.

A project keeps **copies** of what it uses. The copy records its source in `origin`, so a project
opens without the bank, and the studio can tell you when the bank's item has changed.

On the command line:

```bash
etendue library init ~/Documents/etendue/bank --name "My bank"
etendue library import my_lens.json --into ~/Documents/etendue/bank
etendue library import path/to/my_robot --into ~/Documents/etendue/bank   # robot.json, URDF, meshes
etendue library check ~/Documents/etendue/bank
etendue library list --project my_project --bank ~/Documents/etendue/bank --builtin examples
```

- **`import`** checks the item (or loads the robot as the kinematics would) before it writes,
  and writes items at version 2. It replaces an existing item only with `--force`.
- **`list`** prints each item once, from the layer it resolves to, and names the layers it hides.

## In the studio

The **Library** workspace lists every item the open project sees: its own, your bank's and the
built-ins, grouped by kind. Filter it by text (name, id, vendor, part number), by kind and by
layer. Each row names its layer; a row that hides the same item in a lower layer says so, and the
hidden one is dimmed. A project copy whose source in the bank or the built-ins has changed since
it was copied is marked **stale**.

Choose an item to see and edit it:
- **Name, description and source** (vendor, part number, datasheet, notes), for every kind.
- **Sensors, lenses, lasers and mounts** have a form in a datasheet's units (µm, mm, degrees).
- **Targets, cameras, heads and rigs** are edited as JSON, checked as you type; the item saves
  once it checks.
- **Built-in items are read-only.** Duplicate one into your bank or the project to change it.

The actions:
- **New…** makes an item of a kind from typical values, in your bank or the project.
- **Duplicate…**, **Rename…** (a new id: the item is saved under it and the old file removed; the
  project's defaults follow it) and **Delete…**.
- **Import** reads item files into your bank or the project, checking each first. It does not
  replace an item with the same id.
- **Export** saves the item's JSON.
- **Use in project** copies a bank or built-in item into the project, with its origin.
- **Save to bank** copies a project item into your bank.
- **Update copy** replaces a stale copy with its source as it is now, keeping its id.

The desktop app's bank is `Documents/etendue/bank`, made the first time the studio starts. In a
browser, choose a folder with **File › Choose bank folder…** (Chrome and Edge); until then the
bank's actions say so. The bundled examples are read-only: open or create a project to add to it.
