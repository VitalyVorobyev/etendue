# Projects and the library

A **project** is a folder that holds everything you make: cells, the component library, and
later the datasets and calibration runs made from them. The repository's `examples/` folder is
one, and the studio's starter project.

```
<project>/
  etendue-project.json
  library/{sensors,lenses,lasers,targets,cameras,rigs}/<id>.json
  robots/<id>/robot.json …            (optional)
  <cell>/scene.json, <cell>/scenario.json
```

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

- **`cells`** lists the cell folders. Each holds a `scene.json` and usually a `scenario.json`.
- **`robots`** lists robot folders that come with the project. A scene finds its robots by the
  path in the scene first, then in the project, then in the app's library.
- **`defaults`** names the library items that new cameras, lasers and targets are made from.
  A reference is `<folder>/<id>`.
- **Paths** are relative to the project folder and use `/`.

Its JSON Schema is `schemas/project.schema.json`.

## Library items

Each item is one file, `library/<folder>/<id>.json`. It holds exactly one part, under the key
that names its kind:

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

| Kind | Folder | What it holds |
|---|---|---|
| `sensor` | `sensors` | pixel pitch, resolution; full well, read noise and quantum efficiency when known |
| `lens` | `lenses` | focal length, widest aperture, principal-plane gap and entrance pupil (0 for a thin lens), focusing, mount |
| `laser` | `lasers` | wavelength, fan half-angle, beam waist, power |
| `target` | `targets` | a target's geometry, as a scene holds it |
| `camera` | `cameras` | a camera design: its model, image size and, when physical, its optics ([Physical cameras](physical_cameras.md)) |
| `rig` | `rigs` | a rig layout, its camera (and lasers) included |

- **The id is the file name.**
- **`version` is read first.** An etendue that reads version 1 refuses a version 2 file for its
  version, not for a field it does not know.
- **Designs embed their parts by value,** so a library file is complete on its own.
- **A scene copies a design by value.** It never depends on a library being present, and a
  dataset made from it never dangles.

The schema is `schemas/library.schema.json`. The seed values in `examples/library` come from
datasheets and are illustrative: check them before a real design.

## Checking a project

```bash
etendue project check examples
```

The check covers:
- the manifest;
- every cell: the scene with its robots and optics, and the scenario compiled in full;
- every library item: its version, its id against its file name, its kind against its folder, and
  a camera design's params against its optics;
- the defaults: they name existing items;
- the project's robots.

Every problem is reported, one per line.
