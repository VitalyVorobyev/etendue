# P20c2: the planner's noise model from real data, gate P1 judged again

**Gate P1** (unchanged from P20c, `p20c_planner.md`): the planner's system σ, predicted for a real
calibration's own pose set, is within 25 % of a split-half estimate of the real calibration's
scatter, on `kuka_1` and `DS8` (`DS8` with the rig refined). Same splits (interleaved, halves in
capture order, 8 seeded random), same datasets, same estimator.

**Verdict: P1 is retired as a gate and kept as a characterisation** (user decision, 2026-11-14).
`kuka_1` agrees (0.81). `DS8` is **unresolved**: it reads 0.90 or 0.63 depending on which
calibration's view scatter the robot correction takes (see **Sensitivity**). On the twin, the
split-half ratio also moves by 0.2–0.3 from one noise draw to the next. With 30–42 views, a
split-half estimate cannot resolve a 25 % bar, so a pass or a fail here is not evidence either way.

| Dataset | Before (P20c's model) | After (learnt) | Reading |
|---|---|---|---|
| `kuka_1` | 0.519 | **0.806** | agrees |
| `DS8`, rig refined | 2.102 | 0.899 (run's `k`), 0.626 (rig-refined `k`) | unresolved |
| `DS8`, default workflow | 2.10 | 0.900 | — |

**What carries forward:**
- The planner's per-view term (each view's information divided by the view scatter `k`). It is off
  by default.
- The finding that calibration-rs's robot pose σ absorbs view-correlated image error. On `DS8` it
  reports 2.05 mm, which is implausible for a Denso arm; with the correlated error taken out it is
  at the 1 µm floor. The fix goes upstream.

## When and where

- Measured 2026-11-14, Apple M4 Pro, rustc 1.95.0, release builds.
- etendue commit `991edda`, branch `p20c2/noise-model`.
- calibration-rs `v0.18.0`, a clean clone of the tag. The draft calibration-rs change below was
  measured on its own build (the same change applied to `v0.18.0`).

## The model

`etendue_synth::plan::NoiseModel` is what `predict` assumes: white pixel noise σ, the robot prior
the pose corrections are weighed with, and a per-view term `k` (the view scatter). Each view's
information, its pixels and its robot prior together, is divided by `k`, so with exact robot poses
every σ grows by `√k`. `k = 1` is the default: every synthetic prediction, G18 and the CI twin test
are bit for bit as before, and `plan` (`scenario plan`) does not take the term at all.

`NoiseModel::learnt` takes three things from a real run (`etendue real calibrate`'s, the run the
twin is built from): its pixel σ, its robot pose error (calibration-rs's `robot_pose_sigma`), and
its view scatter. The view scatter becomes `k`. The robot error is calibration-rs's estimate with
the view-correlated pixel error taken out (below).

### The per-view term: the view scatter

calibration-rs's view scatter is a delete-one-view jackknife of the shared parameters against
their white-noise covariance, computed from the run's own residuals. calibration-rs scales its
covariance by it, and on both datasets the planner reproduces that scaled covariance (P20c). It
costs nothing (the run reports it) and it does not use the split-half data.

The alternative, a per-view random pose offset fitted to the residuals, is not a separate term for
the planner: a per-view offset of the board's pose lies in the span of the per-view robot pose
corrections, so the calibration absorbs it exactly as it absorbs robot error, and its effect is that
of a larger robot σ. On `kuka_1` (one camera, intrinsics-heavy, system σ dominated by the robot
prior) the two compare as follows. Ratios are split-half σ over planner σ per group, so 1 is right:

| Model (`kuka_1`) | Planner / split-half | fx, fy | cx, cy | k1, k2, p1, p2 | Hand-eye rotation | Hand-eye translation |
|---|---|---|---|---|---|---|
| P20c (white views) | 0.52 | 1.89 | 1.77 | 2.13 | 1.62 | 1.66 |
| Per-view pose jitter (robot σ × 2) | 1.02 | 1.74 | 1.74 | 2.05 | 0.85 | 0.96 |
| View scatter on the pixels only | 0.53 | 1.24 | 1.08 | 1.33 | 1.48 | 1.40 |
| View scatter (pixels and robot prior) | 0.87 | 1.13 | 1.06 | 1.27 | 0.97 | 1.00 |

The pose jitter matches the system σ only by making the robot error twice calibration-rs's
estimate, and it leaves the intrinsics 1.7–2× too optimistic: it moves the hand-eye, not the
pattern within views. The view scatter brings every group within 1.0–1.3. So the per-view term is
the view scatter, applied to the whole view.

### The robot pose σ

**etendue reads calibration-rs's estimate correctly.** `robot_pose_sigma` is in radians and metres,
a σ (not a variance) per gripper axis, for the correction `base_se3_gripper · Exp(δ)` on the right,
as `apply_robot_delta` applies it. The planner's prior is the same: information `1/σ²` on each of
`δ`'s six coordinates, the pose perturbed on the right. The halves of every split estimate the same
size (`DS8` 1.1–2.2e-3 rad and 1.6–2.4 mm per half, `kuka_1` 0.67–0.88e-3 rad and 0.17–0.57 mm),
so the estimate is stable. It is the meaning of the estimate that does not carry over.

**What calibration-rs v0.18.0 computes.** `vision-calibration-optim`,
`src/problems/robot_poses.rs:209–265` (`estimated_priors`) estimates the robot's error from the pose
corrections by variance-component estimation. Per group, `σ̂² = Σ δ̂² / Σ r`, with the redundancy
`r = 1 − Var(δ̂)/σ²` (line 251) taken from the covariance with the view scatter divided out (line
219: "the redundancy is the noise model's"). That is unbiased when the residuals are white. Each
correction also carries the part of its view's pixel error that looks like a pose,
`(Σ − Σ P Σ)ₖₖ` for white residuals (`P` the priors' information). When the residuals are
correlated within views (view scatter `k > 1`), that part is `k` times larger, and the estimate
counts the excess `(k − 1)(Σ − Σ P Σ)ₖₖ` as robot error. T2 found `DS8`'s residuals strongly
structured within views (13–21 against the twin's 0.8–0.9), so its 2 mm is mostly pixel error the
corrections absorbed.

**What the data support.** `Prediction::robot_prior_under_view_scatter(k)` solves calibration-rs's
fixed point for the robot's own error once the `k`-fold pixel part is taken out, from the planner's
per-view information on the corrections. At `k = 1` it returns the estimate itself.

| Dataset | calibration-rs v0.18.0 | etendue, corrected at the run's `k` | Draft calibration-rs fix |
|---|---|---|---|
| `kuka_1` (`k` 2.79) | 7.59e-4 rad, 0.447 mm | 7.50e-4 rad, 0.397 mm | 8.45e-4 rad, 0.329 mm (`k` 3.06) |
| `DS8` (`k` 25.2) | 1.65e-3 rad, 2.05 mm | 7.75e-4 rad, 1 µm (the floor) | 1.16e-3 rad, 1 µm (`k` 28.4) |

`DS8`'s corrections hold no translation error beyond the pixel part, and about 0.04–0.07° of
rotation. That is plausible for the arm (a Denso VS-6577; the poses are a programmed grid of mostly
pure translations), and it is what the split-half data need. Scanning the robot σ with everything
else fixed, the planner matches the split-half at about half calibration-rs's estimate with white
views (× 0.5: 1.07) and at about 0.3 of it with the view scatter on the pixels (× 0.3: 0.93);
`kuka_1` needs no reduction. These scans only locate the answer; the learnt model is not fitted to
the split-half.

**The draft fix** (calibration-rs, not opened): the same correction inside the estimate (`(k − 1)`
times each correction's pixel part taken out of `δ̂²`), on a branch from `main` in a separate clone.
calibration-rs's own tests pass. Its synthetic study (a per-view focal jitter on the `kuka_1`-like
scene) shows the limit: a focal jitter is a per-view depth error, which no estimate can tell from
the robot's, and its view scatter (2.9) understates how much the pose directions are inflated, so
the estimate barely moves (0.759 → 0.745 mm for a simulated 0.2 mm). The correction holds where the
view scatter describes the pose directions too, which is calibration-rs's own assumption when it
scales its covariance by one factor.

## System σ, before and after

On the same calibrations (the splits calibrated once, judged against each model):

| Dataset | Model | View scatter | Robot (rad, m) | Planner (mm) | Split-half (mm) | Planner / split-half |
|---|---|---|---|---|---|---|
| `kuka_1` | P20c (white views, the run's robot error) | 1 | 7.59e-4, 4.47e-4 | 0.461 | 0.889 | 0.519 |
| `kuka_1` | per-view term only | 2.79 | 7.59e-4, 4.47e-4 | 0.770 | 0.889 | 0.867 |
| `kuka_1` | robot correction only | 1 | 7.50e-4, 3.97e-4 | 0.430 | 0.890 | 0.483 |
| `kuka_1` | **P20c2 (learnt)** | 2.79 | 7.50e-4, 3.97e-4 | **0.718** | 0.890 | **0.806** |
| `DS8` | P20c (white views, the run's robot error) | 1 | 1.65e-3, 2.05e-3 | 4.236 | 2.015 | 2.102 |
| `DS8` | per-view term only | 25.2 | 1.65e-3, 2.05e-3 | 21.28 | 2.015 | 10.56 |
| `DS8` | robot correction only | 1 | 7.75e-4, 1e-6 | 0.366 | 2.046 | 0.179 |
| `DS8` | **P20c2 (learnt)** | 25.2 | 7.75e-4, 1e-6 | **1.840** | 2.046 | **0.899** |

Neither part passes alone on `DS8`: the per-view term without the correction scales a robot prior
that is mostly pixel error by `√k`, and the correction without the per-view term leaves the pixel
error out altogether. On `kuka_1` the per-view term does the work; the correction is small.

The split-half σ moves slightly with the model (2.015 → 2.046 mm on `DS8`) because it is the
measured differences over the predicted ones, times the planner's σ, and the predicted halves'
σ change shape, not only scale.

## Where the halves differ

The system difference the halves would have if they differed in one block of the cell only
(`tests/p1/blocks.rs`), RMS over the splits, against the planner's prediction from that block's
covariance alone (mm). The blocks need not add up to the whole: where they compensate, the whole is
smaller.

| Dataset | | Intrinsics | Rig | Hand-eye rotation | Hand-eye translation | All |
|---|---|---|---|---|---|---|
| `kuka_1` | measured | 0.568 | — | 1.820 | 1.111 | 2.004 |
| `kuka_1` | P20c | 0.308 | — | 0.942 | 0.780 | 1.040 |
| `kuka_1` | P20c2 | 0.513 | — | 1.492 | 1.188 | 1.616 |
| `DS8` | measured | 8.18 | 7.42 | 6.56 | 8.89 | 4.40 |
| `DS8` | P20c | 1.59 | 1.20 | 5.71 | 10.02 | 9.26 |
| `DS8` | P20c2 | 7.18 | 5.98 | 5.23 | 6.24 | 3.96 |

P20c's model got `DS8` wrong in both directions at once: the intrinsics and rig blocks 5–6× too
small (the view-correlated error), the hand-eye and the whole 2× too large (the robot prior).
The learnt model has every block within 0.7–0.9 of the measured, and the whole within 10 %.

## Sensitivity

- **Which `k`.** The correction takes `k` from the same calibration whose robot estimate it
  corrects: the real run (calibration-rs's default workflow; `DS8` 25.2). The rig-refined
  calibration of the splits estimates the same robot error but a larger view scatter (37.3: more
  shared parameters, the rig's among them, scatter more), and with it the correction takes out all
  of `DS8`'s robot error (to the floors); P1 then reads **0.626**. The correction rests on the
  view scatter, one number for all shared parameters, describing the pose directions too.
- **The estimator on the twin** (white observations at the real run's noise, where P20c's model
  holds, 10 draws, unchanged from P20c): planner / split-half `kuka_1` mean 1.07 (std 0.27,
  0.80–1.69), `DS8` mean 0.89 (std 0.20, 0.70–1.35). One noise draw moves the ratio by about
  0.2–0.3: 0.81 and 0.90 are within the estimator's own scatter of 1, and the 0.63 of the other
  `k` is within it of the bar. P1 on one real noise draw cannot separate them more finely.

## Reading it

- On these two cells, the planner's σ is right within 25 % once its noise model is the real run's:
  the view scatter as a per-view term, and the robot error without the view-correlated pixel error.
  Both come from the real run's own residuals and estimates, not from the split-half.
- The per-view term is what calibration-rs already does to its covariance; the planner now does the
  same. The robot correction is where etendue departs from calibration-rs's number, and it is the
  more fragile part (one number for every direction, see **Sensitivity**).
- For a synthetic cell, or `scenario plan`, nothing changes: the views are white by construction.
