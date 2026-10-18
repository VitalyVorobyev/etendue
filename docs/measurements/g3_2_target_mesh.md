# G3.2 — Every calib-targets target, drawn as printed

- Gate: **G3.2** (`docs/pivot/PLAN.md`, P7-4)
- Criterion: for a chessboard, a ChArUco board, a marker board, a PuzzleBoard and a PuzzlePole,
  as a scene's `printed` target:
  - the scene's mirror type and calib-targets' own `TargetSpec` are the same JSON;
  - the target mesh's base layer covers the board's area (planar) or the cylinder's `2π r L`
    to 1e-4 relative, and its cells tile the surface without overlap (a 97 × 83 probe grid,
    each probe in exactly one cell);
  - every feature point is a corner of the drawn checker (around it the squares alternate);
  - every triangle faces outward (its winding agrees with its vertex normals);
  - disks: their rims stay within 1 µm of their circles;
  - the PuzzlePole's points agree with calib-targets' pole object frame
    (`calib_targets_puzzleboard::pole::geometry::object_position`, `f32`) to 1e-3 mm, and its
    surface closes at the seam (the window's two edges map to the same circle to 1e-12 m).
- Result: **PASS**. Pole points within 2.4e-5 mm of calib-targets; base areas exact for the
  planar targets and 5.7e-6 relative short for the pole (the chords of a polygon inscribed in its
  circumference, each within 1 µm of the surface).
- Measured: 2026-10-17, on etendue `f05a157` plus the uncommitted P7 work (branch
  `p7/authoring`), calib-targets 0.15.3.

## Setup

`crates/etendue-synth/tests/g3_2.rs`. The targets are built with calib-targets' own
constructors, serialised, and read as etendue's `PrintedTarget`; the round trip must reproduce
calib-targets' JSON:

| Target | calib-targets constructor |
|---|---|
| chessboard | `ChessboardTargetSpec::new(6, 9, 25.0)` |
| ChArUco | `CharucoTargetSpec::new(5, 7, 30.0, 0.75, DICT_4X4_50)` |
| marker board | `MarkerBoardTargetSpec::new(6, 9, 25.0, default_circles(6, 9))` |
| PuzzleBoard | `PuzzleBoardTargetSpec::new(10, 12, 15.0)` |
| PuzzlePole | `PuzzlePoleTargetSpec::new(12, 6, 30.0)` (12-period, the paper's 114.6 mm pole) |

**The mesh** (`etendue_synth::target_mesh`): cells from the print's rectangles (tiled on one grid,
no T-junctions); disks as polar grids lifted 10 µm along the surface normal, their rim segments
chosen so the chord error is ≤ 1 µm; on a cylinder, cells cut along the circumference so no chord
strays more than 1 µm, and disk rings close enough that a disk stays above the cells. A
PuzzlePole's strip is mapped through its seamless window `v ∈ [s/2, s/2 + C)` with `θ = 2π v / C`
into calib-targets' pole object frame (axis +Z, the seam at +X, ink outward), and closed by two
paper end caps.

## Result

`cargo test --release -p etendue-synth --test g3_2 -- --nocapture`:

| Target | Triangles (base) | Disks | Points | Base area (m²) | Expected (m²) |
|---|---:|---:|---:|---:|---:|
| chessboard | 140 (140) | 0 | 54 | 4.375000e-2 | 4.375000e-2 |
| ChArUco | 4480 (4480) | 0 | 24 | 3.150000e-2 | 3.150000e-2 |
| marker board | 668 (140) | 3 | 54 | 4.375000e-2 | 4.375000e-2 |
| PuzzleBoard | 24656 (240) | 218 | 99 | 2.700000e-2 | 2.700000e-2 |
| PuzzlePole | 111872 (6528) | 132 | 60 | 6.479963e-2 | 6.480000e-2 |

PuzzlePole points vs calib-targets `object_position`: max **2.372e-5 mm** (gate 1e-3 mm; the
residual is calib-targets' `f32`).

## What the targets are used for

| Target | Drawn (web, Blender) | Ground truth | `dataset.json` (calibration-rs) |
|---|---|---|---|
| chessboard | yes | yes | yes |
| ChArUco (OpenCV layout, 1-bit border, no inset) | yes | yes | yes |
| marker board | yes | yes | no: calibration-rs has no marker board |
| PuzzleBoard | yes | yes | no: calibration-rs names PuzzleBoards by layout |
| PuzzlePole | yes | yes (3-D points, outward normals) | no: calibration-rs has no non-planar target |

## Reproduce

```bash
cargo test --release -p etendue-synth --test g3_2 -- --nocapture
```
