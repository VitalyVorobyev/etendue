# The studio

etendue studio opens a scene and its scenario, plays the robot's motion in 3D, and shows what each
camera sees. In the desktop app it also generates datasets: ground truth, Blender renders and
board detection, in one run.

![The studio with the hand-eye calibration example (closed_loop_ur5e) at a capture](images/studio.png)

## Opening and saving

Everything that opens, saves or exports is in the **File** menu.

- **Examples**: the built-in cells (see [Examples](examples.md)).
- **Open scene…** (desktop): pick a scene file, and optionally its scenario in the same dialog.
  The robots the scene names are loaded from their manifest paths; a robot that is not there is
  looked up in the built-in robot library.
- **Open files…** or drag and drop (browser): drop a scene, an optional scenario, and the robot
  files it needs.
- **Import tool poses…** (desktop): replace the scenario with a stop-and-shoot one built from a
  file of tool poses, the same as `etendue scenario from-poses` (see [Command line](cli.md)).
- **Save** and **Save as…** (desktop): write the scene and its scenario (`scenario.json` next to
  `scene.json`). The robot manifest paths in the scene are rewritten relative to where it is
  saved. An example is never overwritten: saving it asks where. In a browser, **Download scene**
  downloads both files.
- **Export**: the scenario, or the baked motion (every frame's pose at every sample).

A scene that does not validate keeps the previous one open and lists its problems.

## Editing

Every change in the inspector, the scene tree or the motion is baked at once, and the views
follow. A change that would make the scene invalid (an entity attached to nothing, a rest pose
outside the joint limits) is refused with the reason, and nothing changes. **Undo** and **Redo**
are in the header (⌘Z and ⇧⌘Z; Ctrl on Windows and Linux), and a ● after the document's name
marks unsaved changes.

## Workspaces

The header switches between workspaces. Each keeps the 3D view or the camera images as its main
surface; the panels around it hold only what that workspace needs.

- **Setup**: the cell.
  - **Scene** (left): the frame tree — the world, robots and their links, rigs, cameras, lasers,
    lights, targets and parts. **+** adds a frame, rig, camera, laser, target or light, attached
    to the selected item; the bin removes the selected item and everything attached to it.
  - **Viewport** (centre): the cell in 3D, with camera frusta, laser fans, lights and targets.
    Drag to orbit; with the viewport focused, the arrow keys orbit (Shift for larger steps), `+`
    and `−` zoom and `0` resets the view. Click an item to select it.
  - **Inspector** (right): the selected item's id, parent frame and pose (translation in mm,
    roll/pitch/yaw in degrees), and its parameters — a camera's resolution, intrinsics and
    distortion, a laser's fan, reach, wavelength and waist, a target's layout, a light's power
    and colour, a robot's rest pose. All of them are editable.
- **Motion**: the robot program.
  - **Program** (right): the scenario's steps. Click a capture to move the playhead to it.
  - **Joints** (below the viewport): the joint positions over the scenario, with the playhead.
- **Images**: what every camera sees at the playhead, side by side, with the targets projected
  through each camera's model. Hover over a camera and click its expand button to show it alone.
  The 3D view stays in a corner; hide it to see the whole grid.
- **Dataset** (desktop): generate a dataset; see below.

The **playback** bar along the bottom plays, pauses, scrubs and loops the scenario at a chosen
speed. Captures are marked on the timeline.

## Generating a dataset (desktop)

The **Dataset** workspace runs the native pipeline on the current scene and scenario, as edited.
The scene goes into the output folder as `scene.json`, with the scenario:

| Setting | Meaning |
|---|---|
| Output folder | Where the dataset goes. For a built-in example, or a scene that is not a file, it defaults to `Documents/etendue/<name>`; for your own scene, `dataset/` next to it. |
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

Afterwards each camera in **Images** can show the **dataset image** at a capture instead of the
live view.
A new run starts from an empty dataset: the previous run's images and detections are removed.

The files written are described in [Datasets](datasets.md).
