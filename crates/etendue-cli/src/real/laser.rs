//! A real laser calibration's stripe residuals (P20d): per camera, how far the extracted stripe
//! pixels are from the calibrated laser line in the image and from the plane in space.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use vision_calibration_pipeline::common::ExportDiagnostics;

/// The laser residuals of one camera.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LaserResiduals {
    /// The camera, by id.
    pub camera: String,
    /// Views with laser pixels.
    pub views: usize,
    /// Laser pixels with a residual.
    pub pixels: usize,
    /// Root mean square of the pixels' distances from the calibrated laser line in the image
    /// (px): the residual the laser solve minimises.
    pub rms_px: f64,
    /// Largest distance from the line (px).
    pub max_px: f64,
    /// Root mean square of the back-projected pixels' distances from the calibrated plane (m).
    pub rms_m: f64,
}

/// Each camera's laser residuals in `diagnostics`, for `cameras` cameras named by `id`; empty
/// without laser residuals.
pub fn laser_residuals(
    diagnostics: &ExportDiagnostics,
    cameras: usize,
    id: &impl Fn(usize) -> String,
) -> Vec<LaserResiduals> {
    let laser = &diagnostics.per_feature_residuals.laser;
    if laser.is_empty() {
        return Vec::new();
    }
    (0..cameras)
        .filter_map(|cam| {
            let mine: Vec<_> = laser.iter().filter(|r| r.camera == cam).collect();
            let px: Vec<f64> = mine.iter().filter_map(|r| r.residual_px).collect();
            let m: Vec<f64> = mine.iter().filter_map(|r| r.residual_m).collect();
            (!px.is_empty()).then(|| LaserResiduals {
                camera: id(cam),
                views: mine.iter().map(|r| r.pose).collect::<BTreeSet<_>>().len(),
                pixels: px.len(),
                rms_px: rms(&px),
                max_px: px.iter().copied().map(f64::abs).fold(0.0, f64::max),
                rms_m: rms(&m),
            })
        })
        .collect()
}

fn rms(v: &[f64]) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    (v.iter().map(|x| x * x).sum::<f64>() / v.len() as f64).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use vision_calibration_core::LaserFeatureResidual;

    fn residual(pose: usize, camera: usize, px: f64, m: f64) -> LaserFeatureResidual {
        let mut r = LaserFeatureResidual::default();
        r.pose = pose;
        r.camera = camera;
        r.residual_m = Some(m);
        r.residual_px = Some(px);
        r
    }

    #[test]
    fn per_camera_counts_and_rms() {
        let mut d = ExportDiagnostics::default();
        d.per_feature_residuals.laser = vec![
            residual(0, 0, 3.0, 0.003),
            residual(0, 0, -4.0, -0.004),
            residual(2, 0, 0.0, 0.0),
            residual(1, 2, 1.0, 0.001),
        ];
        let id = |i: usize| format!("cam{i}");
        let r = laser_residuals(&d, 3, &id);
        assert_eq!(r.len(), 2, "camera 1 has no laser pixels");
        assert_eq!(
            (r[0].camera.as_str(), r[0].views, r[0].pixels),
            ("cam0", 2, 3)
        );
        assert!((r[0].rms_px - (25.0f64 / 3.0).sqrt()).abs() < 1e-12);
        assert_eq!(r[0].max_px, 4.0);
        assert!((r[0].rms_m - (25e-6f64 / 3.0).sqrt()).abs() < 1e-15);
        assert_eq!(
            (r[1].camera.as_str(), r[1].views, r[1].pixels),
            ("cam2", 1, 1)
        );
        assert!(laser_residuals(&ExportDiagnostics::default(), 3, &id).is_empty());
    }
}
