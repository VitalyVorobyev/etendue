# Introduction

**etendue** is a pure-Rust desktop tool for the interactive design of
laser-triangulation 3D-sensor optical systems. It is a parametric design aid
for a single working point in the design space: pick a camera, a laser, and a
target; tweak focal length, f-number, focus distance, Scheimpflug tilt, fan
angle, and beam waist; watch the working volume, the defocus heatmap on the
target, and the projected laser line on the simulated image update in real
time.

## The design questions it answers

A typical question early in a triangulation-sensor project is: *given a real
target standoff and a real measurement region, can a 16 mm f/2.8 lens on this
sensor, this laser, and this baseline cover the volume sharply?* The tool
answers four such questions simultaneously, in one view:

- **Working volume sizing.** Where on the laser fan is the point both
  illuminated by the laser, visible to the camera, and within the camera's
  depth of field? The MVP renders this as a translucent patch on the fan plane
  with an area (m²) and a depth range (m) readout.
- **Depth of focus + Scheimpflug.** How does the geometric circle of confusion
  vary across an obliquely viewed target? The defocus heatmap on the target
  shows it directly; sliding the Scheimpflug tilt rotates the in-focus band.
- **Laser line in pixel space.** What does the projected stripe look like on
  the simulated sensor image? Width is the geometric beam cross-section and
  the defocus blur combined in quadrature.
- **Multi-camera reasoning** (post-MVP scaffolding). The scene already holds
  `Vec<CameraEntity>`, so multi-camera overlap is an additive milestone.

## MVP status (M0–M6)

The MVP is complete. Six milestones from the initial workspace bring-up
through to the working-volume analysis are in: a hand-written winit + wgpu +
egui render loop, a 3D viewport with depth-tested opaque and translucent
passes, the posed scene (camera + laser + target) with JSON save/load, the
thick-lens defocus physics with the on-target heatmap, the laser-line
projection with the 2D simulated-image panel, and the working-volume analysis
on the fan plane. **172 tests** pass; the M4 Scheimpflug kill gate (numerical
agreement with hand-computed circle-of-confusion regimes) is met.

## Scope

The tool is a **parametric model**, not a sequential ray tracer. A camera is a
[vision-calibration-core] `Camera` projection chain plus four thick-lens
scalars (`f`, `N`, `s_o`, `g`). A laser is a posed planar fan with a
Gaussian-beam waist. A target is a finite rectangle. The defocus model is the
geometric circle of confusion derived from the Gaussian conjugate relation
and the Scheimpflug tilt — see [the Scheimpflug chapter](scheimpflug.md). The
laser stripe width is the cross-section diameter combined with the
circle-of-confusion diameter in quadrature.

## Out of scope

- Realistic synthetic rendering: path tracing, BRDFs, materials, full sensor
  noise, speckle.
- Component-level optical design (the Zemax world): per-surface ray tracing,
  aspherics, wavefront analysis, Seidel/Zernike aberrations.
- Polarization. Coherent diffraction. Wavelength dependence beyond a single
  nominal wavelength.
- A database of real lens prescriptions.
- Mesh-stitched laser intersection (the MVP target is a single analytic
  plane, so the stripe is one straight segment).

## Reader assumption

A domain expert. The text assumes a working command of paraxial optics, the
Scheimpflug principle, the pinhole camera model, `nalgebra` poses
(`Isometry3<f64>`), and Rust 2024 idioms. Physics derivations stay tight:
no first-principles re-introduction of the thin-lens equation.

[vision-calibration-core]: https://github.com/VitalyVorobyev/calibration-rs
