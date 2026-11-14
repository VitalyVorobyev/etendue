# P20c gate P1: the planner on real poses

**Gate P1:** the P18 planner's system σ, predicted for a real calibration's own pose set, is within
25 % of a split-half estimate of the real calibration's scatter, on `kuka_1` and `DS8` (`DS8`
judged with the rig refined, as T1 was; the default workflow recorded beside it).

*Judged again in P20c2 with the noise model learnt from the real run: PASS on both (0.81, 0.90),
`p20c2_noise_model.md`. This file is P20c's measurement, as it was.*

**Verdict: FAIL on both, in opposite directions.** planner / split-half = **0.52** on `kuka_1`
(the planner is too optimistic) and **2.10** on `DS8` (too pessimistic). The bar is not re-set.
On the twins' own white observations the same estimator reads 1.00 (`kuka_1`) and 1.18 (`DS8`)
times the planner's σ, so neither failure is the estimator's.

What it implies:
- **The planner's information model is right.** On both datasets its system σ times √(view
  scatter) equals calibration-rs's covariance of the real calibration taken through the same
  sightings, to 0.3 %; per parameter group the ratio is exactly √(view scatter) on `kuka_1` and
  within 0.94–1.19 of it on `DS8` (whose real estimate is not the twin's truth for the rig
  refined). The planner and calibration-rs agree on the information; they differ in
  calibration-rs's view-scatter scaling.
- **`kuka_1`: the planner's σ is a lower bound under view-correlated error, by 1.9×.** The halves
  disagree 1.9 times more than white noise predicts. calibration-rs's view scatter (2.79) accounts
  for most of it: planner × √2.79 = 0.77 mm against the split-half 0.89 mm (0.87, within 15 %).
- **`DS8`: two errors that pull apart.** Its intrinsics scatter about 5× their planner σ (as T2's
  view scatter, 25, says), but its system σ is dominated by the robot pose prior (4.24 mm; 0.17 mm
  with exact poses), and the real halves place the cell 2.1× more consistently than random robot
  pose errors of the size calibration-rs estimates (2.05 mm, 1.65e-3 rad per view) would allow.
  The robot error calibration-rs estimates from its pose corrections is not independent per view
  (the corrections also absorb each view's residual pattern, T2's within-view structure 13–21),
  so the planner's prior overstates what the halves can disagree by. calibration-rs's
  view-scaled σ (21–26 mm) overstates it 10–13×.
- So neither the planner's σ nor calibration-rs's view-scaled σ predicts the real system scatter
  within 25 % on `DS8`; on `kuka_1` the view-scaled one does (13 %).

## When and where

- Measured 2026-11-13, Apple M4 Pro, rustc 1.95.0, release builds.
- etendue commit `2cc1bcc`, branch `p20c/planner-real`.
- calibration-rs `v0.18.0`, a clean clone of the tag.

## Method

`tests/p1.rs`, run with
`ETENDUE_P1_SEEDS=10 cargo test --release -p etendue-cli --test p1 -- --ignored --nocapture`
(`etendue real split-half <run> -o <file>` is the same check from the command line). For each
dataset it calibrates the real dataset (`real calibrate`), builds its twin (P20b), and:

1. **Planner σ.** `etendue_synth::plan::predict` on the twin: the P18 information model (each
   camera's intrinsics and `k1, k2, p1, p2`, the rig with every camera but the first refined, the
   hand-eye, the target) over the twin's views (the real robot poses, and in each view the points
   the real detector found), at the real run's reprojection σ. calibration-rs refines each view's
   robot pose under a prior (estimated: `robot_pose_sigma`), so the planner does too: each view's
   pose correction `base_se3_tcp · Exp(δ)` is a parameter under that prior, marginalised (Schur
   complement) out of the view's information. The P18 planner as it was (exact poses) is given
   beside it. The **system σ** is P18's: how far, across the line of sight (mm per image axis),
   the calibrated cell places a point it knows in the base frame, averaged over every sighting of
   every real view.
2. **Real σ.** calibration-rs's covariance of a calibration of every real view (etendue's trial
   loop on the real detections, with the procedure of step 3), which calibration-rs scales by its
   view scatter: per parameter its σ, and for the system its full covariance taken through the
   same sightings as the planner's (`tr(Σ A) / n`).
3. **Split-half σ.** The real views are split into halves ten ways: even against odd views, the
   first ⌈n/2⌉ against the rest in capture order, and 8 seeded random halves (SplitMix64
   Fisher–Yates, seeds 0–7). Each half is calibrated on its own with the planner's model (for a
   rig, one `rig_handeye` stage with `refine_rig`; for one camera, its own workflow; robot poses
   refined, calibration-rs's defaults otherwise). For split `s`:
   - `d_s` is the difference of the halves' estimates: per parameter, `θ_A − θ_B` (the hand-eye
     in calibration-rs's tangent); for the system, `system_difference`, the RMS over every
     sighting of every view of the distance between where the two halves' cells place the point
     (mm per image axis, the truth's depth over its `fx`);
   - `p_s = √(σ_A² + σ_B²)` is its predicted std, from the planner's σ of each half. With equal
     halves `σ_A ≈ σ_B ≈ √2 σ`, so `d_s` is compared with `√2 σ_half ≈ 2σ`.

   Pooled: `ρ = √(Σ_s d_s² / Σ_s p_s²)`, and the split-half σ of the whole set is `ρ σ`. The splits
   share the data, so they are not independent: pooling steadies the estimate against one split's
   luck, not against the one noise draw.
4. **The estimator on the twin.** Steps 1–3 on the twin's own observations (white pixel noise at
   the real run's σ, robot poses perturbed at its estimated error), 10 draws (seeds 200–209):
   where the planner's model holds, what the split-half σ reads, and how much it varies.

P1: `|planner / split-half − 1| ≤ 0.25`. The datasets are public; this file gives aggregate
numbers only, as for private data.

## Data

As in `p20a_r1.md`: `kuka_1` (one camera, 17 × 28 chessboard, 30 views, eye-in-hand), and `DS8`
with its image globs adapted to the local PNG copy (three cameras, 10 × 14 chessboard, 33 of 42
views used, eye-in-hand). Planner noise: `kuka_1` 0.1080 px, robot 7.59e-4 rad and 0.447 mm;
`DS8` 0.1847 px, robot 1.65e-3 rad and 2.05 mm.

## System σ

| Dataset | Procedure | Planner (mm) | Planner, exact robot (mm) | Real (mm) | Split-half (mm) | Planner / split-half | P1 |
|---|---|---|---|---|---|---|---|
| `kuka_1` | its own workflow | 0.461 | 0.0032 | 0.771 | 0.889 | **0.52** | FAIL |
| `DS8` | rig refined | 4.24 | 0.171 | 25.9 | 2.02 | **2.10** | FAIL |
| `DS8` | default (rig held), not gated | 4.24 | 0.171 | 21.3 | 2.01 | 2.10 | — |

**Diagnostic** (not the gate): the planner's σ inflated by calibration-rs's view scatter `v`
(T2's per-view factor), `σ √v`:

| Dataset | Procedure | View scatter | Planner × √v (mm) | Split-half / (planner × √v) |
|---|---|---|---|---|
| `kuka_1` | its own workflow | 2.79 | 0.770 | 1.15 |
| `DS8` | rig refined | 37.3 | 25.9 | 0.08 |
| `DS8` | default | 25.2 | 21.3 | 0.09 |

Per split (measured / predicted halves' difference): `kuka_1` 1.19 (interleaved), 1.74 (halves in
capture order), 1.52–3.14 (random); `DS8` rig refined 0.38, 0.35, 0.35–0.66 (default within 0.01
of each).

**The estimator on the twin** (10 draws; split-half σ over the planner's, RMS and range;
calibration-rs's σ of the twin's calibration of every view over the planner's, mean):

| Dataset | Split-half / planner | Range | calibration-rs / planner | The real data's split-half / planner |
|---|---|---|---|---|
| `kuka_1` | 1.00 | 0.59–1.25 | 1.01 | 1.93 |
| `DS8` | 1.18 | 0.74–1.42 | 1.04 | 0.48 |

The real `kuka_1` is above every twin draw, the real `DS8` below every one.

## Parameters

RMS over each group's members (every camera's together): σ where the members share a unit, and
each member's ratio to the planner's σ.

| Dataset | Group | Unit | Planner σ | Real σ | Split-half σ | Real / planner | Split-half / planner |
|---|---|---|---|---|---|---|---|
| `kuka_1` | focal length (fx, fy) | px | 0.293 | 0.488 | 0.552 | 1.67 | 1.89 |
| `kuka_1` | principal point (cx, cy) | px | 0.217 | 0.362 | 0.396 | 1.67 | 1.77 |
| `kuka_1` | distortion (k1, k2, p1, p2) | | — | — | — | 1.67 | 2.13 |
| `kuka_1` | hand-eye rotation | deg | 0.0168 | 0.0281 | 0.0295 | 1.67 | 1.62 |
| `kuka_1` | hand-eye translation | mm | 0.339 | 0.566 | 0.562 | 1.67 | 1.66 |
| `DS8` rig refined | focal length (fx, fy) | px | 1.13 | 7.99 | 5.70 | 7.10 | 5.44 |
| `DS8` rig refined | principal point (cx, cy) | px | 0.391 | 2.85 | 1.84 | 7.29 | 5.02 |
| `DS8` rig refined | distortion (k1, k2, p1, p2) | | — | — | — | 7.27 | 4.79 |
| `DS8` rig refined | hand-eye rotation | deg | 0.0838 | 0.516 | 0.0944 | 6.17 | 1.21 |
| `DS8` rig refined | hand-eye translation | mm | 4.38 | 27.4 | 8.87 | 6.29 | 2.28 |
| `DS8` default | focal length (fx, fy) | px | 1.13 | 6.24 | 5.73 | 5.41 | 5.46 |
| `DS8` default | principal point (cx, cy) | px | 0.391 | 1.91 | 1.85 | 4.74 | 5.03 |
| `DS8` default | distortion (k1, k2, p1, p2) | | — | — | — | 5.50 | 4.79 |
| `DS8` default | hand-eye rotation | deg | 0.0838 | 0.422 | 0.0953 | 5.05 | 1.21 |
| `DS8` default | hand-eye translation | mm | 4.38 | 22.5 | 8.93 | 5.16 | 2.29 |

A parameter's split-half σ pools one difference per split, so it is rougher than the system's
(on the `closed_loop_ur5e` twin in CI, single parameters read 0.3–1.6 times the planner's).

## Reading it

- **The information model holds.** Real σ / planner σ is √(view scatter) for every group on
  `kuka_1` (1.67 = √2.79) and within 0.94–1.19 of √(view scatter) on `DS8`, and calibration-rs's
  σ on the twins is the planner's within 1–4 %. The planner reproduces calibration-rs's covariance; P1 tests whether that covariance
  describes the real scatter.
- **`kuka_1`.** Every group scatters 1.6–2.1 times its planner σ, as calibration-rs's view
  scatter (1.67) nearly predicts: the residual field T2 found (not white across the image) is
  view-dependent and does not average out. The planner's σ, which assumes white noise, is a lower
  bound by 1.9× on the system σ; its view-scaled value is within 15 %. The halves in capture
  order disagree most (1.74 against 1.19 interleaved): some of the error drifts over the session.
- **`DS8`.** The intrinsics scatter about 5× their planner σ (fx, cx, distortion: 4.8–5.5), as
  T2's view scatter (√25 = 5.0) says, and the default workflow's real σ matches that scatter.
  The hand-eye does not: rotation 1.2×, translation 2.3× (calibration-rs's view-scaled σ
  5–6×). The system σ, which is mostly the robot pose prior's (exact poses give 0.17 mm of its
  4.24 mm), is half the planner's: the robot pose corrections calibration-rs estimates at 2 mm
  per view do not behave like independent errors between halves. Since the hand-eye translation
  does differ by 2.3× its σ while the system difference is half its prediction, the halves'
  differences likely lie along directions the system σ (across the line of sight) hardly sees,
  such as focal length against depth.
- **For the planner.** On a white-noise cell (the twins, G18) its σ holds. On a real cell it is a
  lower bound wherever the residuals are view-correlated (`kuka_1`, and `DS8`'s intrinsics), and
  it is only as good as its robot pose prior where the robot dominates (`DS8`'s system σ). A
  planner that ranks pose sets is unaffected by a common factor; one that promises a σ is not
  reliable on real cells within 25 %.
