# The studio

etendue studio opens a scene and its scenario, plays the robot's motion in 3D, and shows what each
camera sees. In the desktop app it also generates datasets: ground truth, Blender renders and
board detection, in one run.

![The studio with the hand-eye calibration example (closed_loop_ur5e) at a capture](images/studio.png)

## Opening a scene

- **Examples**: the example menu in the header lists the built-in cells (see
  [Examples](examples.md)).
- **Open scene…** (desktop): pick a scene file, and optionally its scenario in the same dialog.
  The robots the scene names are loaded from their manifest paths; a robot that is not there is
  looked up in the built-in robot library.
- **Open files…** or drag and drop (browser): drop a scene, an optional scenario, and the robot
  files it needs.
- **Import poses…** (desktop): replace the scenario with a stop-and-shoot one built from a file
  of tool poses, the same as `etendue scenario from-poses` (see [Command line](cli.md)).

A scene that does not validate keeps the previous one open and lists its problems.

## The window

- **Scene frames** (left): the frame tree — the world, robots and their links, rigs, cameras,
  lasers, lights, targets and parts. Select an item here or click it in 3D to show it in the
  inspector; selecting a camera also shows its view.
- **Viewport** (centre): the cell in 3D, with camera frusta, laser fans, lights and the board.
  Drag to orbit; with the viewport focused, the arrow keys orbit (Shift for larger steps), `+`
  and `−` zoom and `0` resets the view.
- **Inspector** (right): the selected item's pose (translation in mm, roll/pitch/yaw in degrees)
  and parameters — a camera's model, intrinsics and resolution, a laser's fan, a robot's joints.
- **Playback** (bottom bar): play, pause, scrub and loop the scenario at a chosen speed. Captures
  are marked on the timeline.
- **Camera tabs**: one per camera, showing the camera's view at the current sample, with the
  board's corners projected through the camera model ("Projected targets"). Zoom and pan the
  image; "Actual size" shows it at 100 %.
- **Joints**: the robot's joint positions over the scenario, with the playhead.

## Generating a dataset (desktop)

The **Dataset** tab runs the native pipeline on the current scene and scenario:

| Setting | Meaning |
|---|---|
| Output folder | Where the dataset goes. For a built-in example it defaults to `Documents/etendue/<example>`; for your own scene, `dataset/` next to it. |
| Render with Blender | Off: the analytic ground truth only. |
| Samples | Cycles samples per canonical pixel. |
| Supersampling | Canonical render pixels per camera pixel (4 is converged for corner accuracy). |
| Sensor model | A sensor model file gives linear raw images (with noise and quantisation); none gives sRGB images. The examples that have one select it. |
| Cameras | Which cameras to render. |
| Render on the CPU | Bit-identical between runs, and slower. |

The top of the panel shows whether Blender was found and whether its version matches the pin.
**Generate** writes the ground truth, renders every capture (progress and Blender's log are
shown, and **Cancel** stops Blender), and detects the board in every image. The table at the
end summarises the detection per camera: views kept, corners found, labels that disagree with the
ground truth, and the RMS distance of the detected corners from their true pixels.

Afterwards each camera tab can show the **dataset image** at a capture instead of the live view.
A new run starts from an empty dataset: the previous run's images and detections are removed.

The files written are described in [Datasets](datasets.md).
