# ADR 0004: Canonical Render Camera and Remap LUT

- Status: Accepted
- Date: 2026-09-26 (accepted 2026-09-26)

## Context

Synthetic images must follow the calibration-rs camera model exactly. The
calibration-rs pipeline is `pixel = K(sensor(distortion(projection(dir))))`,
with these stages (verified in `vision-calibration-core` 0.8.1,
`models/camera.rs`):

- **Projection:** `Pinhole` only.
- **Distortion:** `None`, `BrownConrady5`, `Rational`, `ThinPrism`, and
  `Division`. The distortion stage runs *before* the sensor homography.
- **Sensor:** `Identity`, `Homography`, and `Scheimpflug`. `Scheimpflug`
  compiles to a homography.
- **Intrinsics:** `FxFyCxCySkew`.

None of the renderers can express this chain natively. three.js offers a
pinhole with an off-centre projection matrix. Cycles offers a pinhole with
`shift_x/y` and no skew, no distortion stage, and no tilted sensor.
Re-implementing the chain in GLSL and in Blender Python would duplicate the
camera-model math in two more languages, and the copies would drift from
calibration-rs.

## Decision

### Every backend renders only a canonical pinhole

The canonical pinhole has:

- square pixels, no skew;
- the principal point at the image centre (pixel-centre convention: see
  *Pending* below);
- a supersample factor `s` relative to the target camera;
- a field of view chosen by `etendue-synth` to cover the whole target-camera
  image, plus a margin, after unprojection.

The canonical camera is itself expressed as a calibration-rs `CameraParams`
(`Pinhole`, no distortion, `Identity` sensor, `FxFyCxCySkew` with `skew = 0`),
so that projecting through it is also calibration-rs code.

### The target model is applied afterwards as a remap LUT

For each output pixel `u_out` of the target camera, calibration-rs's own stage
inverses are chained. The stage traits live in `vision-calibration-core`
`models`:

```text
s      = K⁻¹(u_out)          IntrinsicsModel::pixel_to_sensor
n_d    = sensor⁻¹(s)         SensorModel::sensor_to_normalized   (homography inverse; covers Scheimpflug geometry)
n_u    = undistort(n_d)      DistortionModel::undistort          (iterative for Brown/Rational/ThinPrism via `iters`; closed-form for Division)
u_rend = K_r(n_u)            canonical camera, via the same chain
```

- The LUT stores `u_rend` for every output pixel as two float32 values
  (RG32F), in canonical-image pixels.
- At 4096 px × `s = 8` the float32 spacing is ~0.004 canonical px, which is
  ~0.0005 output px. That is below every later gate.
- **The same LUT serves both backends.**
  - Web `SensorView` renders the canonical pinhole into a render target and
    resamples it in a fragment shader.
  - Blender renders the canonical pinhole to EXR, and `etendue-synth` resamples
    it in Rust.
- Blender needs only `lens` (mm) and `sensor_width` (mm), with
  `sensor_fit = HORIZONTAL` and no `shift_x/y`, because the principal point is
  centred.
- The chain covers every distortion model, skew, arbitrary principal points,
  and Scheimpflug **geometry**.

### Gate G3.1 validates the LUT

- On full images of every supported model, including Scheimpflug tilts up to
  6°, `project(unproject(u))` must hold to ≤ 1e-6 px at random pixels.
- P3-1 reports the `undistort` iterations needed at image corners.
- If the default 8 iterations (BrownConrady5) are not enough, the fix is an
  upstream calibration-rs issue, not a local override.

### Known limitation: Scheimpflug defocus

This is documented and not fixed. Cycles depth of field assumes a focal plane
parallel to the sensor, so Scheimpflug **defocus** is wrong in photometric
renders.

- For untilted cameras, Cycles DOF uses the camera's physical f-number and
  focus distance. This is a thin-lens approximation that ignores the
  principal-plane gap.
- For tilted cameras, `etendue-core::optics::coc` remains the reference for
  blur.

### Pending (P4-2): pixel-centre convention

It is **not assumed**. Probe P4-2 decides it empirically: emissive spheres at
known 3D points are rendered in both Blender and the web `SensorView`, and the
intensity-weighted centroids are compared with the analytic projection (G4.1,
≤ 0.01 px). The result, the convention and any half-pixel offset, is written
into this section. Until then, no backend may hard-code one.

**Result (P4-2, 2026-09-27, `docs/measurements/g4_1_convention.md`).**

- Both backends are free of half-pixel offsets. Blender and the web
  `SensorView` image each probe sphere within 0.005 px of its projection, and
  the mean bias is ≤ 0.0005 px. This holds under either convention, each read
  out in its own, including for an off-centre, skewed, Scheimpflug-tilted
  camera. A half-pixel slip anywhere would show as a ~0.5 px mean.
- The convention is therefore a **labelling choice** made by etendue-synth,
  never a correction inside a backend. A LUT, its canonical camera, and every
  pixel emitted with it (renders, ground truth) carry one `PixelCentre`.
- **Default: `Integer`.** The centre of pixel `i` is at coordinate `i`. This is
  OpenCV's convention, and it is what `@vitavision/stage2d` and
  `calib-targets-core` `image.rs` state.
- The sibling toolchains disagree with each other:
  - `calib-targets-core`'s README samples pixels at `(x + ½, y + ½)`.
  - calib-targets' puzzleboard synthesiser calls `+½` "workspace-wide".
  - chess-corners' upscaler uses OpenCV's half-pixel mapping.

  P4-3 compares detected corners against ground truth. There, a mismatch
  between the detector's convention and the emitted one shows as a ½ px mean
  offset, and `PixelCentre` is set to whichever convention the detector uses.
- **Resampling is part of the result.** Point-sampling a supersampled canonical
  render aliases: the centroid scatter reached 0.035 px in Blender and 0.019 px
  on the web. Both backends therefore average the canonical render over each
  output pixel's footprint. They use `taps × taps` bilinear samples, with the
  footprint taken from the LUT's own finite differences:
  - Rust: `remap_image_box`
  - web: `SensorView` `taps`

## Consequences

- No camera-model math lives outside calibration-rs. The renderers see only a
  plain pinhole.
- One LUT per camera model and resolution. Memory is W × H × 8 bytes, e.g.
  40 MB for 5 MP. The LUT is computed by `etendue-synth` natively or by
  `@etendue/wasm` on the web.
- Resampling adds a filtering step. Supersampling `s` and the filter choice are
  measured against corner bias in P4-3 (G4.2).
- Photometric Scheimpflug images have correct geometry but incorrect blur.
  Datasets that depend on tilted-sensor blur must use the geometric tier's
  CoC-based blur.

## Alternatives considered

- **Distortion in shaders and in Blender** (custom camera or OSL). Rejected:
  it duplicates the model in GLSL and Python, and it cannot express Scheimpflug
  plus distortion ordering without re-deriving calibration-rs.
- **Blender `shift_x/y` for the principal point.** Rejected as unnecessary,
  since the LUT absorbs it and keeps both backends identical.
- **Render directly at target resolution with no supersampling.** It remains
  an option (`s = 1`), but whether it is adequate is a P4-3 measurement, not an
  assumption.

## References

- calibration-rs ADR 0005 (composable camera), ADR 0022 (Scheimpflug)
- `docs/pivot/PLAN.md` §4 (ADR-0004), P3-1, P4-2, P4-3
- etendue `docs/derivations/scheimpflug_pobf.md` (defocus reference)
