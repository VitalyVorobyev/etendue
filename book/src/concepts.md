# Concepts and workflow

This page is the mental model in one place. Each section links to the page that has the details.

## Scene: what is there

A **scene** (`scene.json`) describes a calibration cell as a tree of frames. Every item has an
`id`, a `parent` frame and its pose in that parent:

- **robots**, each from the robot library, with the joint positions it starts at;
- **rigs**, rigid mounts that group cameras and lasers;
- **cameras**, each with its camera model and image size, and optionally its physical optics;
- **lasers**, line lasers that project a flat fan;
- **lights** (point, spot or area);
- **targets**, the calibration boards (chessboard, ChArUco, marker board, PuzzleBoard,
  PuzzlePole) or plain rectangles;
- **parts**, static meshes for context: tables, stands, fixtures;
- **frames**, named helper frames.

A parent is the world, another item (a rig, say), or a robot link such as `ur5e/tool0`. A rig on
the robot's tool flange makes an **eye-in-hand** cell; a board held by the robot and a rig on a
stand make an **eye-to-hand** cell. See [Scenes and scenarios](scenes.md).

## Scenario: what happens

A **scenario** (`scenario.json`) is the robot program: a list of **steps**. A step moves a robot
point to point (to joint positions or to a tool pose), moves it along a straight line, waits, or
**captures**: every camera takes an image. etendue times every move within the robot's velocity,
acceleration and jerk limits, and the robot stops for each capture.

**Baking** a scenario samples the motion at a fixed period and gives the pose of every frame at
every sample. Everything downstream — the 3-D view, the ground truth, the renders — works from
the baked motion. See [Scenes and scenarios](scenes.md#the-scenario) and
[Motion](models/motion.md).

## Projects and the library

A **project** is a folder holding your cells (each a folder with a scene and a scenario), a
**component library** (sensors, lenses, lasers, targets, camera designs, triangulation heads,
rigs), and the datasets and calibration runs made from them. The studio's Optics and Rigs
workspaces work on the library; the Setup and Motion workspaces work on a cell. A scene copies
what it takes from the library, so a cell never depends on the library being present. See
[Projects and the library](library.md).

## Cameras: pinhole and physical

Every camera has a **camera model**: the projection, lens distortion, an optional tilted
(Scheimpflug) sensor, and the intrinsics `fx, fy, cx, cy`. This is the model calibration
estimates. See [Camera models](models/camera_models.md).

A camera can also be **physical**: a sensor (pixel pitch), a thick lens (focal length, f-number,
principal planes, entrance pupil) focused at a distance, and the sensor's tilt. Then:

- the optics are the source of truth, and the camera model's focal lengths are derived from
  them (refocusing the lens changes `fx`: focus breathing);
- the camera's pose is its **body**: the mechanical frame at the image plane, where you would
  bolt it down;
- its rays meet at the **entrance pupil**, which sits some distance in front of the body. A
  calibration recovers the entrance pupil's pose, not the body's.

Only physical cameras have defocus, depth of field and diffraction, which the Optics and Rigs
workspaces use. See [Physical cameras and the thick lens](physical_cameras.md).

## Coordinates in brief

- Units are metres and radians, unless a field's name says otherwise (`square_size_mm`,
  `wavelength_nm`).
- The world is right-handed with +Z up.
- A camera looks along its +Z, with +X to the right of the image and +Y down (the OpenCV
  convention).
- A pose named `a_se3_b` maps coordinates in frame `b` to frame `a`. Every item's pose is
  `parent_se3_self`. On disk a pose is
  `{"rotation": [qx, qy, qz, qw], "translation": [x, y, z]}`.

See [Frames and poses](models/frames.md).

## Ground truth and observations

The **ground truth** is the exact answer, computed analytically from the scene and the baked
motion: every camera's true model and mounting, the true hand-eye transform, the true laser
planes, and, at every capture, the pixel of every target point and laser-stripe point each
camera sees.

**Observations** are what a calibration is given: the target points' pixels in each view, the
laser-stripe pixels, and the robot's reported tool poses. etendue makes them three ways:

- **exact**: the ground truth's own pixels and poses. A correct calibration must recover the
  truth almost perfectly from them;
- **with noise**: the same, with seeded pixel noise, outliers, dropped points and robot pose
  errors, to see how a calibration degrades;
- **detected**: found by etendue's detectors in rendered images, to test the whole image chain.

See [Ground truth and closed-loop calibration](models/closed_loop.md).

## Dataset

A **dataset** is a folder: the ground truth (`gt.json`), a calibration-rs dataset manifest
(`dataset.json`) and the robot's poses (`robot_poses.json`), then optionally the rendered images
and the detections. The kind of calibration problem follows from the scene: intrinsics, rig
extrinsics, hand-eye (eye-in-hand or eye-to-hand), or a laser-line problem. See
[Datasets](datasets.md).

## Run

A **run** is one calibration of a dataset, kept in its own folder: the options it used, the
report (every estimated quantity's error against the truth), and calibration-rs's own result.
Runs with different noise, seeds or observations can then be compared side by side. See
[Calibration](calibration.md).

## The workflow, in the studio

| Step | Workspace | Command line |
|---|---|---|
| Design a head's optics | Optics | — |
| Lay out a rig | Rigs | — |
| Build the cell | Setup | `etendue validate` |
| Program the robot, generate viewpoints | Motion | `etendue scenario sphere`, `etendue scenario from-poses`, `etendue export` |
| Check what the cameras see | Images | — |
| Ground truth, render, detect | Calibrate | `etendue gt`, `etendue render`, `etendue detect` |
| Calibrate and compare | Calibrate | `etendue calibrate` |

The [tutorials](tutorials/first_dataset.md) walk through each step.
