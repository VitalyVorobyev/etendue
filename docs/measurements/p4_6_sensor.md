# P4-6 — sensor model: photon-transfer curve

- Ticket: **P4-6** (`docs/pivot/PLAN.md`). Criterion: the photon-transfer curve (variance vs
  mean) reproduces the configured gain within **≤ 2 %**.
- Result: **PASS.** K̂ = 0.25107 DN/e⁻ for a configured 0.25 DN/e⁻ (+0.43 %).
- Measured: 2026-09-27, on etendue commit `70f1dd6` (branch `pivot/p4-blender`).

## Model (`etendue-synth::sensor`, EMVA 1288 style)

For each pixel, from linear scene radiance (the renderer's output):

1. radiance → mono response (`rgb_weights`, default Rec. 709 luminance);
2. × `electrons_per_unit` (exposure × aperture × QE) × a fixed PRNU gain (normal, mean 1,
   σ = `prnu`, seeded by `seed`) + `dark_electrons`;
3. Poisson shot noise on that mean, + Gaussian read noise (`read_noise_electrons`);
4. clip to `[0, full_well_electrons]`;
5. × `gain_dn_per_electron` + `black_level_dn`, rounded, clipped to `bits`.

Temporal noise is seeded by `(seed, frame)`; the generator is ChaCha8, stable across
platforms and versions, so frames re-render bit-identically. `etendue render --sensor
sensor.json` applies it per image (frame = image index in the job) and writes raw mono PNGs
(8-bit, or 16-bit left-aligned for deeper sensors).

## Procedure

```bash
cargo test -p etendue-synth --release --test ptc -- --nocapture
```

Sensor: 10 000 e⁻/unit, PRNU 1 %, dark 5 e⁻, read noise 3 e⁻, full well 12 000 e⁻,
K = 0.25 DN/e⁻, black level 64 DN, 12 bit. Flat fields of 256 × 256 px at 12 levels (0 … 66 %
of full well), two frames each. EMVA two-frame method: signal = mean(A + B)/2 − black level,
temporal variance = var(A − B)/2 (PRNU cancels). K̂ is the least-squares slope.

## Result

```text
PTC: K̂ = 0.25107 DN/e (configured 0.25)
  mean      1.28 DN   var    0.854 DN²
  mean    176.25 DN   var   44.425 DN²
  mean    351.25 DN   var   88.814 DN²
  mean    526.30 DN   var  132.055 DN²
  mean    701.19 DN   var  175.326 DN²
  mean    876.17 DN   var  218.480 DN²
  mean   1051.31 DN   var  263.921 DN²
  mean   1226.29 DN   var  306.144 DN²
  mean   1401.28 DN   var  351.088 DN²
  mean   1576.28 DN   var  397.438 DN²
  mean   1751.32 DN   var  440.243 DN²
  mean   1926.30 DN   var  483.634 DN²
```

The dark level's variance (0.854 DN²) matches read noise + dark-current shot noise +
quantisation, K²(σ_read² + dark) + 1/12 = 0.958 DN², within its sampling error. The fitted
intercept is not asserted: it is small against the variances the slope is fitted to.
