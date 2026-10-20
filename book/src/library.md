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
│   └── rigs/<id>.json
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

Every item has `version`, `id`, `name` and an optional `description`.

| Kind | Folder | What it holds |
|---|---|---|
| `sensor` | `sensors` | `pixel_pitch_m`, `resolution`; when known, `well_depth_e`, `read_noise_e` and `peak_qe` |
| `lens` | `lenses` | `focal_length_m`, `min_f_number` (the widest aperture), `mount` (`c_mount`, `cs_mount`, `f_mount`, `m12`, `other`); `principal_gap_m` and `entrance_pupil_m` (0 for a thin lens); `focusing`; when known, `fov_diagonal_deg` and `mtf50_lp_mm` |
| `laser` | `lasers` | `wavelength_nm`, `fan_half_angle`; when known, `beam_waist_m`, `waist_distance_m` (where it is focused; absent, the laser is focusable and a head design focuses it) and `power_mw` |
| `target` | `targets` | `geometry`: a target as a scene holds it ([Targets](scenes.md#targets)) |
| `camera` | `cameras` | a camera design: its model, image size and, when physical, its optics ([Physical cameras](physical_cameras.md)) |
| `head` | `heads` | a laser-triangulation head: a physical camera, a laser, the laser's pose on the camera's body (`camera_se3_laser`), its fan length, and the window it must work in (`near_m`, `far_m`, `width_m`, `max_blur_px`) |
| `rig` | `rigs` | a rig layout, its camera and lasers included ([Rig layouts](scenes.md#rig-layouts)) |

- **The id is the file name.**
- **`version` is read first.** An etendue that reads version 1 refuses a version 2 file because
  of its version, not because of a field it does not know.
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

Every problem is reported, one per line, and the command fails.
