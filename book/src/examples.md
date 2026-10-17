# Examples

The examples are in `examples/` in the repository, and built into the desktop app.

| Example | The cell |
|---|---|
| `eye_in_hand_ur5e` | A stereo rig on a UR5e's flange views a 9 × 6 chessboard lying on the table. |
| `eye_to_hand_ur5e` | A static stereo rig on a stand views a chessboard held by the UR5e. |
| `closed_loop_ur5e` | A two-camera rig mounted 15°/10° off square on a UR5e, 24 viewpoints of a 9 × 6 chessboard (20 mm squares) at 0.45–0.55 m, made by the sphere pose generator: a hand-eye calibration dataset. It comes with `sensor_linear.json`, a noise-free linear 8-bit sensor. |
| `laser_ur5e` | The `closed_loop_ur5e` rig with a line laser (660 nm, 20° half-angle): a laser-line calibration dataset. |
| `laser_scheimpflug_ur5e` | The laser rig with Scheimpflug sensors, tilted about 3° and 2°. |
| `rig3_laser_ur5e` | A laser triangulation sensor: a ring of three cameras on the UR5e flange converging 0.5 m ahead, one line laser per camera, 70 poses on spheres around a 9 × 6 chessboard made by the pose generator (its spec is in the scenario's `generator`). |

Try one end to end (from a source checkout with the robot meshes built — see
[Installation](installation.md); the command-line archive has no robot library):

```bash
etendue validate examples/closed_loop_ur5e/scene.json examples/closed_loop_ur5e/scenario.json
etendue gt       examples/closed_loop_ur5e/scene.json examples/closed_loop_ur5e/scenario.json -o out
etendue render   examples/closed_loop_ur5e/scene.json examples/closed_loop_ur5e/scenario.json -o out \
                 --supersample 4 --samples 16 --sensor examples/closed_loop_ur5e/sensor_linear.json
etendue detect   out
```

`out/dataset.json` is then a calibration-rs rig hand-eye dataset of the rendered images, and
`out/gt.json` the rig and hand-eye it should recover.
