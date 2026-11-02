# G16.1, G16.2: calibration procedures

Gates of P16 (`docs/pivot/PLAN.md`, ADR 0011).

**Setup**
- Measured 2026-11-02, Apple M4 Pro.
- etendue commit `1e010f4` (branch `p16a/procedures`).
- calibration-rs `v0.11.0` (`c1880f64`), a clean clone of the tag.

**Datasets**
- The examples' ground truth (`etendue gt`).
- Two one-camera heads made from `laser_ur5e` and `laser_scheimpflug_ur5e`: `cam_left` alone, with the laser re-parented to it.
  - `head` has an untilted sensor.
  - `head_s` has the Scheimpflug tilt (0.05, −0.01) rad.
  - The scenario is `laser_ur5e`'s. No example covers a one-camera head.

## G16.1: the default reproduces the previous reports bit for bit: PASS

**Procedure**
1. Before the change, `etendue calibrate` (the same commit's parent, `31c8fa8`) wrote a report for each case.
2. After the change, the same commands ran with no `--procedure`, which means `auto`.
3. Each pair of JSON reports was compared after dropping only the new `version`, `procedure` and `stages` keys.

| Case | Datasets | Result |
|---|---|---|
| Exact observations | the 8 datasets | identical |
| 0.5 px noise, seed 3, 2 trials | the 8 datasets | identical |
| `--camera cam_right` | closed_loop_ur5e, exact and noisy | identical |

**18 of 18 identical.** The `auto` procedures that ran were:
- `auto_rig` (closed_loop, eye_in_hand, eye_to_hand);
- `auto_rig_laser` (laser_ur5e, rig3_laser_ur5e);
- `auto_rig_laser_scheimpflug` (laser_scheimpflug_ur5e);
- `auto_head` (head, head_s);
- `auto_single` (`--camera`).

## G16.2: each built-in recovers the truth on exact data: PASS

| Built-in | Stages | Example | Check | Worst fx (rel) | Worst hand-eye (°) | Worst laser normal (°) |
|---|---|---|---|---|---|---|
| `intrinsics` | 1 (per camera) | closed_loop_ur5e | PASS | 2.0e-8 | — | — |
| `stereo` | 2 | closed_loop_ur5e | PASS | 1.6e-7 | — | — |
| `handeye` | 2 (per camera) | eye_in_hand_ur5e | PASS | 6.0e-9 | 6.6e-5 | — |
| `handeye` | 2 (per camera) | eye_to_hand_ur5e | PASS | 1.5e-7 | 2.0e-4 | — |
| `rig_handeye` | 3 | closed_loop_ur5e | PASS | 8.5e-8 | 1.5e-5 | — |
| `rig_handeye` | 3 | eye_to_hand_ur5e | PASS | 1.2e-7 | 2.1e-4 | — |
| `laser_head` | 2 | head | PASS | 9.9e-9 | 8.6e-6 | 1.5e-6 |
| `laser_rig` | 1 | laser_ur5e | PASS | 6.3e-8 | 6.2e-5 | 6.2e-5 |
| `laser_rig` | 1 | laser_scheimpflug_ur5e | PASS | 1.8e-8 | 1.1e-7 | 1.3e-7 |
| `laser_rig` | 1 | rig3_laser_ur5e | PASS | 1.3e-8 | 5.2e-6 | 2.2e-5 |

## Characterisation: one Scheimpflug head (`head_s`)

This head is not an example cell, and no built-in passes on it. Its tilt about y (−0.57°) is not observable from these 24 views.

| Procedure | hand-eye (°) | tilt_y error (°) | laser normal (°) | cx (px) |
|---|---|---|---|---|
| `auto` (`auto_head`: device, then a pinhole hand-eye) | 2.95 | 0.57 | 0.58 | 23 |
| `laser_head` (the hand-eye with the device's Scheimpflug model) | 0.59 | 0.57 | 0.58 | 23 |
| `laser_head` with `cx`, `cy` fixed (a procedure file) | 0.006 | 0.42 | 0.020 | 0 (held at the truth) |

In every case the fit is near-perfect (mean reprojection 0.0003–0.0004 px) while the tilt and the principal point trade off. A staged procedure fixes the hand-eye's model error. The pose set itself has to fix the observability, which is what P18's planner is for.

## Procedure

```bash
etendue gt <scene> <scenario> -o <dataset>                         # each dataset
etendue calibrate <dataset> -o base/<n>.exact.json                  # before (parent commit)
etendue calibrate <dataset> --noise noise.json --trials 2 -o base/<n>.noise.json
etendue calibrate <dataset> -o new/<n>.exact.json                   # after: the same, no --procedure
python3 compare.py base new                                         # drop version/procedure/stages, compare
etendue calibrate <dataset> --procedure <built-in> -o p.json        # G16.2
```
