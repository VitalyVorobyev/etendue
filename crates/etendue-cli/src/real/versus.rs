//! A real calibration against its twin (P20b, characterisation T2): per camera, the residuals'
//! level and whiteness; per parameter, the real σ against the twin's scatter, and the real
//! estimate's distance from the twin's mean.
//!
//! Whiteness is measured three ways, each with `σ` the camera's own RMS per axis, so that white
//! residuals score about 1 whatever their level:
//! - **across the image:** the residuals binned on an 8 × 6 grid, `Σ n |mean|² / σ²` over the bins
//!   with at least [`MIN_BIN`] points, divided by its degrees of freedom (two per bin);
//! - **within each view:** the same with each view's residuals binned on a 4 × 3 grid of their
//!   own. A view's own mean is no use: a calibration that refines the robot poses moves each view
//!   to absorb it. What a view's pose cannot absorb stays (a target that is not flat, a lens the
//!   model does not fit, a detector's position-dependent bias);
//! - **radially:** the mean radial component (outward from the principal point) in four rings of
//!   equal width out to the farthest image corner, in pixels.
//!
//! The twin's residuals are white by construction, so its scores are the reference: the scores of
//! white noise after the same fit.

use std::collections::BTreeMap;

use etendue_synth::dataset::GroundTruth;
use serde::Serialize;

use crate::calibrate::{FeatureResidual, Report, Run, param_stats};

/// The fewest points a bin or a view needs to be scored.
pub const MIN_BIN: usize = 5;
/// Bins across and down the image.
const GRID: [usize; 2] = [8, 6];
/// Bins across and down the image within one view.
const VIEW_GRID: [usize; 2] = [4, 3];
/// Radial rings.
const RINGS: usize = 4;

/// The residuals of one camera of one calibration.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Whiteness {
    /// Points with a residual.
    pub points: usize,
    /// Root mean square per image axis (px).
    pub rms_axis_px: f64,
    /// Across the image: `Σ n |mean|² / σ²` over the bins, per degree of freedom (1 when white).
    pub spatial: f64,
    /// The largest bin's mean residual (px).
    pub max_bin_px: f64,
    /// Within each view: as `spatial`, on each view's own coarser grid (1 when white).
    pub view: f64,
    /// The mean radial residual (px, positive outward) in each ring, centre outward.
    pub radial_px: [f64; RINGS],
}

/// One camera, real against twin.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CameraVersus {
    pub camera: String,
    pub real: Whiteness,
    pub twin: Whiteness,
    /// `real.rms_axis_px / twin.rms_axis_px`.
    pub rms_ratio: f64,
}

/// One parameter, real against twin.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ParamVersus {
    /// As [`crate::calibrate::ParamStats`] labels it (`cam0 fx`, `hand-eye tz`).
    pub label: String,
    pub unit: String,
    /// The real estimate (the twin's truth).
    pub real: f64,
    /// calibration-rs's σ of the real estimate.
    pub real_sigma: Option<f64>,
    /// calibration-rs's σ of the twin's, the mean over its trials.
    pub twin_sigma: Option<f64>,
    /// The twin's standard deviation over its trials.
    pub twin_std: f64,
    /// `(real − twin mean) / twin std`: how far the real estimate is from where its twin
    /// lands, in the twin's σ.
    pub distance_sigma: f64,
}

/// A real calibration against its twin.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Versus {
    pub cameras: Vec<CameraVersus>,
    pub params: Vec<ParamVersus>,
}

/// `real` (a real run's report) against `twin` (the trials of its twin, whose truth is `gt`).
/// The twin's residuals are its first trial's; its parameters are those its trials move.
#[must_use]
pub fn versus(real: &Report, gt: &GroundTruth, twin: &Run) -> Versus {
    let first = twin.reports.first();
    let cameras = gt
        .cameras
        .iter()
        .map(|c| {
            let k = &c.params.intrinsics;
            let at = |features: &[FeatureResidual]| {
                whiteness(features, &c.id, c.resolution, [k.cx, k.cy])
            };
            let real = at(&real.residuals.features);
            let twin = at(first.map_or(&[][..], |r| &r.residuals.features));
            CameraVersus {
                camera: c.id.clone(),
                rms_ratio: real.rms_axis_px / twin.rms_axis_px,
                real,
                twin,
            }
        })
        .collect();
    let real_sigma: BTreeMap<String, Option<f64>> = param_stats(gt, std::slice::from_ref(real))
        .into_iter()
        .map(|p| (p.label, p.predicted_sigma))
        .collect();
    let params = twin
        .param_stats
        .clone()
        .unwrap_or_else(|| param_stats(gt, &twin.reports))
        .into_iter()
        // A parameter the twin never moves is held fixed (`k3` by default).
        .filter(|p| p.std > 0.0)
        .map(|p| ParamVersus {
            real_sigma: real_sigma.get(&p.label).copied().flatten(),
            twin_sigma: p.predicted_sigma,
            twin_std: p.std,
            distance_sigma: (p.truth - p.mean) / p.std,
            real: p.truth,
            unit: p.unit,
            label: p.label,
        })
        .collect();
    Versus { cameras, params }
}

/// Running sums of residuals: count, Σ du, Σ dv.
#[derive(Clone, Copy, Default)]
struct Sum(usize, f64, f64);

impl Sum {
    fn add(&mut self, d: [f64; 2]) {
        self.0 += 1;
        self.1 += d[0];
        self.2 += d[1];
    }

    fn mean(&self) -> [f64; 2] {
        let n = self.0 as f64;
        [self.1 / n, self.2 / n]
    }

    /// `n |mean|² / σ²`.
    fn chi2(&self, var: f64) -> f64 {
        let [u, v] = self.mean();
        self.0 as f64 * (u * u + v * v) / var
    }
}

/// `Σ n |mean|² / σ²` over the groups with at least [`MIN_BIN`] points, per degree of freedom.
fn score<'a>(groups: impl Iterator<Item = &'a Sum>, var: f64) -> f64 {
    let (chi2, dof) = groups
        .filter(|g| g.0 >= MIN_BIN)
        .fold((0.0, 0usize), |(c, d), g| (c + g.chi2(var), d + 2));
    if dof == 0 {
        f64::NAN
    } else {
        chi2 / dof as f64
    }
}

/// The whiteness of `camera`'s residuals among `features`, for an image of `size` with its
/// principal point at `centre`.
#[must_use]
pub fn whiteness(
    features: &[FeatureResidual],
    camera: &str,
    size: [u32; 2],
    centre: [f64; 2],
) -> Whiteness {
    let [w, h] = size.map(f64::from);
    let r_max = [[0.0, 0.0], [w, 0.0], [0.0, h], [w, h]]
        .iter()
        .map(|c: &[f64; 2]| (c[0] - centre[0]).hypot(c[1] - centre[1]))
        .fold(0.0, f64::max);
    let mut bins = vec![Sum::default(); GRID[0] * GRID[1]];
    let mut views: BTreeMap<(&str, usize), Sum> = BTreeMap::new();
    let mut rings = [(0usize, 0.0); RINGS];
    let mut sq = 0.0;
    let mut n = 0usize;
    for f in features.iter().filter(|f| f.camera == camera) {
        let d = [
            f.projected_px[0] - f.observed_px[0],
            f.projected_px[1] - f.observed_px[1],
        ];
        let [x, y] = f.observed_px;
        n += 1;
        sq += d[0] * d[0] + d[1] * d[1];
        let bin = |g: [usize; 2]| {
            let col = ((x / w * g[0] as f64).floor().max(0.0) as usize).min(g[0] - 1);
            let row = ((y / h * g[1] as f64).floor().max(0.0) as usize).min(g[1] - 1);
            row * g[0] + col
        };
        bins[bin(GRID)].add(d);
        views
            .entry((f.capture.as_str(), bin(VIEW_GRID)))
            .or_default()
            .add(d);
        let (rx, ry) = (x - centre[0], y - centre[1]);
        let r = rx.hypot(ry);
        if r > 0.0 {
            let ring = ((r / r_max * RINGS as f64) as usize).min(RINGS - 1);
            rings[ring].0 += 1;
            rings[ring].1 += (d[0] * rx + d[1] * ry) / r;
        }
    }
    let var = sq / (2.0 * n as f64);
    Whiteness {
        points: n,
        rms_axis_px: var.sqrt(),
        spatial: score(bins.iter(), var),
        max_bin_px: bins
            .iter()
            .filter(|b| b.0 >= MIN_BIN)
            .map(|b| {
                let [u, v] = b.mean();
                u.hypot(v)
            })
            .fold(0.0, f64::max),
        view: score(views.values(), var),
        radial_px: rings.map(|(k, s)| if k == 0 { f64::NAN } else { s / k as f64 }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feature(view: usize, at: [f64; 2], d: [f64; 2]) -> FeatureResidual {
        FeatureResidual {
            capture: view.to_string(),
            camera: "cam".into(),
            point: 0,
            observed_px: at,
            projected_px: [at[0] + d[0], at[1] + d[1]],
            raw: [0; 3],
        }
    }

    /// Residuals that alternate in sign within every bin and view score 0; a constant offset
    /// scores high, and a radial one shows in the rings.
    #[test]
    fn structure_scores_and_alternating_residuals_do_not() {
        let mut white = Vec::new();
        let mut offset = Vec::new();
        let mut radial = Vec::new();
        for view in 0..4 {
            for i in 0..48 {
                let at = [(i % 8) as f64 * 100.0 + 50.0, (i / 8) as f64 * 100.0 + 50.0];
                for s in [1.0, -1.0] {
                    white.push(feature(view, at, [0.1 * s, -0.1 * s]));
                    offset.push(feature(view, at, [0.02 * s + 0.1, 0.02 * s]));
                    let (rx, ry) = (at[0] - 400.0, at[1] - 300.0);
                    radial.push(feature(view, at, [0.1 * s + 1e-4 * rx, 1e-4 * ry]));
                }
            }
        }
        let score = |f: &[FeatureResidual]| whiteness(f, "cam", [800, 600], [400.0, 300.0]);
        let w = score(&white);
        assert_eq!(w.points, 384);
        assert!((w.rms_axis_px - 0.1).abs() < 1e-12);
        assert!(w.spatial < 1e-20 && w.view < 1e-20, "{w:?}");
        assert!(w.radial_px.iter().all(|r| r.abs() < 1e-12), "{w:?}");
        let o = score(&offset);
        assert!(o.spatial > 5.0 && o.view > 5.0, "{o:?}");
        assert!((o.max_bin_px - 0.1).abs() < 1e-12);
        let r = score(&radial);
        assert!(
            r.radial_px.windows(2).all(|p| p[1] > p[0]) && r.radial_px[0] > 0.0,
            "{r:?}"
        );
        assert!(score(&[]).spatial.is_nan());
    }
}
