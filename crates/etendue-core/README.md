# etendue-core

The optics kernel of [etendue](https://github.com/VitalyVorobyev/etendue): concrete `f64`
geometry and physics for laser-triangulation and camera design.

- Thick-lens imaging and the circle of confusion, including the Scheimpflug plane of best focus.
- Line lasers: the fan plane, Gaussian beam width, the stripe on a target and its projection.
- Defocus maps and the in-focus, in-view working volume; N-view voxel overlap for symmetric rigs.
- A component bank schema (sensors, lenses, lasers).
- The Scheimpflug design solver (sensor tilt and focus for a working geometry) behind the
  `solver` feature.

Camera models come from calibration-rs (`vision-calibration-core`). Documentation:
<https://vitalyvorobyev.github.io/etendue>.
