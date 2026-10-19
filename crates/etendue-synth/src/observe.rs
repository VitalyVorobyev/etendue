//! Observations synthesised from a dataset's ground truth (P13): what a detector and a robot
//! controller would report, with controlled noise.
//!
//! [`observe`] walks the ground truth's captures, then their views, then each view's points in
//! order, and draws from one ChaCha8 stream seeded by [`ObservationNoise::seed`]. A given truth
//! and noise therefore always give the same observations, on every platform (as `sensor` does
//! for images).
//!
//! Per target point that the camera sees:
//! - **dropout**: the detector misses it, with probability `dropout_fraction`;
//! - **outlier**: with probability `outlier_fraction`, it is displaced by `N(0, outlier_sigma_px)`
//!   per axis (a mislabelled or misdetected corner);
//! - otherwise **pixel noise**: `N(0, pixel_sigma_px)` per axis.
//!
//! A laser stripe's points get `N(0, laser_sigma_px)` per axis. The robot's reported
//! `base_se3_tcp` is perturbed by a rotation drawn as a rotation vector with
//! `N(0, robot_rot_sigma_rad)` per component, and a translation with `N(0, robot_trans_sigma_m)`
//! per component. A view keeps its target points only with at least four of them, and its
//! stripe only with at least two.

use nalgebra::{Isometry3, Quaternion, Translation3, UnitQuaternion, Vector3};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use rand_distr::{Distribution, Normal, Uniform};
use serde::{Deserialize, Serialize};

use crate::dataset::GroundTruth;

/// The noise of synthesised observations. Every field defaults to zero: exact observations.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ObservationNoise {
    /// Target-point pixel noise, per axis (px).
    pub pixel_sigma_px: f64,
    /// Share of seen target points that are outliers, in `[0, 1]`.
    pub outlier_fraction: f64,
    /// An outlier's displacement, per axis (px).
    pub outlier_sigma_px: f64,
    /// Share of seen target points the detector misses, in `[0, 1]`.
    pub dropout_fraction: f64,
    /// Laser-stripe pixel noise, per axis (px).
    pub laser_sigma_px: f64,
    /// The robot's reported TCP rotation error, per rotation-vector component (rad).
    pub robot_rot_sigma_rad: f64,
    /// The robot's reported TCP translation error, per axis (m).
    pub robot_trans_sigma_m: f64,
    /// The stream's seed.
    pub seed: u64,
}

impl ObservationNoise {
    /// Every problem with these values, as messages.
    #[must_use]
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (name, v) in [
            ("pixel_sigma_px", self.pixel_sigma_px),
            ("outlier_sigma_px", self.outlier_sigma_px),
            ("laser_sigma_px", self.laser_sigma_px),
            ("robot_rot_sigma_rad", self.robot_rot_sigma_rad),
            ("robot_trans_sigma_m", self.robot_trans_sigma_m),
        ] {
            if !(v.is_finite() && v >= 0.0) {
                out.push(format!("{name} must be finite and ≥ 0"));
            }
        }
        for (name, v) in [
            ("outlier_fraction", self.outlier_fraction),
            ("dropout_fraction", self.dropout_fraction),
        ] {
            if !(0.0..=1.0).contains(&v) {
                out.push(format!("{name} must be in [0, 1]"));
            }
        }
        out
    }
}

/// One observed target point.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObservedPoint {
    /// Index into the target's points.
    pub point: usize,
    /// The point on the target, in its frame (m).
    pub target_xyz: [f64; 3],
    /// Where it was observed (px).
    pub pixel: [f64; 2],
    /// Whether it was made an outlier.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub outlier: bool,
}

/// One camera's observations in one capture.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObservedView {
    pub camera: String,
    /// The target points, or none when fewer than four were seen.
    pub points: Vec<ObservedPoint>,
    /// The laser stripe's pixels, or none when fewer than two were seen.
    pub laser: Vec<[f64; 2]>,
}

/// One capture's observations.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObservedCapture {
    pub id: String,
    /// The robot's reported TCP, if a robot carries the rig or the target.
    pub base_se3_tcp: Option<Isometry3<f64>>,
    /// One per camera of the ground truth, in its order.
    pub views: Vec<ObservedView>,
}

/// Observations of a dataset.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Observations {
    pub noise: ObservationNoise,
    pub captures: Vec<ObservedCapture>,
}

/// The fewest target points a view needs.
pub const MIN_VIEW_POINTS: usize = 4;
/// The fewest stripe points a view needs.
pub const MIN_STRIPE_POINTS: usize = 2;

/// Observations of `gt` with `noise`.
///
/// # Panics
///
/// If `noise` has a problem ([`ObservationNoise::problems`]).
#[must_use]
pub fn observe(gt: &GroundTruth, noise: &ObservationNoise) -> Observations {
    assert!(noise.problems().is_empty(), "{:?}", noise.problems());
    let mut rng = ChaCha8Rng::seed_from_u64(noise.seed);
    let unit = Uniform::new(0.0, 1.0).expect("a unit interval");
    let gauss = |sigma: f64| Normal::new(0.0, sigma).expect("σ is finite and ≥ 0");
    let pixel = gauss(noise.pixel_sigma_px);
    let outlier = gauss(noise.outlier_sigma_px);
    let laser = gauss(noise.laser_sigma_px);
    let rot = gauss(noise.robot_rot_sigma_rad);
    let trans = gauss(noise.robot_trans_sigma_m);
    let mut captures = Vec::with_capacity(gt.captures.len());
    for capture in &gt.captures {
        let base_se3_tcp = capture.base_se3_tcp.map(|p| {
            let r = Vector3::from_fn(|_, _| rot.sample(&mut rng));
            let t = Vector3::from_fn(|_, _| trans.sample(&mut rng));
            Isometry3::from_parts(
                Translation3::from(p.translation.vector + t),
                rotation_vector(&r) * p.rotation,
            )
        });
        let mut views = Vec::with_capacity(capture.views.len());
        for view in &capture.views {
            let mut points = Vec::new();
            for p in &view.points {
                let Some(px) = p.pixel.filter(|_| p.visible()) else {
                    continue;
                };
                if unit.sample(&mut rng) < noise.dropout_fraction {
                    continue;
                }
                let is_outlier = unit.sample(&mut rng) < noise.outlier_fraction;
                let d = if is_outlier { &outlier } else { &pixel };
                let du = d.sample(&mut rng);
                let dv = d.sample(&mut rng);
                let b = &gt.target.points[p.point];
                points.push(ObservedPoint {
                    point: p.point,
                    target_xyz: b.xyz_m.unwrap_or([b.position_m[0], b.position_m[1], 0.0]),
                    pixel: [px[0] + du, px[1] + dv],
                    outlier: is_outlier,
                });
            }
            if points.len() < MIN_VIEW_POINTS {
                points.clear();
            }
            // The camera's own laser (the only one, if it names no camera).
            let own = gt
                .lasers
                .iter()
                .find(|l| l.camera.as_deref() == Some(view.camera.as_str()))
                .or_else(|| (gt.lasers.len() == 1).then(|| &gt.lasers[0]));
            let mut stripe = Vec::new();
            for s in view
                .laser_points
                .iter()
                .filter(|s| own.is_some_and(|l| l.id == s.laser))
            {
                for p in &s.points {
                    let Some(px) = p.pixel.filter(|_| p.visible()) else {
                        continue;
                    };
                    let du = laser.sample(&mut rng);
                    let dv = laser.sample(&mut rng);
                    stripe.push([px[0] + du, px[1] + dv]);
                }
            }
            if stripe.len() < MIN_STRIPE_POINTS {
                stripe.clear();
            }
            views.push(ObservedView {
                camera: view.camera.clone(),
                points,
                laser: stripe,
            });
        }
        captures.push(ObservedCapture {
            id: capture.id.clone(),
            base_se3_tcp,
            views,
        });
    }
    Observations {
        noise: *noise,
        captures,
    }
}

/// The rotation of rotation vector `r` (axis times angle), with `libm`'s transcendentals.
fn rotation_vector(r: &Vector3<f64>) -> UnitQuaternion<f64> {
    let angle = r.norm();
    if angle == 0.0 {
        return UnitQuaternion::identity();
    }
    let s = libm::sin(0.5 * angle) / angle;
    UnitQuaternion::new_unchecked(Quaternion::new(
        libm::cos(0.5 * angle),
        r.x * s,
        r.y * s,
        r.z * s,
    ))
}
