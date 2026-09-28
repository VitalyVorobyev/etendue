# P3-3 — board geometry from calib-targets

- Item: **P3-3** (`docs/pivot/PLAN.md`), ADR 0006.
- What changed:
  - A board target is now what calib-targets prints. `etendue_synth::board::layout` takes the
    pattern from `calib_targets_print::board_primitives` and the feature points from
    `TargetSpec::resolved_points`. That covers chessboard and ChArUco boards; ChArUco uses
    OpenCV's marker layout and the scene's dictionary, as calibration-rs's detector does.
  - The primitives are tiled into non-overlapping cells on one grid (no T-junctions).
    `etendue render` builds the Blender board from those cells, and `etendue gt` takes its
    points from the same layout.
  - The mesh is exact, so the SVG-texture fallback is not needed.
- Measured: 2026-09-28, on etendue commit `69b13ea` (branch `pivot/p3-3-target-geometry`),
  calibration-rs `76353753` (0.8.3), calib-targets 0.15.3, chess-corners 1.2.0, Blender 5.1.1
  (Cycles, Metal, Apple M4 Pro).

## The frame mapping, and why it flips y

calib-targets' board space is millimetres from the top-left corner, with x right and y down. As
a right-handed frame, its z points **into** the paper. etendue's target frame faces **+Z** (a
board on a table faces up). A viewer in front of the board looks along −Z, so the mapping is:

`target = (x / 1000 − w / 2, h / 2 − y / 1000)`

The print's x runs along +X, its "down" along −Y, and its top-left corner is at −X/+Y. Mapping
print y to +Y instead would show the print mirrored from the front. A chessboard looks the same
mirrored, which is how the first version passed its chessboard checks. ArUco markers don't: in
the first ChArUco renders no marker decoded until the image was mirrored back. The test
`the_board_faces_plus_z` pins the mapping down.

For a board with an even number of square rows, the dark top-left square of the print now
sits at −X/+Y, and the −X/−Y corner is light. The G4.2 exact reference and the G4.3 web board
follow the print's parity.

## Checks

**Unit tests** (`etendue_synth::board`):
- The cells tile the board at the scene's `extent_m` without overlap.
- Every feature point is a checker corner of the drawn cells.
- The chessboard points run from the print's top-left, with `grid = [column, row]`.
- ChArUco corners are numbered 0..n, and its markers split the board into many cells.

**Blender, ChArUco (the eye-in-hand example with a 10 × 7 DICT_4X4_50 board):**
- Of 20 views, 3 had no detection and one detected a single marker.
- In the rest, every decoded marker `k` sits within 0.6 px of the k-th white square of the
  print, projected through a homography fitted to that view's ground-truth corners.
- The ground-truth corners land within 0.3 px of the detected corners.
- Corner **ids** from calib-targets 0.15.3's ChArUco detector are one square off in every view
  where the board appears rotated. The print, the render and the ground truth all agree, so
  this is a detector bug. It reproduces on calib-targets' own printed page rotated by 90°:
  [calib-targets-rs#106](https://github.com/VitalyVorobyev/calib-targets-rs/issues/106).

**Blender, chessboard: G4.2 re-run** (`etendue measure g4-2`; the board is now built from the
cells, and this 10 × 8 board has an even number of rows, so its parity flipped). Sharp, linear,
RMS / max in px, over 315 corners (all matched):

| s | center_of_mass | forstner | saddle_point |
|---|---|---|---|
| 1 | 0.131 / 0.273 | 0.119 / 0.217 | 0.104 / 0.206 |
| 4 | 0.183 / 0.385 | 0.073 / 0.278 | 0.126 / 0.286 |
| 8 | 0.186 / 0.389 | 0.070 / 0.271 | 0.126 / 0.287 |

This is the same as the recorded G4.2 (`g4_2_corner_bias.md`) to within Monte Carlo noise.
The render-vs-exact difference at s = 4 is 0.032 / 0.057 / 0.060 px (recorded:
0.032 / 0.059 / 0.063).

**G4.3 re-run** (web `SensorView` vs Blender, s = 4): 0.0192 / 0.0378 / 0.0424 px RMS (recorded:
0.0184 / 0.0403 / 0.0426). It still **passes**.

**Closed loop G5.1** (analytic correspondences) on both examples is unchanged: 1.8e-8 px and
4.3e-7 px mean reprojection.

**wasm:** the graph now includes calib-targets, but the wasm facade does not call it yet. G2.1
is 319,533 B gzipped (was 319,254 B), and G0.1 is still bit-for-bit.

## Open

- **The web viewers still draw a checker** (`@vitavision/three` `TargetBoard`, dark at −X/−Y).
  That matches the print for boards with an odd number of square rows, which covers both
  examples. Boards with an even number of rows, and ChArUco markers, need `TargetBoard` to draw
  the cells: a `cells` option in `@vitavision/three`, and the layout exposed by
  `@etendue/wasm`.
- ChArUco can't enter an image-level closed loop until calib-targets-rs#106 is fixed.
- The puzzleboard is unmapped: its layout is named in the calibration-rs dataset vocabulary.
  The ring grid is not a calib-targets target, so it still renders as a plain surface.
