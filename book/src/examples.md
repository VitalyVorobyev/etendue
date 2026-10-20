# Examples

The examples are the `examples/` folder of the repository, and are built into the desktop app,
where they are the cells of the project the studio opens at start. `examples/` is itself a
[project](library.md): its manifest is `examples/etendue-project.json`, its component library is
`examples/library/`, and each example is a cell folder with a `scene.json` and a `scenario.json`.

Every example uses a Universal Robots UR5e, a 9 × 6 inner-corner chessboard and cameras of
1280 × 1024 pixels.

| Example | The cell | Dataset |
|---|---|---|
| `eye_in_hand_ur5e` | A stereo rig on the UR5e's flange views a chessboard (25 mm squares) lying on the table. Nine viewpoints around the board, then a 6 cm straight-line pass: 10 captures. | rig hand-eye, eye-in-hand |
| `eye_to_hand_ur5e` | A static stereo rig on a stand views a chessboard (25 mm squares) held by the UR5e. Nine board presentations. | rig hand-eye, eye-to-hand |
| `closed_loop_ur5e` | A two-camera rig mounted 15° and 10° off square on the flange, with lens distortion, views a chessboard (20 mm squares) at 0.45–0.55 m. 24 viewpoints made by the pose generator (its spec is kept in the scenario). Comes with `sensor_linear.json`, a noise-free linear 8-bit sensor. | rig hand-eye, eye-in-hand |
| `laser_ur5e` | The `closed_loop_ur5e` rig with a red line laser (660 nm, 20° fan half-angle) between the cameras, imaged by both. | rig hand-eye laser-line |
| `laser_scheimpflug_ur5e` | The laser rig with Scheimpflug sensors, tilted about 3° and 2°. It does not pass the exact-data check: see [Calibration](calibration.md#the-tolerances). | rig hand-eye laser-line |
| `rig3_laser_ur5e` | A laser triangulation sensor: a ring of three cameras (60 mm radius) on the flange, converging 0.5 m ahead, with one line laser beside each camera. 70 viewpoints on three spheres, from the pose generator. | rig hand-eye laser-line, one laser per camera |

`examples/library/` holds illustrative datasheet parts:
- sensors: Sony IMX174 and IMX249, ON Semiconductor Python 1300;
- lenses: Kowa LM16HC (16 mm) and LM25HC (25 mm), Computar M5018-MP2 (50 mm);
- line lasers: Coherent StingRay 660 (red), Z-LASER ZM18 532 nm (green), Osram PLT5 450B (blue);
- camera designs: an IMX174 with a Kowa LM25HC at f/4 focused at 0.5 m (physical), and the
  examples' pinhole camera;
- targets: 9 × 6 chessboards with 20 mm and 25 mm squares;
- rigs: the `rig3_laser_ur5e` ring of three cameras with a laser beside each.

The project's defaults make new cameras the IMX174 with the LM25HC, new lasers the StingRay, and
new targets the 20 mm chessboard.

## Try one end to end

From a source checkout with the robot meshes built (see [Installation](installation.md); the
command-line archive has no robot library):

```bash
etendue validate  examples/closed_loop_ur5e/scene.json examples/closed_loop_ur5e/scenario.json
etendue gt        examples/closed_loop_ur5e/scene.json examples/closed_loop_ur5e/scenario.json -o out
etendue calibrate out                                         # exact observations: PASS
etendue render    examples/closed_loop_ur5e/scene.json examples/closed_loop_ur5e/scenario.json -o out \
                  --supersample 4 --samples 16 --sensor examples/closed_loop_ur5e/sensor_linear.json
etendue detect    out
etendue calibrate out --features out/features.json             # from the rendered images
```

Only `render` needs Blender and the robot meshes. `out/dataset.json` is then a calibration-rs
rig hand-eye dataset of the rendered images, and `out/gt.json` the rig and hand-eye it should
recover. The [first-dataset tutorial](tutorials/first_dataset.md) does the same in the studio.

## Regenerating the generated scenarios

The scenarios of `closed_loop_ur5e`, `laser_ur5e`, `laser_scheimpflug_ur5e` and
`rig3_laser_ur5e` keep their generator spec, so the pose generator can run them again:

```bash
etendue scenario sphere examples/closed_loop_ur5e/scene.json \
    --spec examples/closed_loop_ur5e/scenario.json -o scenario.json
```

```text
24 of 48 candidate poses kept, 22.44 s of motion → scenario.json
```

## Checking the project

```bash
etendue project check examples
```

```text
project ok: etendue examples: 6 cell(s), library 2 cameras, 3 lasers, 3 lenses, 1 rigs, 3 sensors, 2 targets, 0 robot(s)
```
