//! The observations a calibration takes: a detector's (`etendue detect`'s files), checked
//! against the ground truth, and their views as calibration-rs's correspondences.

use anyhow::{Context, Result, ensure};
use etendue_synth::dataset::GroundTruth;
use etendue_synth::observe::{
    ObservationNoise, Observations, ObservedCapture, ObservedPoint, ObservedView,
};
use nalgebra::Isometry3;
use vision_calibration_core::{CorrespondenceView, Pt2, Pt3};

/// Observations from `etendue detect`: `features.json`, and `laser_features.json` for a laser
/// dataset. The robot poses are the ground truth's (exact, as the script reads them).
///
/// # Errors
///
/// When the files do not match the ground truth's captures and cameras.
pub fn from_features(
    gt: &GroundTruth,
    features: &crate::detect::Features,
    laser: Option<&crate::detect::laser::LaserFeatures>,
) -> Result<Observations> {
    ensure!(
        features.version == 1,
        "unsupported features.json version {}",
        features.version
    );
    ensure!(
        features.captures.len() == gt.captures.len()
            && features.captures.iter().zip(&gt.captures).all(|(f, g)| {
                f.id == g.id
                    && f.views
                        .iter()
                        .map(|v| &v.camera)
                        .eq(g.views.iter().map(|v| &v.camera))
            }),
        "features.json does not match gt.json's captures and cameras"
    );
    if let Some(l) = laser {
        ensure!(
            l.version == 1,
            "unsupported laser_features.json version {}",
            l.version
        );
        ensure!(
            l.captures
                .iter()
                .map(|c| &c.id)
                .eq(gt.captures.iter().map(|c| &c.id)),
            "laser_features.json does not match gt.json's captures"
        );
    }
    let captures = features
        .captures
        .iter()
        .zip(&gt.captures)
        .enumerate()
        .map(|(k, (f, g))| ObservedCapture {
            id: f.id.clone(),
            base_se3_tcp: g.base_se3_tcp,
            views: f
                .views
                .iter()
                .map(|v| {
                    let ok = v.status == crate::detect::ViewStatus::Ok;
                    let points = if ok {
                        v.points
                            .iter()
                            .map(|p| {
                                let b = &gt.target.points[p.point];
                                ObservedPoint {
                                    point: p.point,
                                    target_xyz: b.xyz_m.unwrap_or([
                                        b.position_m[0],
                                        b.position_m[1],
                                        0.0,
                                    ]),
                                    pixel: p.pixel,
                                    outlier: false,
                                }
                            })
                            .collect()
                    } else {
                        Vec::new()
                    };
                    let stripe = laser
                        .and_then(|l| l.captures[k].views.iter().find(|s| s.camera == v.camera))
                        .map(|s| s.pixels.clone())
                        .unwrap_or_default();
                    ObservedView {
                        camera: v.camera.clone(),
                        points: if points.len() >= 4 {
                            points
                        } else {
                            Vec::new()
                        },
                        laser: if stripe.len() >= 2 {
                            stripe
                        } else {
                            Vec::new()
                        },
                    }
                })
                .collect(),
        })
        .collect();
    Ok(Observations {
        noise: ObservationNoise::default(),
        captures,
    })
}

pub(super) fn correspondences(view: &ObservedView) -> Option<CorrespondenceView> {
    if view.points.len() < 4 {
        return None;
    }
    let p3 = view
        .points
        .iter()
        .map(|p| Pt3::new(p.target_xyz[0], p.target_xyz[1], p.target_xyz[2]));
    let p2 = view.points.iter().map(|p| Pt2::new(p.pixel[0], p.pixel[1]));
    CorrespondenceView::new(p3.collect(), p2.collect()).ok()
}

pub(super) fn laser_pixels(view: &ObservedView) -> Option<Vec<[f64; 2]>> {
    (view.laser.len() >= 2).then(|| view.laser.clone())
}

/// The captures a workflow's views come from, in order: those `used` admits.
pub(super) fn used_views(
    obs: &Observations,
    used: impl Fn(&ObservedCapture) -> bool,
) -> Vec<usize> {
    (0..obs.captures.len())
        .filter(|&k| used(&obs.captures[k]))
        .collect()
}

/// The views of camera `i`, as planar correspondences.
pub(super) fn planar_views(obs: &Observations, i: usize) -> (Vec<CorrespondenceView>, usize) {
    let views: Vec<CorrespondenceView> = obs
        .captures
        .iter()
        .filter_map(|c| correspondences(&c.views[i]))
        .collect();
    let points = views.iter().map(CorrespondenceView::len).sum();
    (views, points)
}

/// Checks that hold whatever the procedure: the observations are the truth's.
pub(crate) fn check_observations(gt: &GroundTruth, obs: &Observations) -> Result<()> {
    ensure!(
        obs.captures.len() == gt.captures.len()
            && obs.captures.iter().zip(&gt.captures).all(|(o, g)| {
                o.id == g.id
                    && o.views
                        .iter()
                        .map(|v| &v.camera)
                        .eq(g.views.iter().map(|v| &v.camera))
            }),
        "the observations do not match the ground truth's captures and cameras"
    );
    Ok(())
}

/// The robot poses of the captures (a hand-eye stage needs them).
pub(crate) fn robot_poses(obs: &Observations) -> Result<Vec<Isometry3<f64>>> {
    obs.captures
        .iter()
        .map(|c| c.base_se3_tcp.context("a capture has no robot pose"))
        .collect()
}
