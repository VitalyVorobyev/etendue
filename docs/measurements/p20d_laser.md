# P20d: real laser rigs, calibrated with etendue's stripe detector

**Goal:** the private Scheimpflug laser rigs get calibrated laser planes from their real laser
images, with etendue's stripe detector as calibration-rs's `LaserPixelExtractor`
(`detect::laser::StripeExtractor`). No gate (user decision, 2026-11-14: value first, no
fine-tuning). The comparison is with calibration-rs's own run of the same laser stage.

**Verdict: done.** Every view is used, the stripe residual is 0.66–0.85 px RMS, and each plane
agrees with calibration-rs's reference run to 0.16–0.51 mrad and at most 6 µm.

Private data (PLAN §P20): dataset ids and aggregate numbers only. No images, poses, parameters,
plane coefficients or paths.

## When and where

- Measured 2026-11-14, Apple M4 Pro, rustc 1.95.0, release builds.
- etendue commit `3068843`, branch `p20d/laser-rigs`.
- calibration-rs `v0.18.0`, a clean clone of the tag.

## Data

- `scheimpflug_rig` and `rtv3d` are **one dataset**: in the calibration-rs checkout
  `scheimpflug_rig` is a symlink to `rtv3d`'s folder (and `scheimpflug_rig_ref` to `rtv3d_ref`).
  Both ids were run; their results are byte-identical. The numbers below are that one dataset.
- Six 720×540 Scheimpflug tiles in one strip, eye-to-hand, ChArUco, 20 views. The laser stage
  (`dataset_laser.toml`) is `rig_laserline_device`: the planes on a frozen rig hand-eye
  calibration (`upstream_calibration = rig_handeye_export.json`).

### What was adapted (copies in the git-ignored `local/<id>/`; the checkout is untouched)

The dataset's manifests parse with v0.18.0's `DatasetSpec` as they are. Its upstream export does
not: it was written by an older calibration-rs (`cameras[i]` as `{proj, dist, sensor: null, k}`
with the tilts in a separate `sensors`, the hand-eye as `handeye_mode` plus loose poses). A copy
was converted to a v0.18.0 `RigHandeyeExport` value for value: each camera to `CameraParams`
(pinhole, Brown-Conrady 5, Scheimpflug with its `sensors` tilts, `fx fy cx cy`; every skew is 0),
the hand-eye to the tagged `HandeyeSolution`, the rig and target poses as they are; its residual
diagnostics were dropped (the laser stage does not read them). The laser manifest's copy names
the dataset's images and poses by absolute path and the converted export as its upstream.
Nothing else changed.

This upstream is the one calibration-rs's reference laser run froze: the reference export echoes
its cameras, `cam_se3_rig` and `rig_se3_target`, equal to 2e-13. So the two runs differ only in
the extractor.

## Method

```text
etendue real calibrate rtv3d -o local/runs/rtv3d
etendue real calibrate scheimpflug_rig -o local/runs/scheimpflug_rig
```

`real calibrate` runs calibration-rs's `calibrate_dataset` with `StripeExtractor`, at the
`rig_laserline_device` defaults, and keeps the run. The reference is calibration-rs's own
`rig_laserline_export.json` in the dataset (its extractor is vision-metrology's, which is not on
crates.io). A script compares the two exports per camera: views and pixels, stripe residuals, and
the planes' difference: the angle between the normals, and the offset where the reference plane
crosses the camera's optical axis (the stripe's working distance).

## Results

Per camera. Residuals are every stripe pixel's: `px` its distance from the calibrated laser line
in the image (what the solve minimises), `µm` the distance of its back-projected point from the
plane.

| camera | views | stripe pixels (etendue / ref) | RMS px | RMS µm (etendue / ref) | > 3 px | Δ angle (mrad) | Δ offset (µm) |
|---|---|---|---|---|---|---|---|
| cam0 | 20 / 20 | 8098 / 8876 | 0.77 | 80 / 56 | 0.4 % | 0.27 | 5.8 |
| cam1 | 20 / 20 | 8599 / 8694 | 0.67 | 69 / 51 | 0.2 % | 0.16 | 1.3 |
| cam2 | 20 / 20 | 7345 / 8400 | 0.66 | 68 / 46 | 0.1 % | 0.48 | 1.9 |
| cam3 | 20 / 20 | 8009 / 8344 | 0.85 | 88 / 58 | 0.4 % | 0.51 | 0.5 |
| cam4 | 20 / 20 | 7782 / 9169 | 0.80 | 82 / 67 | 0.3 % | 0.20 | 1.2 |
| cam5 | 20 / 20 | 7844 / 9240 | 0.76 | 79 / 61 | 0.2 % | 0.31 | 0.6 |

- **Planes:** etendue's and the reference's agree to 0.16–0.51 mrad and 0.5–5.8 µm. The
  extractors differ; the planes do not, at a level far below the stripe's own scatter.
- **Stripe residual:** etendue's pixels sit 1.2–1.5 times as far from the plane as the
  reference's (68–88 µm against 46–67 µm). Its centres are a Gaussian fit on the raw profile; the
  reference's extractor smooths with a Gaussian derivative (σ 1.2 px) first. etendue keeps 1–15 %
  fewer pixels.
- The reference export's pixel residuals do not read as v0.18.0's (median 4–27 px against a plane
  residual of 46–67 µm): it was written by an older calibration-rs. Stripe residuals are compared
  in µm for that reason.
- calibration-rs's other reference, the vendor's planes in `artifacts.json`, are in its own frames
  with its own camera model. Read in the camera frame they differ from both runs by 27 mrad to over
  a radian, and by the same from each to 0.4 mrad: they say nothing about the extractor and are not
  compared further.

## The extractor on real images

A real laser image is the stripe on the board's dim light (a column median of 3 DN), with no
board image beside it to subtract. The first run, the stripe detector alone along the manifest's
scan axis, found the stripe and also a centre in every column beyond its ends: 7–22 % of the pixels
were more than 3 px from the calibrated line, 86 % of them beyond the stripe's ends, and the
planes still agreed to 0.3–4.1 mrad. Two rules took them out (`detect::laser::chained`):
- **continuity:** centres are chained along the scan lines (a centre continues a chain within
  2 px of its extrapolated line, across gaps of up to 20 lines where the board's ink hides the
  stripe), and only chains of 16 centres or more are kept;
- **width:** a chain more than twice as wide (median σ across) as the longest is not the stripe.
  This drops the defocused bright patches of the board (the Scheimpflug tilt leaves the board's far
  edge out of focus), which chain like a stripe.

After them 0.1–0.4 % of the pixels are over 3 px. The rules apply only to the extractor
(a real image); `etendue detect` on rendered images, which subtracts the board image, is unchanged.

## CI

`tests/laser_extractor.rs` runs the extractor on images drawn from `laser_ur5e`'s analytic
stripes (σ 1.2 px) over a dim checkerboard (3 and 14 DN) with noise: on every view with a stripe,
at most 1 % of the centres are off it and the rest are within 0.2 px RMS, with a centre on at least
90 % of the stripe's lines; a bare board gives no pixels. The private runs are local only.

## Out of scope

The twin's laser (stripes simulated at the real poses, G5.3c on the twin) is P20d2, deferred.
