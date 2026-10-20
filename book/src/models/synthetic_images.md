# Synthetic images

Analytic ground truth tests a calibration on perfect observations. Real calibrations start from
images, and the image step (rendering, the sensor, the corner detector) adds errors of its own.
Synthetic images with exact ground truth measure that step: a corner found in a rendered image can
be compared with the pixel it must be at. For that comparison to mean anything, the images must
follow the camera model exactly, down to a hundredth of a pixel. This page explains how etendue
gets there, and what it does not model.

## Render a pinhole, then remap

No renderer can express calibration-rs's camera model: Blender's cameras have no distortion
stage, no skew and no tilted sensor. etendue therefore renders every view with a **canonical
camera**, a plain pinhole, and bends the result into the real camera afterwards:

- **The canonical camera** has square pixels, no skew, no distortion and its principal point at
  the image centre. Its field of view is the symmetric one that contains every ray of the target
  camera's image (scanned every 8 pixels, the border included), widened by 2 %. Its focal length
  is $s \cdot \max(f_x, f_y)$, with $s$ the supersampling (`--supersample`).
- **The remap table** gives, for every pixel $\mathbf{u}$ of the target camera, where to read the
  canonical image:

$$
\mathbf{u}_{\text{render}} = \text{canonical.project}\big(\text{target.backproject}(\mathbf{u})\big).
$$

Both calls are calibration-rs's own, so the table inherits the target model exactly: every
distortion model, skew, principal point and Scheimpflug tilt. Each output pixel averages the
canonical image over its own footprint with a $\lceil s\rceil \times \lceil s\rceil$ box filter,
the footprint taken from the table's finite differences.

Over 10 000 random pixels of each supported camera model, projecting a back-projected pixel
returns to within $10^{-12}$ px; stored as 32-bit floats the table is exact to $1.3\times10^{-4}$ px.

## Where a pixel is

Which coordinate names the centre of pixel $i$, $i$ or $i + \tfrac12$, is a convention that must
agree between the ground truth, the renderer and the detector, or every corner is off by half a
pixel. etendue uses the **integer** convention: pixel $i$ is centred at coordinate $i$, so an
image $W$ pixels wide spans $[-\tfrac12, W - \tfrac12]$. This is the convention of OpenCV and of
the chess-corners detector, and `gt.json` records it (`pixel_centre`).

It was checked by rendering small glowing spheres at known points and measuring the brightness
centroid of each image: the centroids are within 0.003 px of the analytic projection in Blender
and 0.005 px in the studio's renderer, with a mean bias below 0.001 px. There is no half-pixel
slip anywhere in the pipeline.

## The sensor model

Without a sensor model, `etendue render` writes sRGB images scaled by `--exposure`. With
`--sensor`, each image goes through a linear camera sensor after EMVA 1288, per pixel:

1. linear scene radiance → a mono response (`rgb_weights`, Rec. 709 luminance by default);
2. × `electrons_per_unit` (exposure time, aperture and quantum efficiency in one number) × a fixed
   per-pixel gain (photo-response non-uniformity, `prnu`) + `dark_electrons`;
3. Poisson shot noise on that mean, plus Gaussian read noise (`read_noise_electrons`);
4. clipped to `full_well_electrons`;
5. × the conversion gain `gain_dn_per_electron` + `black_level_dn`, rounded and clipped to `bits`.

The images are raw mono PNGs (8-bit, or 16-bit for deeper sensors). Every random draw is seeded:
the gain pattern by the sensor's `seed`, temporal noise by the seed and the image's index, so a
dataset re-renders bit for bit. The photon-transfer method (variance against mean over flat
fields) recovers the configured conversion gain to 0.4 %.

## How accurate are detected corners

On noise-free renders of a chessboard (five poses, 315 corners), against the analytic ground
truth:

- **Rendering converges** at $s \ge 4$: beyond it, the error stops changing.
- **What remains is the detector's.** chess-corners' Radon detector is 0.032 px RMS from the
  truth at $s = 4$ on linear images; its ChESS refiners are 0.07–0.19 px. On an exact,
  renderer-free image of the same board they are about as far off (0.046 px for Radon), so the
  renderer adds nothing measurable.
- In the full image loop (24 views of two cameras), the detector finds every view, labels all
  2592 corners correctly, and lands 0.058 px RMS from the truth. The calibration that follows is
  about as good as one from the exact corners with white noise of that size, so the detector adds noise,
  not bias (see [Ground truth and closed-loop calibration](closed_loop.md)).

Laser images are made differently, from the render's depth, normals and colours; see [Laser line
and stripe](../laser.md#laser-images).

## Assumptions and limits

- **Pinhole sharpness.** The renders have no defocus, no diffraction and no motion blur: the
  only blur is the pixel's own area. Compare with [Defocus and the blur budget](blur.md), which
  is a design tool, not part of the image model.
- **No vignetting, no chromatic aberration, no rolling shutter.**
- **The scene is what Blender renders**: path-traced with a fixed seed and lit by the scene's
  lights and a uniform environment (`--ambient`). On the CPU (`--cpu`) a render repeats bit for
  bit; on the GPU, repeated renders differ by about $10^{-8}$ of the radiance on average (a few
  pixels by up to 1.8 DN of 240 at the closed loop's settings). Do not mix devices in one dataset.
- **A mono sensor.** Colour is folded into one response; there is no Bayer pattern or
  demosaicking.

API: `etendue-synth` `remap` (`CanonicalCamera`, `remap_lut`), `sensor` (`SensorModel`); `etendue
render`; see [Rendering](../rendering.md) for the commands.
