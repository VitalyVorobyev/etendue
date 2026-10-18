# etendue

[![CI](https://github.com/VitalyVorobyev/etendue/actions/workflows/ci.yml/badge.svg)](https://github.com/VitalyVorobyev/etendue/actions/workflows/ci.yml)
[![Docs](https://github.com/VitalyVorobyev/etendue/actions/workflows/publish-docs.yml/badge.svg)](https://vitalyvorobyev.github.io/etendue)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

etendue designs robot–camera–laser calibration cells and generates synthetic calibration
datasets with exact ground truth.

You describe a cell — a robot, camera rigs, line lasers, lights, a calibration board — and a
scenario of robot moves and captures. etendue bakes the motion with the robot's kinematics,
computes where every board corner and laser-stripe point lands in every camera, renders the
images with Blender, and detects the board in them. The result is a dataset in
[calibration-rs](https://github.com/VitalyVorobyev/calibration-rs)'s format, together with the
ground truth that a calibration of it should recover.

![etendue studio: the hand-eye calibration example, with the UR5e, its two-camera rig and the board in 3D, and the left camera's view at a capture below](book/src/images/studio.png)

## What it does

- **The studio** (a desktop app; built from source, it also runs in a browser, without dataset
  generation) shows the cell in 3D, plays the scenario back, and shows what each camera sees at
  every capture, with the board's corners projected through the full camera model (distortion,
  Scheimpflug tilt).
- **Ground truth**: for every capture, the camera poses, the robot pose and the pixel of every
  board corner and laser-stripe point, with visibility. Written as calibration-rs datasets: single
  camera intrinsics (planar or Scheimpflug), rig extrinsics, eye-in-hand and eye-to-hand hand-eye
  for one camera or a rig, and a laser-line rig.
- **Synthetic images**: Blender (Cycles) renders each capture through an ideal pinhole, and
  etendue resamples it onto the real camera model, so distortion and tilt are exact. An optional
  sensor model adds exposure, noise and quantisation and writes raw images.
- **Detection**: chessboard corners are found and labelled in the rendered images and checked
  against the ground truth, so the image-level loop can be calibrated end to end.
- **Robots**: URDF robots with forward and inverse kinematics; the Universal Robots UR5e and the
  ABB IRB 1200 are included. Scenarios move the robot point to point or in straight lines, or
  come from a list of tool poses.

## Install

Download the desktop app and the command-line tool from the
[releases page](https://github.com/VitalyVorobyev/etendue/releases) (once a version is released;
until then, build from source):

| Platform | Desktop app | Command line |
|---|---|---|
| macOS (Apple silicon) | `.dmg` | `etendue-cli-…-aarch64-apple-darwin.tar.gz` |
| Windows (x64) | `.msi` or `-setup.exe` | `etendue-cli-…-x86_64-pc-windows-msvc.zip` |
| Linux (x64) | `.AppImage` or `.deb` | `etendue-cli-…-x86_64-unknown-linux-gnu.tar.gz` |

The builds are not signed yet. On macOS, open the app the first time with right-click → Open
(or run `xattr -dr com.apple.quarantine "/Applications/etendue studio.app"`); on Windows,
choose "More info → Run anyway" in SmartScreen.

Rendering needs [Blender 5.1.1](https://www.blender.org/download/releases/5-1/). etendue looks
for it in `$ETENDUE_BLENDER`, then in the default install location (`/Applications/Blender.app`
on macOS, `blender` on the `PATH` elsewhere). Everything else works without it.

To build and run from source (Rust, Node.js, bun, wasm-pack and uv; calibration-rs checked out
next to etendue):

```bash
uv run --locked --project tools/robot-assets tools/robot-assets/build.py   # robot meshes
node crates/etendue-wasm/scripts/build-npm.mjs
(cd web && bun install)
(cd web/apps/studio && bun run desktop:dev)        # the desktop app, run from the checkout
(cd web/apps/studio && bun run desktop:build)      # or an installable app in src-tauri/target/release/bundle
```

The [installation chapter](https://vitalyvorobyev.github.io/etendue/installation.html#from-source)
of the documentation has the prerequisites and the command line.

## Quick start: the studio

1. Open **etendue studio**. It starts with a built-in example; pick another from the example
   menu, or open your own scene (and scenario) with **Open scene…**.
2. Press play to watch the scenario. Select a camera in the scene tree to see its view, with
   the board's corners projected at each capture. The **Joints** tab plots the robot's motion.
3. In the **Dataset** tab, choose an output folder and press **Generate**: etendue writes the
   ground truth, renders every capture with Blender, and detects the board. The camera views
   then show the rendered images.

## Quick start: the command line

```bash
etendue validate scene.json scenario.json          # check the cell and that every move is reachable
etendue gt       scene.json scenario.json -o out   # ground truth: dataset.json, gt.json (+ robot_poses.json)
etendue render   scene.json scenario.json -o out --supersample 4 --samples 16   # images/<camera>/<capture>.png
etendue detect   out                               # features.json, checked against the ground truth
etendue scenario from-poses poses.json --robot ur5e -o scenario.json --scene scene.json
```

`etendue <command> --help` describes every option.

## Examples

| Example | What it shows |
|---|---|
| `examples/eye_in_hand_ur5e` | A stereo rig on a UR5e's flange views a chessboard on the table. |
| `examples/eye_to_hand_ur5e` | A static stereo rig views a chessboard held by the UR5e. |
| `examples/closed_loop_ur5e` | A rig mounted off square, 24 generated viewpoints for a hand-eye calibration, and a linear sensor model. |
| `examples/laser_ur5e` | The same rig with a line laser: a laser-plane calibration dataset. |
| `examples/laser_scheimpflug_ur5e` | The laser rig with Scheimpflug-tilted sensors. |

## File formats

Scenes, scenarios, baked scenarios and robot manifests are versioned JSON documents. Their JSON
Schemas are in [`schemas/`](schemas/); editors that understand JSON Schema validate and complete
them. The [documentation](https://vitalyvorobyev.github.io/etendue) describes each one, and the
dataset layout etendue writes.

## Documentation

The user guide, the CLI reference and the optics behind the design tools are at
**https://vitalyvorobyev.github.io/etendue**. The Rust API documentation is at
https://vitalyvorobyev.github.io/etendue/api/.

## License

MIT — see [LICENSE](LICENSE).
