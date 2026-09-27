# P4-5 — render determinism (Blender / Cycles)

- Ticket: **P4-5** (`docs/pivot/PLAN.md`). No numeric gate. Criterion: render the same job
  twice and report the maximum absolute difference for Metal and for CPU; GT-critical
  renders use whichever device is deterministic, or the variance is documented with its
  bound.
- Result: **CPU is bit-exact; Metal differs by at most 2.4e-7 in linear radiance.**
  Policy: GPU (Metal) is the default, with this bound documented; `--cpu` gives bit-exact
  renders. **Never mix devices within one dataset**: GPU and CPU sample differently.
- Measured: 2026-09-27, on etendue commit `71d340f` (branch `pivot/p4-blender`), Blender 5.1.1,
  Apple M4 Pro.

## Procedure

```bash
cargo run --release -p etendue-cli -- measure p4-5 -o target/p4_5 --samples 64 --supersample 1
```

`examples/eye_in_hand_ur5e`, first capture, `cam_left`, canonical 1320 × 1056, 64 samples,
seed 0, no denoising, ambient 1. The job is rendered twice on the GPU (Metal) and twice on the
CPU; the Combined pass (linear float32 RGB, 4 181 760 values) is compared.

## Result

```text
  Gpu run 0: 1.9 s      Gpu run 1: 1.8 s      Cpu run 0: 6.3 s      Cpu run 1: 6.3 s
  mean radiance 0.4659
  GPU run 0 vs run 1   max |Δ| 2.384e-7   mean |Δ| 4.487e-9   differing values 541580 / 4181760
  CPU run 0 vs run 1   max |Δ| 0.000e0    mean |Δ| 0.000e0    differing values 0 / 4181760
  GPU vs CPU (run 0)   max |Δ| 2.169e-2   mean |Δ| 7.884e-7   differing values 2502353 / 4181760
```

## Findings

1. **Metal is not bit-deterministic, but the bound is tiny.** 13 % of values differ between
   two identical GPU renders, by at most 2.4e-7 — a few float32 ULPs at the image's radiance
   level, about 1/16 000 of an 8-bit quantisation step. This is floating-point accumulation
   order on the GPU, not sampling noise. It is invisible after the sensor model (P4-6) and
   far below every geometric gate.
2. **The CPU is bit-exact** run to run, at 3.4× the time on this machine.
3. **GPU and CPU are different sample streams**: the same seed gives per-pixel differences up
   to 2.2e-2 (mean 7.9e-7). A dataset rendered partly on each device would carry
   device-dependent noise; render a dataset on one device (the job records it).
