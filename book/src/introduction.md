# etendue

etendue designs camera and laser sensors, lays them out on robots, and generates synthetic
calibration datasets with exact ground truth. It then calibrates those datasets and tells you how
close the calibration came to the truth.

It is for engineers who design laser-triangulation heads, multi-camera rigs and robot calibration
cells, and for the people who calibrate them. With it you can answer questions such as:

- Which lens, f-number, focus and sensor tilt keep a laser line sharp across a measuring window?
- How many cameras does a ring need, and at what tilt, to see a volume from two sides?
- Can the robot reach every viewpoint a calibration needs, without a joint at its limit?
- How well does a calibration recover a cell I know exactly, with this much pixel noise, or from
  rendered images?

![The studio with the hand-eye calibration example at a capture](images/studio.png)

## The workflow

etendue follows a sensor from its optics to its calibration. You can enter at any step.

1. **Design the optics.** Choose a sensor, a lens and a line laser, and set the f-number, the
   focus and the sensor tilt (Scheimpflug) so the laser plane stays sharp over the window you
   must measure.
2. **Lay out a rig.** Repeat a camera or a head as a stereo pair or a ring, and see which points
   are seen by two or more cameras, and which a laser and its camera measure.
3. **Build the cell and program the robot.** Place the robot, the rig, the calibration target and
   any lights in a scene. Write the robot's moves and captures, jog the robot by hand, or let
   etendue generate viewpoints on spheres around the target.
4. **Write the ground truth.** For every capture, etendue computes where every target point and
   laser-stripe point lands in every camera, through the full camera model.
5. **Render.** Blender renders each capture, and etendue maps the render onto the real camera
   model, optionally through a noisy sensor.
6. **Detect.** etendue finds the target and the laser stripes in the rendered images and checks
   every detection against the ground truth.
7. **Calibrate.** etendue calibrates the dataset with
   [calibration-rs](https://github.com/VitalyVorobyev/calibration-rs), from the exact
   observations, from observations with controlled noise, or from the detections, and compares
   every estimated quantity with the truth.

The datasets are in calibration-rs's format, so you can also calibrate them with your own tools.

## The parts

- **etendue studio**, the desktop app. Its workspaces follow the workflow: **Optics** (head
  design), **Rigs** (rig layout and coverage), **Setup** (the cell), **Motion** (the robot
  program, jogging and pose generation), **Images** (what every camera sees) and **Calibrate**
  (datasets and calibration). See [The studio](studio.md).
- **`etendue`**, the command line: validation, baking, ground truth, rendering, detection,
  calibration, motion export and pose generation, for scripts and batch runs. See
  [Command line](cli.md).
- **Documents**: scenes, scenarios, projects, library items and the rest are versioned JSON
  files with published JSON Schemas. See [File formats](formats.md).
- **The library**: a project folder keeps its sensors, lenses, lasers, targets, camera designs,
  heads and rigs, and the studio carries a library of robots. See
  [Projects and the library](library.md) and [Robots](robots.md).
- **Libraries for developers**: the Rust crates the tools are built on, and the `@etendue/wasm`
  npm package that runs the same kernel in a browser. Their API documentation is at
  [`/api`](https://vitalyvorobyev.github.io/etendue/api/).

## This book

- **Getting started**: [Installation](installation.md), then
  [Concepts and workflow](concepts.md), the ideas the rest of the book uses.
- **Tutorials** walk through complete tasks, starting with
  [your first synthetic dataset](tutorials/first_dataset.md).
- **The user guide** describes every tool: the studio, projects, scenes, robots, the command
  line, rendering, datasets, calibration, the examples and the file formats.
- **Models and algorithms** explains the physics and mathematics behind the tools: frames and
  poses, camera models, the thick lens, defocus, the Scheimpflug condition, laser lines, heads,
  coverage, kinematics, motion, pose generation, synthetic images and closed-loop calibration.
- **Reference**: the [Glossary](glossary.md) and [Troubleshooting](troubleshooting.md).
