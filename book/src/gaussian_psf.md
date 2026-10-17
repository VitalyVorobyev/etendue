# Gaussian PSF

`etendue-core::optics::psf` converts the geometric circle-of-confusion (CoC)
diameter into an equivalent Gaussian point-spread-function sigma, so that a
simulated image can use a smooth blur kernel that matches photon-counting
intuition rather than the sharp-edged geometric disk.

## From CoC to Gaussian sigma

The standard practice in computational imaging is to match the **FWHM** of
the Gaussian to the diameter of the geometric CoC disk:

$$
\text{FWHM} = 2\sqrt{2\ln 2}\;\sigma
\implies
\sigma = \frac{\text{CoC}_{\text{px}}}{2\sqrt{2\ln 2}}.
$$

The constant `GAUSSIAN_FWHM_PER_SIGMA ≈ 2.3548` is this factor. The
conversion is exposed as a free function, re-exported from
`etendue_core::optics`:

```rust
pub const GAUSSIAN_FWHM_PER_SIGMA: f64 = 2.354_820_045_030_949_4; // 2·√(2 ln 2)

pub fn coc_to_gaussian_sigma(coc: f64) -> f64;
// Returns coc / GAUSSIAN_FWHM_PER_SIGMA, or 0.0 for non-positive or non-finite input.
```

The output sigma is in the same units as the input: pass a CoC in pixels
(for example `ProjectedPoint::defocus_px` or `ThickLens::coc_diameter_px`)
to get a sigma in pixels.

## Using it

To draw a laser stripe with a defocus halo, convert the per-sample defocus
CoC into a sigma and take a 2σ halo radius, which contains about 95 % of the
energy:

```rust
let sigma = coc_to_gaussian_sigma(p.defocus_px);
let halo = 2.0 * sigma;   // 2σ ≈ 95 % of energy
```

and add the halo to the geometric stripe half-width:

$$
\text{half-width}_{\text{total}} = \tfrac{1}{2}\,w_{\text{geom}}
  + 2\sigma_{\text{blur}},
$$

where `w_geom` is the physical line width from `GaussianBeamWidth` (in
pixels, `ProjectedPoint::geom_px`) and the defocus sigma comes from the CoC
at that sample depth. This linear sum is a conservative envelope for drawing
the band. The line-width figure that combines the two independent blur
sources in quadrature is `ProjectedPoint::total_px =
sqrt(geom_px² + defocus_px²)`, computed by `project_stripe` (see
[Laser Line Projection](laser.md)).

## Scope

The Gaussian PSF is a **geometric approximation**: the sigma is derived from
the CoC disk diameter by FWHM matching, not from a diffraction integral. It
is accurate in the geometric-optics regime (CoC ≫ diffraction limit) and
gives a qualitatively correct blur progression. A wave-optics diffraction
term (Airy → Gaussian in the scalar-diffraction approximation, scaled by
`λ / NA`) is not modelled.

## Tests

Three tests in `optics::psf::tests`:

- `fwhm_per_sigma_matches_the_textbook_value`: the constant equals
  `2·√(2·ln 2)` to machine precision.
- `coc_to_gaussian_sigma_matches_the_fwhm_definition`: a round trip
  `sigma → FWHM → coc, coc → sigma` closes.
- `coc_to_gaussian_sigma_rejects_bad_inputs`: zero and negative CoC give
  `0.0` (a zero-width delta, not a negative width).
