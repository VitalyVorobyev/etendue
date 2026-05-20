# Roadmap

The MVP (M0–M6) is done. What follows is the post-MVP queue, in priority
order. Each item is additive — it extends rather than replaces the MVP.

## 1. Promote `ThickLens` to an upstream `ApertureModel<S>` trait in calibration-rs

The architectural rule from the handoff: any addition to calibration-rs is
**upstreamed**, never duplicated or vendored. `etendue-core::optics::ThickLens`
is designed for this promotion — its `coc_diameter(&Point3<f64>)` signature
is the prototype for an `ApertureModel<S>` trait method, and "3D point in,
CoC out" is the right shape under Scheimpflug tilt (a scalar
`(depth, off_axis)` cannot reconstruct the lateral coordinate the tilted
PoBF needs).

The promotion is **gated on the model stabilising**. We want the same API
in production for a few real designs before freezing the upstream trait
signature.

## 2. Full voxelized working-volume analysis

The MVP's working volume is a 2D region on the laser fan plane (see [the
working-volume chapter](working_volume.md)). The full analysis is a 3D
voxel grid with per-voxel:

- visibility from each of N cameras,
- illumination by each of N lasers,
- triangulation angle between camera pairs / camera–laser pairs,
- projected pixel resolution.

This is the headless precondition for the multi-camera overlap analysis
below. Implementation will likely pull in `rayon` (per-voxel work is
embarrassingly parallel) and `criterion` (the inner loop becomes
performance-relevant for the first time).

## 3. Multi-camera overlap; triangulation-resolution maps

Once the voxel analysis is in place: N-view overlap (which voxels are seen
by ≥ 2 cameras, by ≥ 3, ...) and per-voxel triangulation resolution
(`dZ per pixel error`) along the laser plane and for stereo correspondences.
The `Scene` already holds `Vec<CameraEntity>`, so the data path is in
place.

## 4. Component-picker UI + importers

The bank schema, loader, and seed files are already in M3 (see [the
component-bank chapter](bank.md)). What is missing is the picker UI —
catalog browsing, filtering by `LensMount` / wavelength / pixel-pitch
range, and drag-onto-scene to populate a `CameraEntity` / `LaserEntity` /
`TargetEntity`. The lens-spec and laser-spec fields map directly onto
entity fields, so the picker is plumbing rather than physics.

Importers for vendor formats (Sony / FLIR sensor specs, Edmund / Thorlabs
lens datasheets) come on top of the same loader.

## 5. `argmin`-based optimization over a `DesignParams` struct

The handoff calls this out as deferred from the MVP but designed-in. Given
a target working distance and depth range, find the focal length, aperture,
Scheimpflug tilt, and baseline that maximise a weighted combination of
working-volume area, depth-of-focus coverage, and triangulation resolution.
`argmin` is the obvious black-box optimizer. The objective evaluations
reuse the same headless analysis functions the UI calls per frame.

## 6. Physics refinements

In priority order within the bucket:

- **Gaussian-PSF defocus upgrade.** Replace the geometric CoC by a
  depth-dependent Gaussian PSF (with a smooth diffraction term at the
  diffraction limit).
- **Laser intensity / along-fan width function.** The MVP `WidthModel` is
  across-stripe only. Adding the along-fan intensity profile (Gaussian /
  top-hat) lights up power-density analysis and lets the simulated-image
  panel modulate brightness along the line.
- **Sphere / cylinder / step target primitives.** Parametric primitives
  beyond the planar rectangle — extending the MVP target type without
  forcing every consumer to handle a general mesh.
- **Mesh-stitched laser intersection.** Lift the MVP's plane∩plane
  restriction. A general triangle-mesh intersection (a piecewise-linear 3D
  curve) replaces the single-segment output of `stripe_on_target`; the
  output type already preserves the polyline interface so the projection
  layer is unchanged.

Beyond these six, the explicit out-of-scope items from the handoff
(realistic synthetic rendering, sequential ray-tracing of prescription
surfaces, Seidel/Zernike aberrations, polarization, coherent diffraction,
speckle, wavelength-dependent effects) are deliberately out of bounds.
etendue's scope is the **design questions** of [the
introduction](introduction.md), not Zemax-style optical design.
