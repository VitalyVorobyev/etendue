# Rendering

`etendue render` (and the studio's Dataset tab) renders every capture with
[Blender](https://www.blender.org)'s Cycles path tracer. Blender only ever renders an ideal
pinhole camera; etendue maps that render onto the real camera model itself.

## How an image is made

1. **The canonical camera.** For each camera, etendue picks a pinhole that sees every ray of the
   real camera, with a small margin, and whose focal length is `supersample × max(fx, fy)`.
2. **The render job.** The robot's link meshes, the scene's parts, the boards (as the printed
   squares, not a texture), the lights and the canonical cameras go to Blender with the pose of
   every frame at every capture (`job.json`). Blender renders each capture to a multilayer EXR in
   `exr/<camera>/`, linear, with depth and object-index passes. The render seed is fixed.
3. **The remap.** etendue tabulates, for every pixel of the real camera, where its ray lands in the
   canonical image (a lookup table through the full camera model: distortion, Scheimpflug tilt),
   and resamples the EXR through it, averaging over each pixel's footprint. This is where
   distortion comes from, so it is exact — the same model the ground truth and calibration-rs
   use.
4. **The sensor.** Without `--sensor`, the linear image is scaled by `--exposure` and written as an
   8-bit sRGB PNG. With a sensor model, it becomes a raw mono image (8- or 16-bit PNG).

A camera pixel's centre is at integer coordinates (pixel `i` spans `[i − ½, i + ½]`), the
convention of calibration-rs and chess-corners; the ground truth uses the same one.

## Choosing settings

- **`--supersample 4`** renders 4 × 4 canonical pixels per camera pixel at the image centre. At 4,
  detected corner positions stop changing with supersampling; below it they carry rendering
  error.
- **`--samples`** is per canonical pixel, so at `--supersample 4`, 16 samples is 256 per camera
  pixel.
- **Linear images for calibration.** Detectors are more accurate on linear intensities than on
  sRGB-encoded ones; use a sensor model (even a noise-free one) for images meant for calibration.
- **`--cpu`** gives bit-identical images between runs; GPU renders can differ in the last bits.
- **`--ambient`** lights the scene with a uniform white environment (1 by default). Set it to 0 to
  light the scene with its own lights only.

## Sensor model

A sensor model is a JSON file:

| Field | Meaning |
|---|---|
| `electrons_per_unit` | Photo-electrons per unit of linear scene radiance at the chosen exposure (exposure time × aperture × quantum efficiency). |
| `full_well_electrons` | Full-well capacity; brighter pixels clip. |
| `gain_dn_per_electron` | Conversion gain, digital numbers per electron. |
| `bits` | ADC bit depth, 1–16. |
| `rgb_weights` | Spectral response of the mono sensor to linear RGB (default: Rec. 709 luminance). |
| `read_noise_electrons`, `dark_electrons` | Read noise (standard deviation) and mean dark current per exposure. |
| `prnu` | Photo-response non-uniformity: relative standard deviation of per-pixel gain. |
| `black_level_dn` | Offset added before quantisation. |
| `seed` | Seed of the fixed pattern; temporal noise also mixes in the frame index, so renders are reproducible. |

Shot noise follows from the electron count. `examples/closed_loop_ur5e/sensor_linear.json` is a
noise-free linear 8-bit sensor.

## The Blender version

Renders are validated with the Blender version pinned in `etendue.toml` (5.1.1). etendue reads the
nearest `etendue.toml` with a pin above the scene, then above the working directory, and refuses
another version unless `--allow-blender-version` is given; with no pin found it renders and
warns. Lasers are not rendered yet: a laser scene's images show the
board without the stripe.
