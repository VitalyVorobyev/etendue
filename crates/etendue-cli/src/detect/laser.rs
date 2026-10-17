//! Laser-stripe extraction (P7-7): the stripe's centre line in every laser
//! image of a dataset, checked against the analytic stripe.
//!
//! A laser image (`<capture>_laser.png`) is the capture's board image dimmed
//! (the room light left on) plus the stripe. The two are differenced:
//! 1. the dimming is estimated as the median ratio of laser to board image
//!    over lit pixels (the stripe covers too few to move it);
//! 2. the dimmed board image is subtracted, which leaves the stripe alone;
//! 3. what is left is divided by the board image, which takes out the
//!    print's albedo: the stripe is as bright on ink as on paper. Pixels too
//!    dark in the board image (ink), or saturated in the laser image, carry
//!    no measurement and are left out;
//! 4. the stripe's direction is the principal axis of what is left;
//! 5. across it, every column (or row) fits a Gaussian to its peak: a
//!    parabola to the logarithm of the samples above 10 % of the peak,
//!    weighted by their squares. A fit, unlike a centroid, finds the centre
//!    of a profile an ink edge cuts in half. A centre is kept only with the
//!    pixels either side of it measured and two samples on each side.
//!
//! Every extracted pixel is then compared with the analytic stripe of
//! `gt.json` (`laser_points`): its distance from that polyline is reported,
//! never used to correct it. So is the fitted width against the beam's
//! (gate G5.2): `gt.json`'s beam diameter at the nearest stripe point, a
//! Gaussian `exp(−2d²/w²)` in the distance `d` from the laser plane, carried
//! into the image through the camera model and the surface's tangent plane.

use std::path::Path;

use anyhow::Result;
use etendue_synth::dataset::{GroundTruth, GtCapture, GtView};
use etendue_synth::images::{Gray8, read_png_luma8};
use nalgebra::{Point2, Vector3};
use serde::{Deserialize, Serialize};
use vision_calibration_core::CameraModel;

/// The `laser_features.json` format version.
pub const LASER_FEATURES_VERSION: u32 = 1;

/// A column (or row) is part of the stripe when its peak is at least this
/// far above the dimmed board image, in DN.
const MIN_PEAK_DN: f64 = 8.0;
/// The fit takes the samples above this fraction of the peak.
const FLOOR: f64 = 0.1;
/// At most this many pixels either side of the peak.
const HALF_WINDOW: usize = 20;
/// Board pixels darker than this (ink) carry no measurement: neither the
/// dimming nor the stripe is taken from them.
const MIN_BOARD_DN: u8 = 16;
/// The stripe's local slope is fitted over this many lines either side.
const SLOPE_LINES: f64 = 8.0;
/// An extracted pixel farther than this from the analytic stripe is an
/// outlier.
pub const OUTLIER_PX: f64 = 3.0;

/// How the extracted stripe compares with the analytic one.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StripeCheck {
    /// Pixels extracted.
    pub points: usize,
    /// Of them, within [`OUTLIER_PX`] of the analytic stripe.
    pub matched: usize,
    /// Of them, farther.
    pub outliers: usize,
    /// RMS distance of the matched pixels from the analytic stripe.
    pub rms_px: f64,
    /// Largest such distance.
    pub max_px: f64,
    /// Mean signed distance across the stripe (`+` toward +Y for a stripe
    /// within 45° of the image's x axis, toward +X otherwise): a bias.
    pub mean_px: f64,
}

/// How the stripe's width compares with the beam's (gate G5.2), over the
/// matched pixels with a beam width at their analytic point.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WidthCheck {
    /// Pixels compared.
    pub points: usize,
    /// Median fitted σ across the stripe, pixels.
    pub sigma_px: f64,
    /// Median σ of the beam alone across the stripe in the image, pixels
    /// (a quarter of its `1/e²` diameter).
    pub beam_sigma_px: f64,
    /// Median of fitted / beam σ per pixel.
    pub ratio: f64,
    /// Its 5th percentile.
    pub ratio_p05: f64,
    /// Its 95th percentile.
    pub ratio_p95: f64,
}

/// The stripe in one laser image.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LaserView {
    /// Camera id.
    pub camera: String,
    /// Laser image path relative to the dataset directory.
    pub image: String,
    /// The stripe's centre line, `[u, v]` in the dataset's pixel convention;
    /// empty if the image is missing or shows no stripe.
    pub pixels: Vec<[f64; 2]>,
    /// The fitted Gaussian's σ across the stripe at each pixel, in pixels.
    pub sigma_px: Vec<f64>,
    /// Against the analytic stripe.
    pub check: StripeCheck,
    /// Width against the beam's; `None` when no pixel has a beam width.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<WidthCheck>,
}

/// A point of a stripe's centre line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Centre {
    /// `[x, y]`, pixel `i` at `i`.
    pub pixel: [f64; 2],
    /// The fitted Gaussian's σ across the stripe (perpendicular to its
    /// local direction), pixels.
    pub sigma_px: f64,
}

/// One capture's laser images.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LaserCapture {
    /// Capture id.
    pub id: String,
    /// Its laser images, in `gt.json`'s view order (views without one are
    /// left out).
    pub views: Vec<LaserView>,
}

/// `laser_features.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LaserFeatures {
    /// [`LASER_FEATURES_VERSION`].
    pub version: u32,
    /// How the stripe was found.
    pub method: String,
    /// Per capture, in `gt.json` order.
    pub captures: Vec<LaserCapture>,
}

/// The fraction of the board image's light in the laser image: the median
/// ratio over pixels lit in both.
#[must_use]
pub fn dimming(laser: &Gray8, board: &Gray8) -> f64 {
    let mut ratios: Vec<f64> = laser
        .data
        .iter()
        .zip(&board.data)
        .filter(|&(_, &b)| b >= MIN_BOARD_DN)
        .map(|(&l, &b)| f64::from(l) / f64::from(b))
        .collect();
    if ratios.is_empty() {
        return 0.0;
    }
    let mid = ratios.len() / 2;
    *ratios.select_nth_unstable_by(mid, f64::total_cmp).1
}

/// Centre and σ of a Gaussian through `(t, value)` samples: the vertex and
/// curvature of the parabola fitted to `ln value`, each sample weighted by
/// `value²`. `None` if the samples do not curve down.
fn gaussian_centre(samples: &[(f64, f64)]) -> Option<(f64, f64)> {
    // Normal equations of a + b·t + c·t² ≈ ln v, about the first sample
    // (conditioning).
    let t0 = samples.first()?.0;
    let mut m = [[0.0_f64; 3]; 3];
    let mut r = [0.0_f64; 3];
    for &(t, v) in samples {
        let (x, w, y) = (t - t0, v * v, v.ln());
        let basis = [1.0, x, x * x];
        for i in 0..3 {
            for j in 0..3 {
                m[i][j] += w * basis[i] * basis[j];
            }
            r[i] += w * basis[i] * y;
        }
    }
    let det = |m: &[[f64; 3]; 3]| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let d = det(&m);
    if d.abs() < 1e-12 {
        return None;
    }
    // Cramer's rule for b and c.
    let with = |k: usize| {
        let mut mk = m;
        for (i, row) in mk.iter_mut().enumerate() {
            row[k] = r[i];
        }
        det(&mk) / d
    };
    let (b, c) = (with(1), with(2));
    (c < 0.0).then(|| (t0 - b / (2.0 * c), (-0.5 / c).sqrt()))
}

/// The stripe's centre line in `laser`, with `board` (the same view without
/// the laser) subtracted at its estimated dimming and divided out.
///
/// # Panics
///
/// If the two images differ in size.
#[must_use]
pub fn extract(laser: &Gray8, board: Option<&Gray8>) -> Vec<Centre> {
    let (w, h) = (laser.width as usize, laser.height as usize);
    // Per pixel: the stripe above the dimmed board (DN), and the same divided
    // by the board's brightness, or `None` where the pixel measures nothing.
    let stripe: Vec<Option<(f64, f64)>> = match board {
        Some(board) => {
            assert_eq!(
                (board.width, board.height),
                (laser.width, laser.height),
                "laser and board image sizes"
            );
            let s = dimming(laser, board);
            laser
                .data
                .iter()
                .zip(&board.data)
                .map(|(&l, &b)| {
                    (b >= MIN_BOARD_DN && l < u8::MAX).then(|| {
                        let r = (f64::from(l) - s * f64::from(b)).max(0.0);
                        (r, r / f64::from(b))
                    })
                })
                .collect()
        }
        None => laser
            .data
            .iter()
            .map(|&l| (l < u8::MAX).then(|| (f64::from(l), f64::from(l))))
            .collect(),
    };
    // The stripe's direction: the principal axis of the bright residual.
    let (mut n, mut mx, mut my) = (0.0, 0.0, 0.0);
    let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
    for (k, v) in stripe.iter().enumerate() {
        if v.is_some_and(|(r, _)| r >= MIN_PEAK_DN) {
            let (x, y) = ((k % w) as f64, (k / w) as f64);
            n += 1.0;
            mx += x;
            my += y;
            sxx += x * x;
            syy += y * y;
            sxy += x * y;
        }
    }
    if n < 2.0 {
        return Vec::new();
    }
    let (mx, my) = (mx / n, my / n);
    let (cxx, cyy, cxy) = (sxx / n - mx * mx, syy / n - my * my, sxy / n - mx * my);
    // Along x when the principal axis is within 45° of x.
    let along_x = (2.0 * cxy).atan2(cxx - cyy).abs() / 2.0 <= std::f64::consts::FRAC_PI_4;
    let (lines, span) = if along_x { (w, h) } else { (h, w) };
    let at = |line: usize, t: usize| {
        if along_x {
            stripe[t * w + line]
        } else {
            stripe[line * w + t]
        }
    };
    // (line, centre, σ along the scan line).
    let mut fits = Vec::new();
    for line in 0..lines {
        // The peak: the brightest normalised sample bright enough in DN.
        let Some((peak_at, peak)) = (0..span)
            .filter_map(|t| {
                at(line, t)
                    .filter(|(r, _)| *r >= MIN_PEAK_DN)
                    .map(|(_, v)| (t, v))
            })
            .fold(None, |best: Option<(usize, f64)>, c| {
                if best.is_none_or(|b| c.1 > b.1) {
                    Some(c)
                } else {
                    best
                }
            })
        else {
            continue;
        };
        let lo = peak_at.saturating_sub(HALF_WINDOW);
        let hi = (peak_at + HALF_WINDOW + 1).min(span);
        let samples: Vec<(f64, f64)> = (lo..hi)
            .filter_map(|t| at(line, t).map(|(_, v)| (t as f64, v)))
            .filter(|&(_, v)| v > FLOOR * peak)
            .collect();
        if samples.len() < 3 {
            continue;
        }
        let Some((centre, sigma)) = gaussian_centre(&samples) else {
            continue;
        };
        // An interpolation, not an extrapolation: the pixels either side of
        // the centre are measured, and two more on each side. Where ink hides
        // the stripe's centre, its tail on paper does not place it.
        let has = |t: f64| samples.iter().any(|&(x, _)| x == t);
        let below = samples.iter().filter(|&&(t, _)| t < centre).count();
        let above = samples.len() - below;
        if !(has(centre.floor()) && has(centre.ceil()) && below >= 2 && above >= 2) {
            continue;
        }
        fits.push((line as f64, centre, sigma));
    }
    fits.iter()
        .map(|&(line, centre, sigma)| Centre {
            pixel: if along_x {
                [line, centre]
            } else {
                [centre, line]
            },
            // Across the stripe: the scan line's σ times the cosine of the
            // stripe's local slope, the line through the centres within
            // SLOPE_LINES lines.
            sigma_px: sigma / slope(&fits, line).hypot(1.0),
        })
        .collect()
}

/// The stripe's slope (centre per line) at `line`: a line fitted to the
/// centres of `fits` (`(line, centre, _)`, ascending) within
/// [`SLOPE_LINES`]; 0 with fewer than three.
fn slope(fits: &[(f64, f64, f64)], line: f64) -> f64 {
    let near = fits
        .iter()
        .skip_while(|f| f.0 < line - SLOPE_LINES)
        .take_while(|f| f.0 <= line + SLOPE_LINES);
    let (mut n, mut sx, mut sy, mut sxx, mut sxy) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for &(x, y, _) in near {
        let x = x - line;
        n += 1.0;
        sx += x;
        sy += y;
        sxx += x * x;
        sxy += x * y;
    }
    let det = n * sxx - sx * sx;
    if n < 3.0 || det <= 0.0 {
        return 0.0;
    }
    (n * sxy - sx * sy) / det
}

/// Distance of `p` from the segment `a`–`b`, and its signed distance from
/// the segment's line, positive toward +Y for a segment within 45° of x and
/// toward +X otherwise.
fn to_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> (f64, f64) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dy * dy;
    if len2 == 0.0 {
        let d = (p[0] - a[0]).hypot(p[1] - a[1]);
        return (d, 0.0);
    }
    let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0);
    let d = (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy);
    let mut n = [-dy, dx];
    if (dx.abs() >= dy.abs() && n[1] < 0.0) || (dx.abs() < dy.abs() && n[0] < 0.0) {
        n = [dy, -dx];
    }
    let signed = ((p[0] - a[0]) * n[0] + (p[1] - a[1]) * n[1]) / len2.sqrt();
    (d, signed)
}

/// The analytic stripe of `view` as polylines of its visible points (a
/// break wherever a point is not seen).
fn truth(view: &GtView) -> Vec<Vec<[f64; 2]>> {
    let mut runs = Vec::new();
    for stripe in &view.laser_points {
        let mut run: Vec<[f64; 2]> = Vec::new();
        for p in &stripe.points {
            match (p.visible(), p.pixel) {
                (true, Some(px)) => run.push(px),
                _ => {
                    if !run.is_empty() {
                        runs.push(std::mem::take(&mut run));
                    }
                }
            }
        }
        if !run.is_empty() {
            runs.push(run);
        }
    }
    runs
}

/// Compare `pixels` with the analytic stripe `runs`.
#[must_use]
pub fn check(pixels: &[[f64; 2]], runs: &[Vec<[f64; 2]>]) -> StripeCheck {
    let mut out = StripeCheck {
        points: pixels.len(),
        ..StripeCheck::default()
    };
    let (mut sq, mut signed) = (0.0, 0.0);
    for &p in pixels {
        let mut best: Option<(f64, f64)> = None;
        for run in runs {
            let segments: Vec<([f64; 2], [f64; 2])> = if run.len() == 1 {
                vec![(run[0], run[0])]
            } else {
                run.windows(2).map(|w| (w[0], w[1])).collect()
            };
            for (a, b) in segments {
                let (d, signed) = to_segment(p, a, b);
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, signed));
                }
            }
        }
        match best {
            Some((d, s)) if d <= OUTLIER_PX => {
                out.matched += 1;
                sq += d * d;
                signed += s;
                out.max_px = out.max_px.max(d);
            }
            _ => out.outliers += 1,
        }
    }
    if out.matched > 0 {
        out.rms_px = (sq / out.matched as f64).sqrt();
        out.mean_px = signed / out.matched as f64;
    }
    out
}

/// The beam's σ across the stripe at `pixel` of `view` (camera `model`), in
/// pixels: the analytic stripe point nearest `pixel`, if within
/// [`OUTLIER_PX`], gives the beam's diameter and the surface's tangent plane,
/// on which the distance from the laser plane is differenced across the image.
fn beam_sigma(
    gt: &GroundTruth,
    capture: &GtCapture,
    view: &GtView,
    model: &CameraModel,
    pixel: [f64; 2],
) -> Option<f64> {
    let mut nearest: Option<(f64, &str, usize)> = None;
    for stripe in &view.laser_points {
        for p in &stripe.points {
            if let (true, Some(px)) = (p.visible(), p.pixel) {
                let d = (px[0] - pixel[0]).hypot(px[1] - pixel[1]);
                if d <= OUTLIER_PX && nearest.is_none_or(|(b, ..)| d < b) {
                    nearest = Some((d, &stripe.laser, p.point));
                }
            }
        }
    }
    let (_, laser, j) = nearest?;
    let stripe = capture.laser_stripes.iter().find(|s| s.laser == laser)?;
    let width = *stripe.width_m.get(j)?;
    let plane = gt
        .lasers
        .iter()
        .find(|l| l.id == stripe.laser)?
        .camera_planes
        .iter()
        .find(|p| p.frame == view.camera)?;
    let cam_se3_target = view.world_se3_camera.inverse() * capture.world_se3_target;
    let point = stripe.points.get(j)?;
    let x = (cam_se3_target * point.xyz()).coords;
    let n = cam_se3_target.rotation * point.normal.map_or_else(Vector3::z, Vector3::from);
    let fan = Vector3::from(plane.normal);
    // The distance from the laser plane of what pixel (u, v) sees.
    let d = |u: f64, v: f64| {
        let ray = model.backproject_pixel(&Point2::new(u, v)).point;
        fan.dot(&(ray * (n.dot(&x) / n.dot(&ray)))) + plane.distance
    };
    const H: f64 = 0.5;
    let [u, v] = pixel;
    let gradient = (d(u + H, v) - d(u - H, v)).hypot(d(u, v + H) - d(u, v - H)) / (2.0 * H);
    (gradient > 0.0).then(|| width / 4.0 / gradient)
}

/// The `q` quantile of `sorted` (nearest rank).
fn quantile(sorted: &[f64], q: f64) -> f64 {
    sorted[((sorted.len() - 1) as f64 * q).round() as usize]
}

/// Compare the fitted widths `sigma_px` at `pixels` with the beam's.
fn width_check(
    gt: &GroundTruth,
    capture: &GtCapture,
    view: &GtView,
    model: &CameraModel,
    pixels: &[[f64; 2]],
    sigma_px: &[f64],
) -> Option<WidthCheck> {
    let (mut fitted, mut beam, mut ratios) = (Vec::new(), Vec::new(), Vec::new());
    for (&p, &sigma) in pixels.iter().zip(sigma_px) {
        if let Some(b) = beam_sigma(gt, capture, view, model, p) {
            fitted.push(sigma);
            beam.push(b);
            ratios.push(sigma / b);
        }
    }
    if ratios.is_empty() {
        return None;
    }
    for v in [&mut fitted, &mut beam, &mut ratios] {
        v.sort_by(f64::total_cmp);
    }
    Some(WidthCheck {
        points: ratios.len(),
        sigma_px: quantile(&fitted, 0.5),
        beam_sigma_px: quantile(&beam, 0.5),
        ratio: quantile(&ratios, 0.5),
        ratio_p05: quantile(&ratios, 0.05),
        ratio_p95: quantile(&ratios, 0.95),
    })
}

/// Extract the stripe from every laser image of the dataset in `dir` (`gt`
/// its ground truth) and check it; `None` if the dataset has no laser images.
///
/// # Errors
///
/// If an image exists but does not read.
pub fn extract_dataset(dir: &Path, gt: &GroundTruth) -> Result<Option<LaserFeatures>> {
    if !gt
        .captures
        .iter()
        .flat_map(|c| &c.views)
        .any(|v| v.laser_image.is_some())
    {
        return Ok(None);
    }
    let offset = gt.pixel_centre.offset();
    let models = gt
        .cameras
        .iter()
        .map(|c| Ok((c.id.as_str(), c.params.build()?)))
        .collect::<Result<std::collections::HashMap<_, _>>>()?;
    let mut captures = Vec::with_capacity(gt.captures.len());
    for capture in &gt.captures {
        let mut views = Vec::new();
        for view in &capture.views {
            let Some(image) = &view.laser_image else {
                continue;
            };
            let path = dir.join(image);
            let centres = if path.is_file() {
                let laser = read_png_luma8(&path)?;
                let board_path = dir.join(&view.image);
                let board = if board_path.is_file() {
                    Some(read_png_luma8(&board_path)?)
                } else {
                    None
                };
                extract(&laser, board.as_ref())
            } else {
                Vec::new()
            };
            let pixels: Vec<[f64; 2]> = centres
                .iter()
                .map(|c| [c.pixel[0] + offset, c.pixel[1] + offset])
                .collect();
            let sigma_px: Vec<f64> = centres.iter().map(|c| c.sigma_px).collect();
            let check = check(&pixels, &truth(view));
            let width = models
                .get(view.camera.as_str())
                .and_then(|model| width_check(gt, capture, view, model, &pixels, &sigma_px));
            views.push(LaserView {
                camera: view.camera.clone(),
                image: image.clone(),
                pixels,
                sigma_px,
                check,
                width,
            });
        }
        captures.push(LaserCapture {
            id: capture.id.clone(),
            views,
        });
    }
    Ok(Some(LaserFeatures {
        version: LASER_FEATURES_VERSION,
        method: format!(
            "board image subtracted at its median dimming and divided out (pixels below \
             {MIN_BOARD_DN} DN in it, or saturated, left out); per column (or row) across the \
             stripe, a Gaussian fitted (weighted log-parabola) to the samples above {FLOOR} of \
             the peak (peak ≥ {MIN_PEAK_DN} DN); its σ across the stripe compared with the \
             beam's"
        ),
        captures,
    }))
}

/// Per camera: laser images, extracted pixels, outliers, RMS and mean
/// distance from the analytic stripe, and the fitted width over the beam's
/// (the median over images, and the images' extremes), as table lines.
#[must_use]
pub fn summary_table(features: &LaserFeatures) -> Vec<String> {
    let mut cameras: Vec<&str> = Vec::new();
    for v in features.captures.iter().flat_map(|c| &c.views) {
        if !cameras.contains(&v.camera.as_str()) {
            cameras.push(&v.camera);
        }
    }
    let mut lines = vec![format!(
        "{:<12} {:>6} {:>8} {:>8} {:>9} {:>9} {:>9}  {}",
        "camera", "images", "pixels", "outliers", "rms px", "mean px", "max px", "width/beam"
    )];
    for camera in cameras {
        let views: Vec<&LaserView> = features
            .captures
            .iter()
            .flat_map(|c| &c.views)
            .filter(|v| v.camera == camera)
            .collect();
        let (mut points, mut outliers, mut matched) = (0, 0, 0);
        let (mut sq, mut signed, mut max) = (0.0, 0.0, 0.0_f64);
        for v in &views {
            points += v.check.points;
            outliers += v.check.outliers;
            matched += v.check.matched;
            sq += v.check.rms_px.powi(2) * v.check.matched as f64;
            signed += v.check.mean_px * v.check.matched as f64;
            max = max.max(v.check.max_px);
        }
        let m = matched.max(1) as f64;
        let mut ratios: Vec<f64> = views
            .iter()
            .filter_map(|v| v.width.as_ref().map(|w| w.ratio))
            .collect();
        ratios.sort_by(f64::total_cmp);
        let width = if ratios.is_empty() {
            "-".to_owned()
        } else {
            format!(
                "{:.3} ({:.3}–{:.3})",
                quantile(&ratios, 0.5),
                ratios[0],
                ratios[ratios.len() - 1]
            )
        };
        lines.push(format!(
            "{camera:<12} {:>6} {points:>8} {outliers:>8} {:>9.4} {:>+9.4} {max:>9.3}  {width}",
            views.len(),
            (sq / m).sqrt(),
            signed / m
        ));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A checkerboard dimmed to 10 %, plus a Gaussian stripe through
    /// `y = y0 + slope·x` (σ 2 px, peak 120 DN).
    fn images(y0: f64, slope: f64) -> (Gray8, Gray8) {
        let (w, h) = (320u32, 240u32);
        let mut board = Vec::new();
        let mut laser = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let paper = ((x / 40) + (y / 40)) % 2 == 0;
                let b: f64 = if paper { 220.0 } else { 8.0 };
                let d = f64::from(y) - (y0 + slope * f64::from(x));
                let stripe = 120.0 * (-d * d / (2.0 * 4.0)).exp() * if paper { 1.0 } else { 0.04 };
                board.push(b.round() as u8);
                laser.push((0.1 * b + stripe).round().min(255.0) as u8);
            }
        }
        let image = |data| Gray8 {
            width: w,
            height: h,
            data,
        };
        (image(laser), image(board))
    }

    #[test]
    fn the_dimming_is_recovered() {
        let (laser, board) = images(100.3, 0.05);
        assert!((dimming(&laser, &board) - 0.1).abs() < 0.01);
    }

    #[test]
    fn the_centre_line_is_found_across_the_checkerboard() {
        for (y0, slope) in [(100.3, 0.05), (80.7, -0.12)] {
            let (laser, board) = images(y0, slope);
            let centres = extract(&laser, Some(&board));
            let pixels: Vec<[f64; 2]> = centres.iter().map(|c| c.pixel).collect();
            assert!(pixels.len() > 100, "{} columns", pixels.len());
            let worst = pixels
                .iter()
                .map(|[x, y]| (y - (y0 + slope * x)).abs())
                .fold(0.0, f64::max);
            assert!(worst < 0.15, "worst {worst} px");
            // σ 2 px down the columns is 2·cos(atan slope) across the stripe.
            let mut sigma: Vec<f64> = centres.iter().map(|c| c.sigma_px).collect();
            sigma.sort_by(f64::total_cmp);
            let across = 2.0 / slope.hypot(1.0);
            let median = quantile(&sigma, 0.5);
            assert!(
                (median / across - 1.0).abs() < 0.01,
                "σ {median} px, {across} px"
            );
            let truth = vec![vec![[0.0, y0], [319.0, y0 + slope * 319.0]]];
            let c = check(&pixels, &truth);
            assert_eq!(c.outliers, 0);
            assert!(c.mean_px.abs() < 0.05, "bias {}", c.mean_px);
        }
    }

    #[test]
    fn a_vertical_stripe_is_scanned_by_rows() {
        let (laser, board) = images(100.0, 0.0);
        // Transpose: the stripe runs down the image.
        let t = |g: &Gray8| Gray8 {
            width: g.height,
            height: g.width,
            data: (0..g.width)
                .flat_map(|x| (0..g.height).map(move |y| (x, y)))
                .map(|(x, y)| g.data[(y * g.width + x) as usize])
                .collect(),
        };
        let centres = extract(&t(&laser), Some(&t(&board)));
        assert!(centres.len() > 100);
        assert!(centres.iter().all(|c| (c.pixel[0] - 100.0).abs() < 0.15));
    }

    #[test]
    fn no_stripe_no_pixels() {
        let (_, board) = images(0.0, 0.0);
        let dark = Gray8 {
            data: board
                .data
                .iter()
                .map(|&b| (f64::from(b) * 0.1).round() as u8)
                .collect(),
            ..board.clone()
        };
        assert!(extract(&dark, Some(&board)).is_empty());
    }
}
