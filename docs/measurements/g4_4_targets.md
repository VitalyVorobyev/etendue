# G4.4 — Detection for every target kind

- Gate: **G4.4** (`docs/pivot/PLAN.md`, P7-5), a characterisation: per target kind, detection
  rate, mislabels and RMS against the analytic ground truth, on Blender renders.
- Result: **every kind is detected in every view, with no mislabelled, unmatched or off-board
  label.**
  - Chessboard, ChArUco, marker board and PuzzleBoard: 48/48 views, every visible corner found,
    0.058 px RMS (max 0.14 px). The four planar kinds give the same corners to within 0.001 px:
    every kind is located by Radon at the same board corners.
  - PuzzlePole: 40/40 views, 4703 of 5837 visible corners (81 %), 0.063 px RMS. Every corner
    seen within 60° of its surface normal is found; the misses are all past 60°, the larger
    errors (14 corners above 0.3 px, max 0.85 px) all past 60° as well.
- Not covered by this run: calib-targets 0.15.3 **mislabels ChArUco and marker boards turned
  more than 45° in the image** (every id one square off). The rendered views stay within ±45°, so
  it does not show here; the CPU-rendered tests reproduce it (see *Known issue* below). Fixed in
  calib-targets 0.15.5.
- Measured: 2026-10-17, on etendue `f05a157` plus the uncommitted P7 work (branch
  `p7/authoring`), calib-targets 0.15.3, chess-corners 1.2.0, **Blender 5.2.2** (Cycles, Metal,
  Apple M4 Pro) with `--allow-blender-version`: the pin is 5.1.1, which was not installed. The
  render is not expected to depend on the minor version, but this has not been checked.

## Detection

`etendue detect` (`crates/etendue-cli/src/detect/`) takes the target from `gt.json`
(`target.geometry`; `dataset.json`'s board for datasets written before it), runs one corner pass
per image, and hands the corners to calib-targets' detector for the kind:

| Kind | Corners | Labeller | Matched to the board by |
|---|---|---|---|
| chessboard | Radon (`DetectorConfig::radon()`) | `ChessboardDetector`, rotation by square colour (P5-1b) | grid label |
| ChArUco | ChESS, located by Radon | `CharucoDetector` (`CharucoParams::for_board`) | ChArUco id |
| marker board | ChESS, located by Radon | `MarkerBoardDetector` (`for_board`) | aligned grid label |
| PuzzleBoard | ChESS, located by Radon | `PuzzleBoardDetector`, `FixedBoard` search | master id |
| PuzzlePole | ChESS, located by Radon | `PuzzlePoleDetector` (`for_pole`) | pole id |

*ChESS, located by Radon*: calib-targets' ChESS front end (`default_chess_config`) finds the
corners and each moves to the Radon corner within 1 px (Radon's threshold lowered to 0.05 of the
frame's strongest corner, since it only locates), keeping ChESS's axes and strength; a corner
Radon does not confirm is dropped. Feeding Radon's corners straight to these labellers fails:
their corner-strength floors are on ChESS's scale, and Radon's threshold (0.28 of the strongest
corner) lets the edges of a turned board through by the thousand. On a CPU-rendered ChArUco
board turned by 35° Radon gives 1509 corners (54 real) and the chessboard stage finds no grid;
ChESS gives 147. Raising Radon's threshold instead loses the dim side of a board under an
illumination gradient. Located corners keep Radon's accuracy (the same RMS as the chessboard's
pure-Radon corners above).

Every label is checked against the ground truth as before: a feature must lie within 1.5 px of
its own point's analytic pixel and nearer to it than to any other point's. Points facing away
from the camera (the far side of a pole) are not candidates. A label that names no point of the
target cannot be a feature and is counted as *off board*. Nothing is corrected.

## Procedure

```bash
cargo build --release -p etendue-cli
# Scenes: examples/closed_loop_ur5e with its board replaced by each kind (below)
uv run --locked --project tools/closed-loop python g4_4_scenes.py
for k in chessboard charuco marker_board puzzle_board puzzlepole; do
  sc=examples/closed_loop_ur5e/scenario.json
  [ $k = puzzlepole ] && sc=target/g4_4/puzzlepole/scenario.json
  ./target/release/etendue gt target/g4_4/$k/scene.json $sc -o target/g4_4/$k
  ./target/release/etendue render target/g4_4/$k/scene.json $sc -o target/g4_4/$k \
    --supersample 4 --samples 16 --sensor examples/closed_loop_ur5e/sensor_linear.json
  rm -rf target/g4_4/$k/exr
  ./target/release/etendue detect target/g4_4/$k
done
```

The renders take 1.8–5.5 min per kind (48 images; the pole 40).

- **Planar kinds:** `examples/closed_loop_ur5e` (a UR5e, a two-camera rig on `tool0`, 1280 × 1024,
  f = 2318.8 px) and its scenario (24 generated viewpoints, both cameras: 48 views), the board
  at the example's pose, each kind about the size of its 9 × 6 chessboard:
  - chessboard: 6 × 9 inner corners, 20 mm (93 px a square at 0.5 m);
  - ChArUco: 7 × 10 squares, 20 mm, markers 0.75, `DICT_4X4_50`;
  - marker board: 6 × 9 inner corners, 20 mm, circles at squares (4, 2) white, (5, 2) black,
    (5, 3) white;
  - PuzzleBoard: 10 × 12 squares, 16 mm, master origin (0, 0).
- **PuzzlePole:** 24 squares round (period 24, start row 242), 14 along the axis, 12 mm: 91.7 mm
  across, 168 mm tall. It stands upright on a stand, its foot at (0.85, 0, 0.25) m. The example's
  scenario looks down on the table, which sees a standing pole end on, so the pole has its own: 20
  side views of its middle from between the robot and the pole, 0.45–0.55 m away, 0–20° above
  level, within ±40° in azimuth, rolled by up to ±15°.
- **Rendering:** as `closed_loop_images.md`: s = 4, 16 Cycles samples per canonical pixel, a
  uniform white environment, the linear 8-bit sensor `sensor_linear.json`.

`g4_4_scenes.py` (run from the repository root; `target/` is not committed):

```python
"""G4.4 scenes: examples/closed_loop_ur5e with its board replaced by each printed kind."""

import copy
import json
import math
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, "tools/closed-loop")
from closed_loop import Iso, quat  # noqa: E402

base = json.loads(Path("examples/closed_loop_ur5e/scene.json").read_text())
base["robots"][0]["manifest"] = "../../../assets/robots/ur5e/robot.json"
boards = {
    "chessboard": {"kind": "chessboard", "inner_rows": 6, "inner_cols": 9, "square_size_mm": 20.0},
    "charuco": {"kind": "charuco", "rows": 7, "cols": 10, "square_size_mm": 20.0,
                "marker_size_rel": 0.75, "dictionary": "DICT_4X4_50"},
    "marker_board": {"kind": "marker_board", "inner_rows": 6, "inner_cols": 9, "square_size_mm": 20.0,
                     "circles": [{"i": 4, "j": 2, "polarity": "white"}, {"i": 5, "j": 2, "polarity": "black"},
                                 {"i": 5, "j": 3, "polarity": "white"}]},
    "puzzle_board": {"kind": "puzzle_board", "rows": 10, "cols": 12, "square_size_mm": 16.0},
    "puzzlepole": {"kind": "puzzlepole", "circumference_squares": 24, "start_row": 242,
                   "axial_squares": 14, "square_size_mm": 12.0},
}
pole_at = np.array([0.85, 0.0, 0.25])  # the pole's foot, on a stand; it is 0.168 m tall
for kind, target in boards.items():
    scene = copy.deepcopy(base)
    scene["targets"][0]["geometry"] = {"type": "printed", "target": target}
    if kind == "puzzlepole":
        scene["targets"][0]["parent_se3_self"]["translation"] = pole_at.tolist()
    out = Path("target/g4_4") / kind
    out.mkdir(parents=True, exist_ok=True)
    (out / "scene.json").write_text(json.dumps(scene, indent=2) + "\n")

# The pole's scenario: 20 side views of its middle from between the robot and the pole, at
# 0.45-0.55 m, 0-20 deg above level, +-40 deg in azimuth, the image's down along -Z rolled by
# up to +-15 deg.
rig = Iso.wire(base["rigs"][0]["parent_se3_self"])
centre = pole_at + np.array([0.0, 0.0, 0.084])
steps = []
for k in range(20):
    azimuth = math.radians(180 + 40 * math.sin(2.4 * k))
    elevation = math.radians(20 * ((k * 7) % 20) / 19)
    distance = 0.45 + 0.10 * ((k * 3) % 20) / 19
    roll = math.radians(15 * math.sin(1.7 * k))
    d = np.array([math.cos(elevation) * math.cos(azimuth), math.cos(elevation) * math.sin(azimuth),
                  math.sin(elevation)])
    eye = centre + distance * d
    z = -d
    x = np.cross(np.array([0.0, 0.0, -1.0]), z)
    x /= np.linalg.norm(x)
    y = np.cross(z, x)
    c, s = math.cos(roll), math.sin(roll)
    r = np.column_stack([x, y, z]) @ np.array([[c, -s, 0], [s, c, 0], [0, 0, 1]])
    tool = Iso(r, eye) * rig.inv()
    steps += [{"type": "ptp_pose", "robot": "ur5e",
               "base_se3_tool": {"rotation": quat(tool.r), "translation": tool.t.tolist()},
               "speed_scale": 0.5},
              {"type": "capture"}]
scenario = {"version": 1, "dt": 0.01, "description": "G4.4: 20 side views of a PuzzlePole", "steps": steps}
Path("target/g4_4/puzzlepole/scenario.json").write_text(json.dumps(scenario, indent=2) + "\n")
```

## Results

All views and all cameras (per-camera tables: `etendue detect`'s output):

| kind | views ok | points | GT visible | mislabelled | unmatched | off board | RMS vs GT px | max px |
|---|---|---|---|---|---|---|---|---|
| chessboard | 48/48 | 2592 | 2592 | 0 | 0 | 0 | 0.0580 | 0.137 |
| ChArUco | 48/48 | 2592 | 2592 | 0 | 0 | 0 | 0.0581 | 0.137 |
| marker board | 48/48 | 2592 | 2592 | 0 | 0 | 0 | 0.0580 | 0.137 |
| PuzzleBoard | 48/48 | 4752 | 4752 | 0 | 0 | 0 | 0.0576 | 0.142 |
| PuzzlePole | 40/40 | 4703 | 5837 | 0 | 0 | 0 | 0.0633 | 0.854 |

*GT visible*: points in front of the camera, facing it, inside the image's 2 px margin. No view
failed, so no view reports *no board*, *partial*, *ambiguous* or *undecoded*.

The PuzzlePole's corners by the angle between the surface normal and the line of sight:

| angle | 0–10° | 10–20° | 20–30° | 30–40° | 40–50° | 50–60° | 60–70° | 70–80° | 80–90° |
|---|---|---|---|---|---|---|---|---|---|
| found / visible | 219/219 | 644/644 | 803/803 | 773/773 | 650/650 | 656/656 | 694/720 | 264/693 | 0/679 |

Its median error is 0.036 px, 99 % are within 0.195 px, and the 14 corners above 0.3 px are seen
at 60–76°.

`closed_loop.py --features` reads the new `features.json` (the chessboard's and the ChArUco
board's, which have a `dataset.json`) and calibrates as in `closed_loop_images.md`.

## Known issue: turned ChArUco and marker boards (calib-targets 0.15.3; fixed in 0.15.5)

**Update (2026-10-17):** fixed in calib-targets 0.15.5 (calib-targets-rs#109), which converts
the cell alignment to the corner alignment (`calib_targets_core::cell_to_corner_alignment`).
etendue moved to 0.15.5 and `rotated_charuco_and_marker_boards` passes. The text below is the
original diagnosis.

calib-targets' chessboard stage labels corners up to the view (`u` along the image's +x). Once a
board is turned more than 45° in the image, its labels run turned against the print, and the
ChArUco and marker board detectors must turn them back. Both align on **cells**, named by their
top-left corner, and turn those names as if they were corners, then apply the same alignment to
the corners. A turned cell's top-left corner is a different corner of it, so the corner alignment
is off by one square: a quarter turn by one column, a half turn by one column and one row (the
translation should be `t_cell + (1, 0)`, `(1, 1)`, `(0, 1)` for 90°, 180°, 270°). Every id is then
wrong, consistently, and a pose solver would take it. In calib-targets 0.15.3 (and 0.15.4, and
`main` at 655e60a) this is `board_match/hypothesis.rs` (`transform.apply(c.gc)`) in
calib-targets-charuco and `corner_frame` in calib-targets-marker; PuzzleBoard and PuzzlePole
align corners and are not affected.

`crates/etendue-cli/tests/detect_targets.rs` reproduces it on CPU-rendered images:
`rotated_charuco_and_marker_boards` (ignored until calib-targets is fixed) has all 48 ChArUco
labels wrong at a 100° roll. The rendered views here stay within ±45°, where both detectors are
right. `etendue detect` reports such labels as mislabelled; it does not correct them. The fix
belongs upstream (a calib-targets PR, for review).

## Tests without Blender

`crates/etendue-cli/tests/detect_targets.rs` ray-casts each target's print
(`etendue_synth::board::layout`: the cells and disks Blender paints) through a pinhole,
area-sampled 4 × 4, and checks every label against the points' exact projections: chessboards in
eight rolls, under an illumination gradient, cut off and half-turn symmetric (as before); ChArUco,
marker board and PuzzleBoard rolled and tilted (PuzzleBoard also turned past 45°) and under a
gradient; a PuzzlePole from three azimuths, the seam among them.
