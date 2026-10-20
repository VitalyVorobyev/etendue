# Rendering

`etendue render`, and **Generate** in the studio's Calibrate workspace, render every capture with
[Blender](https://www.blender.org)'s Cycles path tracer. Blender only ever renders an ideal
pinhole camera; etendue maps that render onto the real camera model itself. Rendering needs
Blender 5.2 ([Installation](installation.md#blender)); everything else works without it.

The model behind each step is in [Synthetic images](models/synthetic_images.md).

## How an image is made

1. **The render camera.** For each camera, etendue picks a pinhole that sees every ray of the
   real camera, with a small margin, and whose focal length is `supersample × max(fx, fy)`.
2. **The render job.** The robot's link meshes, the scene's parts, the targets (as printed
   geometry, not a texture), the lights and the render cameras go to Blender with the pose of
   every frame at every capture (`job.json`). Blender renders each capture to a multilayer EXR in
   `exr/<camera>/`: linear, with depth and object-index passes (and normal and albedo passes when
   a laser is imaged). The render seed is fixed.
3. **The remap.** For every pixel of the real camera, etendue computes where its ray lands in the
   render, through the full camera model (distortion, Scheimpflug tilt), and resamples the EXR
   there, averaging over the pixel's footprint. Distortion is applied here, exactly, with the
   same model as the ground truth.
4. **The sensor.** Without `--sensor`, the linear image is scaled by `--exposure` and written as an
   8-bit sRGB PNG. With a sensor model, it becomes a raw mono image (8- or 16-bit PNG) with
   noise and quantisation.

A camera pixel's centre is at integer coordinates: pixel `i` spans `[i − ½, i + ½]`. This is the
convention of calibration-rs and chess-corners, and the ground truth uses it too.

The rendered images have no defocus blur and no lens vignetting: the render is as sharp as an
ideal pinhole. Blur is modelled analytically in the Optics and Rigs workspaces
([Defocus and the blur budget](models/blur.md)).

## Choosing the render settings

| Goal | Settings |
|---|---|
| A quick look | the defaults: `--supersample 1 --samples 64` |
| Corner-accurate images for calibration | `--supersample 4 --samples 16 --sensor <file>` |
| Images that are bit-identical between runs | add `--cpu` |

- **Supersampling** renders `s × s` render pixels per camera pixel at the image centre. At 4,
  detected corner positions stop changing with supersampling; below it they carry rendering
  error. The render time grows with `s²`.
- **Samples** are per render pixel, so at `--supersample 4`, 16 samples is 256 per camera pixel.
- **Linear images for calibration.** Detectors are more accurate on linear intensities than on
  sRGB-encoded ones: use a sensor model, even a noise-free one, for images meant for calibration.
- **`--cpu`** gives bit-identical images between runs; GPU renders can differ in the last bits.
- **`--ambient`** lights the scene with a uniform white environment of that radiance (1 by
  default). Set it to 0 to light the scene with its own lights only.
- **`--camera`** renders only the cameras you name.

In the studio the same settings are **Samples**, **Supersampling**, **Sensor model**,
**Cameras** and **Render on the CPU** ([The studio](studio.md#generate-a-dataset)).

## Sensor model

A sensor model is a JSON file. It models a monochrome sensor the way the EMVA 1288 standard
describes one: radiance → photo-electrons (with pixel-to-pixel gain variation) → shot, dark and
read noise → full-well clipping → conversion gain and black level → quantisation.

| Field | Default | Meaning |
|---|---|---|
| `electrons_per_unit` | — | Photo-electrons per unit of linear scene radiance at the chosen exposure: exposure time × aperture × quantum efficiency, folded into one number. |
| `full_well_electrons` | — | Full-well capacity; brighter pixels clip. |
| `gain_dn_per_electron` | — | Conversion gain, digital numbers (DN) per electron. |
| `bits` | — | ADC bit depth, 1–16. Up to 8 bits gives an 8-bit PNG; deeper, a 16-bit PNG with the values left-aligned. |
| `read_noise_electrons` | 0 | Read noise (standard deviation). |
| `dark_electrons` | 0 | Mean dark signal per exposure. |
| `prnu` | 0 | Photo-response non-uniformity: the relative standard deviation of each pixel's gain. |
| `black_level_dn` | 0 | Offset added before quantisation. |
| `rgb_weights` | `[0.2126, 0.7152, 0.0722]` | The mono sensor's response to linear red, green and blue (Rec. 709 luminance by default). |
| `seed` | 0 | Seed of the fixed pixel-gain pattern. Temporal noise also mixes in the frame index, so renders are reproducible. |

Shot noise follows from the electron count. `examples/closed_loop_ur5e/sensor_linear.json` is a
linear 8-bit sensor whose electron counts are so high that it is effectively noise-free:

```json
{"electrons_per_unit": 1e9, "full_well_electrons": 2e9, "gain_dn_per_electron": 2.4e-7, "bits": 8}
```

### Choosing the values

Start from the sensor's datasheet (the library's sensor items keep the full well, read noise and
quantum efficiency when known):

1. **`full_well_electrons`**, **`read_noise_electrons`** and **`bits`**: from the datasheet.
2. **`gain_dn_per_electron`**: to put the full well at the top of the ADC range, take
   `(2^bits − 1) / full_well_electrons`. A higher gain clips before the full well, as a camera's
   analogue gain does.
3. **`electrons_per_unit`** sets the exposure. With the default `--ambient 1` and no other
   lights, a light target square (albedo 0.85) that sees the whole environment renders at a
   linear value of about 0.85. To put it at a fraction `f` of the full well, take
   `electrons_per_unit ≈ f × full_well_electrons / 0.85`. Around `f = 0.7` leaves headroom for
   brighter spots; a lower `f` gives darker, noisier images.
4. **`prnu`**, **`dark_electrons`** and **`black_level_dn`**: from the datasheet if you have
   them; a PRNU of about 0.01 (1 %) is typical.

For a sensor like the Sony IMX174 (32 000 e⁻ full well, 6 e⁻ read noise, 12-bit):

```json
{
  "electrons_per_unit": 26000,
  "full_well_electrons": 32000,
  "gain_dn_per_electron": 0.128,
  "bits": 12,
  "read_noise_electrons": 6,
  "prnu": 0.01,
  "seed": 1
}
```

The light squares then collect about 22 000 electrons, a shot-noise signal-to-noise ratio of about
150. The datasheet values in the examples' library are illustrative: check them before relying on
the noise level.

## The Blender version

Renders are validated with the Blender version pinned in `etendue.toml` (5.2: a pin `X.Y` accepts
every `X.Y.Z`, a pin `X.Y.Z` only that version). etendue reads the nearest `etendue.toml` with a
pin above the scene, then above the working directory, and refuses another version unless
`--allow-blender-version` is given; with no pin found it renders and warns. See
[Troubleshooting](troubleshooting.md#blender).

## Laser images

A camera that images a laser also gets `images/<camera>/<capture>_laser.png`. In a rig with one
laser per camera, each camera images its own laser; a laser that names no camera is imaged by
every camera. The board image stays free of the laser.

The laser image is the same capture with the scene's light dimmed to `--laser-ambient` (default
0.1), plus the stripe. The stripe is not path-traced: etendue composites it from the render's
depth, normal and albedo:
- Lambertian reflection of a line laser whose power (`power_w`, default 0.5 mW) is spread evenly
  over the fan;
- a Gaussian beam profile across the fan, as wide as the beam is where it hits;
- shadows, from depth views rendered at the laser.

The power is as the camera responds to it: it counts the same in every colour channel. The stripe
is as sharp as the pinhole render (no defocus) and has no speckle.
