//! The stripe's centre line in one laser image (P7-7): the board image
//! subtracted and divided out, then a Gaussian fitted across the stripe on
//! every column (or row). See the [module](super) docs for the steps.

use etendue_synth::images::Gray8;

/// A column (or row) is part of the stripe when its peak is at least this
/// far above the dimmed board image, in DN.
pub(super) const MIN_PEAK_DN: f64 = 8.0;
/// The fit takes the samples above this fraction of the peak.
pub(super) const FLOOR: f64 = 0.1;
/// At most this many pixels either side of the peak.
pub(super) const HALF_WINDOW: usize = 20;
/// Board pixels darker than this (ink) carry no measurement: neither the
/// dimming nor the stripe is taken from them.
pub(super) const MIN_BOARD_DN: u8 = 16;
/// The stripe's local slope is fitted over this many lines either side.
pub(super) const SLOPE_LINES: f64 = 8.0;

/// A point of a stripe's centre line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Centre {
    /// `[x, y]`, pixel `i` at `i`.
    pub pixel: [f64; 2],
    /// The fitted Gaussian's σ across the stripe (perpendicular to its
    /// local direction), pixels.
    pub sigma_px: f64,
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
    extract_along(laser, board, None)
}

/// [`extract`], the stripe's direction given: `Some(true)` scans columns (a stripe running along
/// x, one centre per column), `Some(false)` rows; `None` takes it from the image.
///
/// # Panics
///
/// If the two images differ in size.
#[must_use]
pub fn extract_along(laser: &Gray8, board: Option<&Gray8>, along_x: Option<bool>) -> Vec<Centre> {
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
    let along_x = along_x
        .unwrap_or_else(|| (2.0 * cxy).atan2(cxx - cyy).abs() / 2.0 <= std::f64::consts::FRAC_PI_4);
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
        // The peak's run: outward from it while samples stay above the
        // floor (ink, which measures nothing, does not end it), so a noise
        // pixel beyond the tail is not fitted.
        let lo = peak_at.saturating_sub(HALF_WINDOW);
        let hi = (peak_at + HALF_WINDOW + 1).min(span);
        let above = |t: &usize| at(line, *t).is_none_or(|(_, v)| v > FLOOR * peak);
        let first = (lo..peak_at)
            .rev()
            .take_while(above)
            .last()
            .unwrap_or(peak_at);
        let last = (peak_at + 1..hi)
            .take_while(above)
            .last()
            .unwrap_or(peak_at);
        let samples: Vec<(f64, f64)> = (first..=last)
            .filter_map(|t| at(line, t).map(|(_, v)| (t as f64, v)))
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

#[cfg(test)]
mod tests {
    use super::super::{check, quantile};
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
    fn a_noise_pixel_beyond_the_tail_is_not_fitted() {
        let (mut laser, board) = images(100.3, 0.0);
        // A bright pixel 12 rows below the stripe in every column on paper.
        for x in 0..laser.width as usize {
            let k = 112 * laser.width as usize + x;
            if board.data[k] > 100 {
                laser.data[k] += 15;
            }
        }
        let centres = extract(&laser, Some(&board));
        assert!(centres.len() > 100);
        assert!(centres.iter().all(|c| (c.pixel[1] - 100.3).abs() < 0.15));
        assert!(
            centres
                .iter()
                .all(|c| (c.sigma_px / 2.0 - 1.0).abs() < 0.05)
        );
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
