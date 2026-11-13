//! The planner's prediction for a ground truth's own captures (P20c): a pose set that was not
//! planned, such as a real calibration's twin ([`crate::twin`]), whose captures are its real
//! robot poses and whose views are exactly the points its detector found.
//!
//! The model is [`super::plan`]'s (the rig hand-eye with the rig refined), over the captures'
//! seen points; the system σ averages over every sighting of every capture. With a
//! [`RobotPrior`], each view's robot pose is refined under it, as calibration-rs does by default.

use nalgebra::{DMatrix, Isometry3, Point3};

use super::bad;
use super::information::{Cam, Information, Layout, RobotPrior, View, view_information};
use crate::Result;
use crate::dataset::{GroundTruth, GtHandeye};
use crate::poses::Mounting;

/// The planner's information for each capture of a ground truth.
pub struct Prediction {
    layout: Layout,
    information: Information,
}

/// The truth in the planner's frames (the rig is the first camera's): each camera's
/// `cam_se3_ref`, the hand-eye (`gripper_se3_ref` or `ref_se3_base`), the target's pose (in the
/// base, or on the gripper) and the mounting.
type Frames = (
    Vec<Isometry3<f64>>,
    Isometry3<f64>,
    Isometry3<f64>,
    Mounting,
);

/// `gt`'s [`Frames`].
pub(super) fn frames(gt: &GroundTruth) -> Result<Frames> {
    let cam_se3_rig: Vec<Isometry3<f64>> = gt
        .cameras
        .iter()
        .map(|c| c.cam_se3_rig.unwrap_or_else(Isometry3::identity))
        .collect();
    let ref_se3_rig = *cam_se3_rig
        .first()
        .ok_or_else(|| bad("the ground truth has no camera"))?;
    let cam_se3_ref = cam_se3_rig
        .iter()
        .map(|c| c * ref_se3_rig.inverse())
        .collect();
    Ok(match gt.handeye.as_ref() {
        Some(GtHandeye::EyeInHand {
            gripper_se3_rig,
            base_se3_target,
            ..
        }) => (
            cam_se3_ref,
            gripper_se3_rig * ref_se3_rig.inverse(),
            *base_se3_target,
            Mounting::EyeInHand,
        ),
        Some(GtHandeye::EyeToHand {
            rig_se3_base,
            gripper_se3_target,
            ..
        }) => (
            cam_se3_ref,
            ref_se3_rig * rig_se3_base,
            *gripper_se3_target,
            Mounting::EyeToHand,
        ),
        None => return Err(bad("the ground truth has no robot: there is no hand-eye")),
    })
}

/// The planner's prediction for `gt`'s captures, observed with `noise_px` per image axis and, with
/// `robot`, robot poses refined under its prior (exact without).
///
/// # Errors
///
/// [`crate::Error::InvalidInput`] for a ground truth without a hand-eye or a camera, a capture
/// without its robot pose, a view of an unknown camera, a non-positive noise, or no sighting.
pub fn predict(gt: &GroundTruth, noise_px: f64, robot: Option<RobotPrior>) -> Result<Prediction> {
    if !(noise_px.is_finite() && noise_px > 0.0) {
        return Err(bad("noise_px must be positive"));
    }
    let (cam_se3_ref, handeye, target, mounting) = frames(gt)?;
    let cams = gt
        .cameras
        .iter()
        .map(|c| Cam::new(&c.params))
        .collect::<Result<Vec<_>>>()?;
    let layout = Layout::new(&cams, mounting);
    let n = layout.len();
    let mut a = DMatrix::<f64>::zeros(n, n);
    let mut infos = Vec::with_capacity(gt.captures.len());
    let mut rows = 0;
    for capture in &gt.captures {
        let base_se3_tcp = capture
            .base_se3_tcp
            .ok_or_else(|| bad(format!("capture {} has no robot pose", capture.id)))?;
        let mut seen: Vec<Vec<Point3<f64>>> = vec![Vec::new(); cams.len()];
        for v in &capture.views {
            let c = gt
                .cameras
                .iter()
                .position(|c| c.id == v.camera)
                .ok_or_else(|| bad(format!("no camera `{}`", v.camera)))?;
            seen[c].extend(
                v.points
                    .iter()
                    .filter(|p| p.visible())
                    .map(|p| gt.target.points[p.point].xyz()),
            );
        }
        let view = View {
            cameras: cam_se3_ref.iter().copied().zip(seen).collect(),
            handeye,
            target,
            base_se3_tcp,
        };
        let (info, r) = view_information(
            &cams,
            &layout,
            mounting,
            &view,
            noise_px * noise_px,
            robot,
            &mut a,
        );
        infos.push(info);
        rows += r;
    }
    if rows == 0 {
        return Err(bad("no capture sees the target"));
    }
    let information = Information::new(infos, a, rows, layout.target_offset);
    Ok(Prediction {
        layout,
        information,
    })
}

impl Prediction {
    /// The parameters' labels, as calibration-rs names them (`cam0.fx`, `gripper_se3_rig.rx`, …;
    /// px, radians, metres).
    #[must_use]
    pub fn labels(&self) -> &[String] {
        &self.layout.labels
    }

    /// The captures predicted for.
    #[must_use]
    pub fn captures(&self) -> usize {
        self.information.infos.len()
    }

    /// The system σ (mm) of a calibration from `captures` (indices into the ground truth's),
    /// averaged over every sighting of every capture; `None` when they cannot determine it.
    #[must_use]
    pub fn system_sigma_mm(&self, captures: &[usize]) -> Option<f64> {
        let info = self.information.total(captures);
        self.information.sigma_of(&info).map(|(s, _)| s)
    }

    /// Each parameter's σ (in [`Self::labels`]' order) from `captures`; `None` when they cannot
    /// determine the calibration.
    #[must_use]
    pub fn sigma(&self, captures: &[usize]) -> Option<Vec<f64>> {
        let info = self.information.total(captures);
        let (_, cov) = self.information.sigma_of(&info)?;
        Some(
            (0..cov.nrows())
                .map(|x| cov[(x, x)].max(0.0).sqrt())
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::twin::{TwinCamera, TwinView, twin};
    use nalgebra::{Translation3, UnitQuaternion, Vector3};
    use vision_calibration_core::{
        BrownConrady5, CameraParams, DistortionParams, FxFyCxCy, ProjectionParams, SensorParams,
    };
    use vision_calibration_dataset::Topology;

    /// One camera 0.6 m above a 9 × 9 board, sixteen tilted views.
    fn truth() -> GroundTruth {
        let camera = TwinCamera {
            id: "a".into(),
            params: CameraParams {
                projection: ProjectionParams::Pinhole,
                distortion: DistortionParams::BrownConrady5 {
                    params: BrownConrady5 {
                        k1: -0.1,
                        k2: 0.02,
                        k3: 0.0,
                        p1: 1e-4,
                        p2: -2e-4,
                        iters: 8,
                    },
                },
                sensor: SensorParams::Identity,
                intrinsics: FxFyCxCy {
                    fx: 1000.0,
                    fy: 1001.0,
                    cx: 640.0,
                    cy: 480.0,
                },
            },
            resolution: [1280, 960],
            cam_se3_rig: Isometry3::identity(),
        };
        let handeye = GtHandeye::EyeInHand {
            robot: "real".into(),
            tcp_link: "tcp".into(),
            gripper_se3_rig: Isometry3::from_parts(
                Translation3::new(0.0, 0.05, 0.1),
                UnitQuaternion::from_scaled_axis(Vector3::new(0.0, 0.0, 0.3)),
            ),
            base_se3_target: Isometry3::from_parts(
                Translation3::new(-0.08, -0.08, 0.0),
                UnitQuaternion::from_scaled_axis(Vector3::new(std::f64::consts::PI, 0.0, 0.0)),
            ),
        };
        let board: Vec<[f64; 3]> = (0..81)
            .map(|i| [0.02 * (i % 9) as f64, 0.02 * (i / 9) as f64, 0.0])
            .collect();
        let views: Vec<TwinView> = (0..16)
            .map(|k| {
                let a = std::f64::consts::TAU * f64::from(k) / 8.0;
                let r = if k < 8 { 0.3 } else { 0.15 };
                let tilt = Vector3::new(r * a.cos(), r * a.sin(), 0.2 * f64::from(k % 3));
                TwinView {
                    base_se3_tcp: Isometry3::from_parts(
                        Translation3::new(0.1 * a.sin(), -0.1 * a.cos(), -0.7),
                        UnitQuaternion::from_scaled_axis(tilt),
                    ),
                    points: vec![Some(board.clone())],
                }
            })
            .collect();
        twin(Topology::SingleCamHandeye, &[camera], handeye, &views).unwrap()
    }

    #[test]
    fn halves_and_robot_error_raise_the_system_sigma() {
        let gt = truth();
        let all: Vec<usize> = (0..16).collect();
        let exact = predict(&gt, 0.5, None).unwrap();
        assert_eq!(exact.captures(), 16);
        assert_eq!(exact.labels().len(), 8 + 6 + 6);
        let full = exact.system_sigma_mm(&all).unwrap();
        assert!(full > 0.0 && full.is_finite(), "{full}");
        // Half the views (a less varied half): at least √2 the σ.
        let even: Vec<usize> = (0..16).step_by(2).collect();
        let half = exact.system_sigma_mm(&even).unwrap();
        assert!((1.4..2.0).contains(&(half / full)), "{}", half / full);
        // A negligible robot pose error changes nothing; a real one raises the σ.
        let tiny = RobotPrior {
            rot_sigma_rad: 1e-12,
            trans_sigma_m: 1e-12,
        };
        let s = predict(&gt, 0.5, Some(tiny))
            .unwrap()
            .system_sigma_mm(&all)
            .unwrap();
        assert!((s / full - 1.0).abs() < 1e-6, "{}", s / full);
        let robot = RobotPrior {
            rot_sigma_rad: 1e-3,
            trans_sigma_m: 1e-3,
        };
        let s = predict(&gt, 0.5, Some(robot))
            .unwrap()
            .system_sigma_mm(&all)
            .unwrap();
        assert!(s > 1.5 * full, "{} against {full}", s);
        // σ scales with the pixel noise.
        let twice = predict(&gt, 1.0, None)
            .unwrap()
            .system_sigma_mm(&all)
            .unwrap();
        assert!((twice / full - 2.0).abs() < 1e-9);
    }

    #[test]
    fn two_estimates_differ_by_their_difference() {
        let gt = truth();
        let (cam_se3_ref, handeye, ..) = frames(&gt).unwrap();
        let cell = |dx: f64| super::super::EstimatedCell {
            cameras: vec![(gt.cameras[0].params.clone(), cam_se3_ref[0])],
            handeye: handeye * Isometry3::translation(dx, 0.0, 0.0),
        };
        let same = super::super::system_difference(&gt, &cell(0.0), &cell(0.0)).unwrap();
        assert!(same.abs() < 1e-12, "{same}");
        // A 1 mm shift of the hand-eye across the line of sight: about 1 mm on one axis, so
        // 1/√2 mm per axis.
        let d = super::super::system_difference(&gt, &cell(0.0), &cell(1e-3)).unwrap();
        assert!((d - 0.5f64.sqrt()).abs() < 0.05, "{d}");
    }
}
