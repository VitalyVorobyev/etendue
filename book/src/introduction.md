# etendue

etendue designs robot–camera–laser calibration cells and generates synthetic calibration
datasets with exact ground truth.

A calibration cell is a robot, one or more camera rigs, optional line lasers and lights, and a
calibration board. You describe it in a **scene**, and the robot's moves and captures in a
**scenario**. etendue then:

1. **bakes** the scenario with the robot's kinematics: every frame's pose at every sample;
2. computes the **ground truth**: where every board corner and laser-stripe point lands in every
   camera at every capture, with visibility, through the full camera model (distortion,
   Scheimpflug tilt);
3. **renders** each capture with Blender (Cycles), through an ideal pinhole camera that etendue
   then resamples onto the real camera model, optionally through a sensor model;
4. **detects** the board in the rendered images and checks every detection against the ground
   truth.

The output is a dataset in [calibration-rs](https://github.com/VitalyVorobyev/calibration-rs)'s
format, plus the ground truth that calibrating it should recover. That makes etendue a test bench
for calibration: how well does a setup, a set of viewpoints, a detector or a solver recover a
cell you know exactly?

## The parts

- **etendue studio**, the desktop app: open a scene, play the scenario in 3D, see what each
  camera sees, and generate a dataset. See [The studio](studio.md).
- **`etendue`**, the command line: the same steps, for scripts and batch runs. See
  [Command line](cli.md).
- **Documents**: scenes, scenarios and robot manifests are versioned JSON with published JSON
  Schemas. See [Scenes and scenarios](scenes.md).
- **Libraries**: the Rust crates the tools are built on (`etendue-scene`, `etendue-kinematics`,
  `etendue-synth`, `etendue-core`) and the `@etendue/wasm` npm package. Their API documentation is
  at [`/api`](https://vitalyvorobyev.github.io/etendue/api/).

The last part of this book, **Optics**, describes the optics kernel (`etendue-core`): the thick
lens and defocus model, the Scheimpflug plane of best focus, the laser line, the working volume,
and the design solvers built on them.
