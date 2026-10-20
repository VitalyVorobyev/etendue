# etendue

[![CI](https://github.com/VitalyVorobyev/etendue/actions/workflows/ci.yml/badge.svg)](https://github.com/VitalyVorobyev/etendue/actions/workflows/ci.yml)
[![Docs](https://github.com/VitalyVorobyev/etendue/actions/workflows/publish-docs.yml/badge.svg)](https://vitalyvorobyev.github.io/etendue)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

etendue designs camera and laser sensors, lays them out on robots, and generates synthetic
calibration datasets with exact ground truth — then calibrates them and tells you how close the
calibration came to the truth.

![etendue studio: the closed-loop calibration example in the Setup workspace, with the UR5e, its two-camera rig and the board in 3D at a capture](book/src/images/studio.png)

## What it does

- **Optics**: design a laser-triangulation head — sensor, lens, line laser, f-number, focus and
  Scheimpflug tilt — against the window it must measure, with blur, depth resolution and stripe
  width mapped over the window, an optimiser and a lens search over your library.
- **Rigs**: repeat a camera or a head as a stereo pair or a ring, and see which points two or
  more cameras see sharply, and which a laser and its camera measure.
- **Cells and robots**: build a cell around a Universal Robots UR5e, an ABB IRB 1200 or an igus
  ReBeL; program it point to point and in straight lines, jog it by hand, or generate calibration
  viewpoints on spheres around the target. Every move is jerk-limited within the robot's limits,
  and a motion report flags joint limits and singularities.
- **Ground truth**: every camera's true model and pose, the hand-eye transform, the laser planes,
  and the pixel of every target corner and laser-stripe point in every view, as
  [calibration-rs](https://github.com/VitalyVorobyev/calibration-rs) datasets.
- **Synthetic images**: Blender renders each capture, mapped exactly onto the real camera model
  (distortion, Scheimpflug tilt), through an optional noisy sensor, with laser-stripe images.
- **Targets**: chessboards, ChArUco boards, marker boards, PuzzleBoards and the PuzzlePole
  cylinder, drawn, ground-truthed and detected, every label checked against the truth.
- **Calibration**: calibrate a dataset from exact, noisy or detected observations, compare every
  estimate with the truth, and keep and compare the runs.

## Install

Download the desktop app and the command line from the
[latest release](https://github.com/VitalyVorobyev/etendue/releases/latest):

| Platform | Desktop app | Command line |
|---|---|---|
| macOS (Apple silicon) | `etendue-studio-<version>-aarch64-apple-darwin.dmg` | `etendue-cli-<version>-aarch64-apple-darwin.tar.gz` |
| Windows (x64) | `…-x86_64-pc-windows-msvc.msi` or `…-setup.exe` | `etendue-cli-<version>-x86_64-pc-windows-msvc.zip` |
| Linux (x64) | `…-x86_64-unknown-linux-gnu.AppImage` or `.deb` | `etendue-cli-<version>-x86_64-unknown-linux-gnu.tar.gz` |

`SHA256SUMS.txt` in the release lists every file's checksum. The builds are not signed yet: on
macOS open the app the first time with right-click → **Open**; on Windows choose **More info →
Run anyway**. Rendering needs [Blender 5.2](https://www.blender.org/download/); everything else
works without it. The [installation chapter](https://vitalyvorobyev.github.io/etendue/installation.html)
has the details and how to build from source.

## Quick start: the studio

1. Open **etendue studio**. The first time it opens the built-in examples; pick another cell from
   the **File** menu, or open your own project with **File › Open project…**.
2. **Setup** shows the cell in 3-D. Press play to run the robot program; **Images** shows what
   every camera sees, with the target projected through each camera's model.
3. **Motion** lists the program with its timing and warnings; **Jog** moves the robot by hand,
   and **Generate poses** makes calibration viewpoints.
4. **Optics** and **Rigs** design heads and rigs from the project's library.
5. **Calibrate** generates the dataset (ground truth, Blender renders, detection), calibrates it,
   and draws the estimate beside the truth.

## Quick start: the command line

```bash
etendue validate  scene.json scenario.json          # check the cell; every move reachable and within limits
etendue gt        scene.json scenario.json -o out   # ground truth: gt.json, dataset.json, robot_poses.json
etendue calibrate out                               # calibrate the exact observations, compare with the truth
etendue render    scene.json scenario.json -o out --supersample 4 --samples 16 --sensor sensor.json
etendue detect    out                               # features.json, checked against the truth
etendue calibrate out --features out/features.json  # calibrate from the rendered images
```

`etendue <command> --help` describes every option. The examples are in [`examples/`](examples/).

## Documentation

- **The book**: https://vitalyvorobyev.github.io/etendue — installation, concepts, the user guide,
  and the models behind the tools.
- **Tutorials**: [your first synthetic dataset](https://vitalyvorobyev.github.io/etendue/tutorials/first_dataset.html),
  designing a head, laying out a rig, programming a robot, and the command-line pipeline.
- **File formats**: JSON Schemas in [`schemas/`](schemas/), described in the
  [book](https://vitalyvorobyev.github.io/etendue/formats.html).
- **API**: the Rust crates and the `@etendue/wasm` package, at
  https://vitalyvorobyev.github.io/etendue/api/.
- **Contributing**: [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT — see [LICENSE](LICENSE).
