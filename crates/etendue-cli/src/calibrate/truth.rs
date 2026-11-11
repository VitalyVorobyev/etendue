//! The ground truth in calibration-rs's frames: its rig frame is the reference camera's.

use anyhow::{Result, bail};
use etendue_synth::dataset::{GroundTruth, GtCamera, GtHandeye, GtLaser};
use etendue_synth::laser::GtPlane;
use nalgebra::Isometry3;
use vision_calibration_core::{CameraParams, DistortionParams, SensorParams};

/// The truth's frames, mapped onto calibration-rs's: its rig frame is the reference camera's.
pub(crate) struct Truth<'a> {
    pub(crate) gt: &'a GroundTruth,
    /// `cam_se3_rig` per camera (identity without a rig).
    pub(crate) cam_se3_rig: Vec<Isometry3<f64>>,
}

impl<'a> Truth<'a> {
    pub(crate) fn new(gt: &'a GroundTruth) -> Self {
        Self {
            gt,
            cam_se3_rig: gt
                .cameras
                .iter()
                .map(|c| c.cam_se3_rig.unwrap_or_else(Isometry3::identity))
                .collect(),
        }
    }

    pub(crate) fn eye_in_hand(&self) -> Result<bool> {
        match &self.gt.handeye {
            Some(GtHandeye::EyeInHand { .. }) => Ok(true),
            Some(GtHandeye::EyeToHand { .. }) => Ok(false),
            None => bail!("the dataset has no robot: there is no hand-eye to calibrate"),
        }
    }

    /// The hand-eye transform of the frame `ref_se3_rig` maps into (the rig, a camera of it).
    pub(crate) fn handeye(&self, ref_se3_rig: &Isometry3<f64>) -> Isometry3<f64> {
        match self.gt.handeye.as_ref().expect("checked") {
            GtHandeye::EyeInHand {
                gripper_se3_rig, ..
            } => gripper_se3_rig * ref_se3_rig.inverse(),
            GtHandeye::EyeToHand { rig_se3_base, .. } => ref_se3_rig * rig_se3_base,
        }
    }

    /// The target in the frame the workflow reports it in: the base eye-in-hand, the gripper
    /// eye-to-hand.
    pub(crate) fn target(&self) -> (&'static str, Isometry3<f64>) {
        match self.gt.handeye.as_ref().expect("checked") {
            GtHandeye::EyeInHand {
                base_se3_target, ..
            } => ("base_se3_target", *base_se3_target),
            GtHandeye::EyeToHand {
                gripper_se3_target, ..
            } => ("gripper_se3_target", *gripper_se3_target),
        }
    }

    pub(crate) fn scheimpflug(&self, cameras: &[usize]) -> bool {
        cameras.iter().any(|&i| {
            matches!(
                self.gt.cameras[i].params.sensor,
                SensorParams::Scheimpflug { .. }
            )
        })
    }

    /// Camera `i`'s own laser (the only laser, if none names it).
    pub(crate) fn laser(&self, i: usize) -> Option<&GtLaser> {
        let id = &self.gt.cameras[i].id;
        self.gt
            .lasers
            .iter()
            .find(|l| l.camera.as_deref() == Some(id.as_str()))
            .or_else(|| (self.gt.lasers.len() == 1).then(|| &self.gt.lasers[0]))
    }

    /// The truth's plane of camera `i`'s own laser.
    pub(crate) fn plane(&self, i: usize) -> Option<&GtPlane> {
        let id = &self.gt.cameras[i].id;
        self.laser(i)?.camera_planes.iter().find(|p| &p.frame == id)
    }
}

/// The values a user would enter for a Scheimpflug rig from datasheets, as the script makes them:
/// per camera, the focal length rounded to 100 px with the principal point at the image centre,
/// and the tilt about x rounded to a whole degree.
pub(crate) fn nominal_values(cameras: &[&GtCamera]) -> (Vec<[f64; 4]>, Vec<[f64; 2]>) {
    cameras
        .iter()
        .map(|c| {
            let k = &c.params.intrinsics;
            let f = ((k.fx + k.fy) / 2.0 / 100.0).round_ties_even() * 100.0;
            let [w, h] = c.resolution.map(f64::from);
            let tilt = tilt_of(&c.params)[0];
            (
                [f, f, w / 2.0, h / 2.0],
                [tilt.to_degrees().round_ties_even().to_radians(), 0.0],
            )
        })
        .unzip()
}

/// A camera's Brown-Conrady coefficients `[k1, k2, k3, p1, p2]` (zero without distortion);
/// `None` for another distortion model, which etendue does not calibrate.
pub(crate) fn distortion_of(p: &CameraParams) -> Option<[f64; 5]> {
    match &p.distortion {
        DistortionParams::BrownConrady5 { params: d } => Some([d.k1, d.k2, d.k3, d.p1, d.p2]),
        DistortionParams::None => Some([0.0; 5]),
        _ => None,
    }
}

/// A camera's Scheimpflug tilt `[tilt_x, tilt_y]`, zero for an untilted sensor.
pub(crate) fn tilt_of(p: &CameraParams) -> [f64; 2] {
    match p.sensor {
        SensorParams::Scheimpflug { params } => [params.tilt_x, params.tilt_y],
        _ => [0.0, 0.0],
    }
}
