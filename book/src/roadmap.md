# Roadmap

M0–M10 and several post-MVP features are done. The two genuinely open items
are listed below, in priority order.

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
signature. This is a calibration-rs PR requiring explicit user review before
merge.

## 2. Component-picker UI

The bank schema, loader, and seed files are already in place (see [the
component-bank chapter](bank.md)). What is missing is the picker UI —
catalog browsing, filtering by `LensMount` / wavelength / pixel-pitch
range, and drag-onto-scene to populate a `CameraEntity` / `LaserEntity` /
`TargetEntity`. The lens-spec and laser-spec fields map directly onto
entity fields, so the picker is plumbing rather than physics.

---

## Already shipped (not in scope for further work)

| Item | Shipped in |
|---|---|
| N-view voxelized overlap (3D per-voxel visibility + illumination) | M10 |
| Multi-camera overlap visualisation | M10 |
| Symmetric-rig builder (`Scene::triangulation_ring`) | M10 |
| Scheimpflug solver (`argmin`-based min-max CoC) | M9 |
| Gaussian-PSF defocus (`optics::psf`) | post-MVP (commit `40371a6`) |
| Triangle-mesh laser intersection (`laser::stripe_segments_on_mesh`) | post-MVP (commit `40371a6`) |
| Mesh-target entity + UI (`MeshTarget`, F7 panel section) | pre-release (this review) |
| Camera anatomy renderer (M8 — imager, principal planes, aperture ring) | M8 |
