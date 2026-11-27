//! The stripe's continuity (P20d): a real laser image has no board image to subtract, so beyond
//! the stripe's ends, and where the board's ink hides it, a scan line's brightest pixel is the
//! board's dim light. Such centres scatter; the stripe's form a continuous line. Centres are
//! chained along the scan lines, and only long chains are kept. A defocused bright patch of the
//! board can chain too, but it is wide: a chain is kept only as thin as the stripe, the longest
//! chain.

use super::Centre;

/// A chain continues across at most this many lines without a centre (the board's ink hides
/// the stripe on its dark squares).
const MAX_GAP_LINES: f64 = 20.0;
/// A centre continues a chain when it is within this many pixels of the chain's line,
/// extrapolated from its last centres.
const LINK_PX: f64 = 2.0;
/// The chain's slope is taken over its last centres within this many lines.
const SLOPE_LINES: f64 = 8.0;
/// Chains shorter than this many centres are not the stripe.
pub(super) const MIN_CHAIN: usize = 16;
/// A chain whose median width (σ across) is more than this many times the longest chain's is
/// not the stripe.
const MAX_WIDTH_RATIO: f64 = 2.0;

/// The centres of `centres` (one per scan line, as [`super::extract_along`] gives them) on chains
/// of at least [`MIN_CHAIN`], at most [`MAX_WIDTH_RATIO`] times as wide as the longest; `along_x`
/// says which coordinate is the scan line's (`x` for a stripe along x).
#[must_use]
pub fn chained(centres: &[Centre], along_x: bool) -> Vec<Centre> {
    let split = |c: &Centre| {
        if along_x {
            (c.pixel[0], c.pixel[1])
        } else {
            (c.pixel[1], c.pixel[0])
        }
    };
    let mut order: Vec<usize> = (0..centres.len()).collect();
    order.sort_by(|&a, &b| split(&centres[a]).0.total_cmp(&split(&centres[b]).0));
    // Each chain: its centres' indices, in line order.
    let mut chains: Vec<Vec<usize>> = Vec::new();
    for &k in &order {
        let (line, at) = split(&centres[k]);
        let best = chains
            .iter()
            .enumerate()
            .filter_map(|(i, chain)| {
                let (last_line, last_at) = split(&centres[*chain.last()?]);
                let gap = line - last_line;
                if gap <= 0.0 || gap > MAX_GAP_LINES + 1.0 {
                    return None;
                }
                // Before the chain has a slope, any slope within the scan axis's 45°.
                let miss = match slope(centres, chain, &split) {
                    Some(s) => (at - (last_at + s * gap)).abs(),
                    None => ((at - last_at).abs() - gap).max(0.0),
                };
                (miss <= LINK_PX).then_some((i, miss))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        match best {
            Some((i, _)) => chains[i].push(k),
            None => chains.push(vec![k]),
        }
    }
    let width = |chain: &[usize]| {
        let mut s: Vec<f64> = chain.iter().map(|&k| centres[k].sigma_px).collect();
        s.sort_by(f64::total_cmp);
        s[s.len() / 2]
    };
    chains.retain(|c| c.len() >= MIN_CHAIN);
    let Some(stripe) = chains.iter().max_by_key(|c| c.len()).map(|c| width(c)) else {
        return Vec::new();
    };
    let mut kept: Vec<usize> = chains
        .into_iter()
        .filter(|c| width(c) <= MAX_WIDTH_RATIO * stripe)
        .flatten()
        .collect();
    kept.sort_unstable();
    kept.into_iter().map(|k| centres[k]).collect()
}

/// The slope (along-scan position per line) through the chain's last centres within
/// [`SLOPE_LINES`] of its end; `None` with fewer than two.
fn slope(
    centres: &[Centre],
    chain: &[usize],
    split: &impl Fn(&Centre) -> (f64, f64),
) -> Option<f64> {
    let (end, _) = split(&centres[*chain.last()?]);
    let near: Vec<(f64, f64)> = chain
        .iter()
        .rev()
        .map(|&k| split(&centres[k]))
        .take_while(|&(line, _)| end - line <= SLOPE_LINES)
        .collect();
    if near.len() < 2 {
        return None;
    }
    let n = near.len() as f64;
    let (sx, sy) = near
        .iter()
        .fold((0.0, 0.0), |(a, b), &(x, y)| (a + x, b + y));
    let (mx, my) = (sx / n, sy / n);
    let (sxy, sxx) = near.iter().fold((0.0, 0.0), |(a, b), &(x, y)| {
        (a + (x - mx) * (y - my), b + (x - mx) * (x - mx))
    });
    (sxx > 0.0).then(|| sxy / sxx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn centre(x: f64, y: f64) -> Centre {
        Centre {
            pixel: [x, y],
            sigma_px: 1.0,
        }
    }

    #[test]
    fn the_stripe_is_kept_and_scattered_centres_are_not() {
        // A sloped stripe over columns 100..300 with a gap of 15 columns (ink), and centres in
        // every other column scattered over the image.
        // A hash scatters them: no two in neighbouring columns are within a few pixels.
        let scatter = |x: u64| ((x.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 33) % 540) as f64;
        let mut centres = Vec::new();
        for k in 0..400u64 {
            let x = k as f64;
            if (100.0..300.0).contains(&x) && !(180.0..195.0).contains(&x) {
                centres.push(centre(x, 200.0 + 0.3 * x + 0.2 * (x * 0.7).sin()));
            } else {
                centres.push(centre(x, scatter(k)));
            }
        }
        // Every stripe centre is kept, and of the 215 scattered ones only the few that happen
        // to fall on the stripe's line where it is extrapolated (its gap and just past its ends).
        let on_stripe = |c: &Centre| {
            let x = c.pixel[0];
            c.pixel[1] == 200.0 + 0.3 * x + 0.2 * (x * 0.7).sin()
        };
        let kept = chained(&centres, true);
        assert_eq!(kept.iter().filter(|c| on_stripe(c)).count(), 200 - 15);
        assert!(kept.len() <= 200 - 15 + 5, "{} kept", kept.len());
        // The same along y.
        let swapped: Vec<Centre> = centres
            .iter()
            .map(|c| centre(c.pixel[1], c.pixel[0]))
            .collect();
        let kept = chained(&swapped, false);
        assert!(
            (200 - 15..=200 - 15 + 5).contains(&kept.len()),
            "{} kept",
            kept.len()
        );
    }

    #[test]
    fn a_wide_chain_is_not_the_stripe() {
        let mut centres: Vec<Centre> = (0..100).map(|x| centre(f64::from(x), 50.0)).collect();
        for x in 100..140 {
            centres.push(Centre {
                pixel: [f64::from(x), 300.0],
                sigma_px: 8.0,
            });
        }
        let kept = chained(&centres, true);
        assert_eq!(kept.len(), 100);
        assert!(kept.iter().all(|c| c.pixel[1] == 50.0));
    }

    #[test]
    fn a_short_run_is_not_the_stripe() {
        let centres: Vec<Centre> = (0..MIN_CHAIN - 1).map(|x| centre(x as f64, 50.0)).collect();
        assert!(chained(&centres, true).is_empty());
    }
}
