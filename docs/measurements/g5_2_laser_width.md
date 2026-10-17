# G5.2: laser stripe width

**Gate (redefined 2026-10-17, the user's decision):** in rendered laser images, the stripe's
fitted width is within 10 % of the beam's width (`etendue-core` `GaussianBeamWidth`) over the
scene's depth range. The check is per laser image, on the median over its stripe pixels.

The original G5.2 (P5-2) compared a Cycles slit light with "beam ⊕ CoC". P7-7 composites the
laser in Rust from Blender's depth, normal and colour passes instead (ADR 0005). The scenes'
cameras have no aperture and the renders have no defocus, so the CoC term is dropped until they
do.

Measured 2026-10-17 on etendue `8013182` plus the G5.2 change (branch `p7/authoring`), Apple
M4 Pro. **The renders used Blender 5.2.2 (`--allow-blender-version`); the pin is 5.1.1.**

## Procedure

```bash
cargo run --release -p etendue-cli -- gt examples/rig3_laser_ur5e/scene.json <scenario.json> -o target/rig3_img
cargo run --release -p etendue-cli -- render examples/rig3_laser_ur5e/scene.json <scenario.json> -o target/rig3_img \
  --supersample 4 --samples 16 --sensor examples/closed_loop_ur5e/sensor_linear.json --allow-blender-version
cargo run --release -p etendue-cli -- detect target/rig3_img     # the width/beam column; laser_features.json
```

- **Data:** the image-level laser loop's renders (`g5_3_laser_closed_loop.md`): the
  three-camera ring of `examples/rig3_laser_ur5e`, 44 generated captures at two sphere radii.
  That makes 132 laser images, 1280 × 1024, rendered at 4× supersampling and box-filtered per
  pixel.
- **Ground truth:** the images were rendered before `gt.json` carried beam widths. Its
  regeneration (version 3) is identical to the rendered one apart from the new `width_m` and the
  version.
- **Laser:** 0.5 mm waist radius at the laser, 660 nm, Rayleigh range 1.19 m.

**Measured width.** Each column (or row) across the stripe is fitted with a Gaussian, the
weighted log-parabola that also gives the centre. Its σ is turned to σ across the stripe through
the stripe's local slope, a line through the centres within 8 lines.

**Beam width.** At each extracted pixel, the nearest analytic stripe point within 3 px gives:
- the beam's 1/e² diameter there (`gt.json` `width_m`);
- the surface's tangent plane.

The beam alone gives `σ = (diameter / 4) / |∇d|`, where `∇d` is the image gradient of the
distance from the laser plane of what each pixel sees. The gradient comes from
`backproject_pixel` (calibration-rs) and the tangent plane.

## Result: PASS

Fitted over beam σ, per laser image (the median over its pixels):

| Laser distance | Beam diameter | Images | Pixels | Beam σ (px) | Fitted σ (px) | Width / beam | Gate |
|---|---|---:|---:|---|---|---|---|
| 0.414–0.506 m | 1.059–1.087 mm | 60 | 27 638 | 1.29–1.43 | 1.32–1.45 | 1.021–1.026 | ≤ 1.10 |
| 0.513–0.604 m | 1.089–1.121 mm | 72 | 25 769 | 1.10–1.20 | 1.14–1.23 | 1.029–1.035 | ≤ 1.10 |

The three cameras agree to within 0.002. Per pixel, 90 % of the ratios lie within 1.008–1.062.

**What the excess is.** A camera pixel integrates over its area, which adds 1/12 px² to the
stripe's variance. Divided by `√(1 + 1/(12 σ_beam²))`, every image's ratio is 0.9991–1.0018,
the median 1.0004. Nothing else widens or narrows the stripe at the 0.2 % level: the remap, the
sensor model's noise and quantisation, and the fit together stay within it.

## Width against depth

The Rayleigh range of this scene's laser is 1.19 m, so over 0.41–0.60 m its beam widens by only
6 %. The image level confirms the width there but tests the depth law weakly.

The compositor's unit test `the_stripe_is_as_wide_as_the_beam` (`etendue-synth::laser_render`)
tests the law directly:
- the laser is 0.05–1.0 m above a plane, with a 0.2 mm waist (Rayleigh range 0.19 m), from
  inside its Rayleigh range to 5 times beyond it;
- every image row's second moment matches the beam's σ to within 0.1 %.

## Not covered

- **Defocus:** the renders have none (pinhole cameras without an aperture). The CoC term returns
  to the gate when they do.
- **The PuzzlePole:** the check runs on its tangent planes, but no pole laser images were
  rendered for this measurement.
