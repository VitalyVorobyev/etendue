//! The twin of a real calibration (P20b): the same cell, robot poses and seen points, simulated.
//!
//! [`twin`] reads a run `etendue real calibrate` kept and builds its [`GroundTruth`] with
//! `etendue_synth::twin`:
//! - the truth is the run's result (calibration-rs's export): cameras, rig and hand-eye;
//! - the views are calibration-rs's dataset runner's input, rebuilt from the run's detection
//!   cache: the robot poses as the calibration took them, and each view's points as the real
//!   detector found them (the same views, cameras and feature order);
//! - the noise is the level the real run implies: calibration-rs's reprojection noise σ, and the
//!   robot's pose error it weighed the pose corrections with (estimated, by default).
//!
//! The twin is calibrated by the trial loop of [`crate::calibrate`] with `auto`'s procedure, which
//! for a hand-eye dataset is the workflow the runner picks for the manifest with its default
//! configuration: the real run's.

use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use etendue_synth::dataset::{GroundTruth, GtHandeye};
use etendue_synth::observe::ObservationNoise;
use etendue_synth::twin::{TwinCamera, TwinView};
use nalgebra::Isometry3;
use vision_calibration_core::{CorrespondenceView, RunControl};
use vision_calibration_dataset::{DatasetSpec, Topology};
use vision_calibration_detect::FsDetectionCache;
use vision_calibration_pipeline::common::HandeyeSolution;
use vision_calibration_pipeline::dataset_runner::{
    Detection, build_rig_handeye_input, build_single_cam_handeye_input, camera_image_sizes,
};
use vision_calibration_pipeline::rig_handeye::RigHandeyeExport;
use vision_calibration_pipeline::single_cam_handeye::{
    SingleCamHandeyeExport, SingleCamHandeyeSolution,
};

use super::{DETECTIONS, DatasetSource};
use crate::calibrate::{RUN_CONFIG, RUN_REPORT, RUN_RESULT, Report, Run, RunConfig, predicted};

/// A real run's twin: its truth, and the noise its observations get.
#[derive(Clone, Debug)]
pub struct Twin {
    /// The real dataset.
    pub source: DatasetSource,
    /// The real run's report (its first, and only, trial).
    pub real: Report,
    /// The twin's ground truth.
    pub gt: GroundTruth,
    /// The real run's noise: calibration-rs's reprojection σ and its robot pose error (`seed` 0).
    pub noise: ObservationNoise,
    /// calibration-rs's view scatter of the real run: how many times the variance of its
    /// parameters across views exceeds what its residual noise predicts (its σ is scaled by it);
    /// `None` when they agree.
    pub view_scatter: Option<f64>,
}

/// The twin of the real run kept in `run` (`etendue real calibrate`'s output folder). The
/// manifest and its images must still be where the run found them: the runner's input is rebuilt
/// from the run's detection cache, which is keyed by the images.
///
/// # Errors
///
/// A folder that is not a real run, a topology without a robot (no hand-eye to twin), an
/// unreadable manifest or export, a detection the cache no longer matches, or views that differ
/// from the run's.
pub fn twin(run: &Path) -> Result<Twin> {
    let config: RunConfig = crate::read_json(&run.join(RUN_CONFIG), "run config")?;
    let Some(source) = config.real else {
        bail!(
            "{} is not a real dataset's run (`etendue real calibrate` keeps one)",
            run.display()
        );
    };
    let kept: Run = crate::read_json(&run.join(RUN_REPORT), "run report")?;
    let real = kept
        .reports
        .into_iter()
        .next()
        .context("the run has no report")?;
    let summary = real
        .real
        .as_ref()
        .context("the run's report is not a real one")?;
    let spec = DatasetSpec::from_path(source.manifest())
        .with_context(|| format!("reading {}", source.manifest))?;
    let base = source.base_dir();
    let sizes = camera_image_sizes(&spec, &base)
        .with_context(|| format!("reading the images of {}", source.name()))?;
    let cache = FsDetectionCache::new(run.join(DETECTIONS));
    let detection = Detection::Cached(&cache);
    let control = RunControl::default();
    let result = run.join(RUN_RESULT);
    let ids: Vec<String> = spec.cameras.iter().map(|c| c.id.clone()).collect();
    let (cameras, handeye, views, uncertainty, robot) = match spec.topology {
        Topology::SingleCamHandeye => {
            let e: SingleCamHandeyeExport = crate::read_json(&result, "calibration result")?;
            let input = build_single_cam_handeye_input(&spec, &base, detection, &control)?.input;
            let views = input
                .views
                .iter()
                .map(|v| TwinView {
                    base_se3_tcp: v.meta.base_se3_gripper,
                    points: vec![Some(points(&v.obs))],
                })
                .collect::<Vec<_>>();
            let handeye = match e.handeye {
                SingleCamHandeyeSolution::EyeInHand {
                    gripper_se3_camera,
                    base_se3_target,
                } => eye_in_hand(gripper_se3_camera, base_se3_target),
                SingleCamHandeyeSolution::EyeToHand {
                    camera_se3_base,
                    gripper_se3_target,
                } => eye_to_hand(camera_se3_base, gripper_se3_target),
            };
            let cameras = vec![camera(&ids[0], e.camera, sizes[0], Isometry3::identity())];
            let robot = e.robot_deltas.and(e.robot_pose_sigma);
            (cameras, handeye, views, e.uncertainty, robot)
        }
        Topology::RigHandeye => {
            let e: RigHandeyeExport = crate::read_json(&result, "calibration result")?;
            let input = build_rig_handeye_input(&spec, &base, detection, &control)?.input;
            let views = input
                .views
                .iter()
                .map(|v| TwinView {
                    base_se3_tcp: v.meta.base_se3_gripper,
                    points: v
                        .obs
                        .cameras
                        .iter()
                        .map(|c| c.as_ref().map(points))
                        .collect(),
                })
                .collect::<Vec<_>>();
            let handeye = match e.handeye {
                HandeyeSolution::EyeInHand {
                    gripper_se3_rig,
                    base_se3_target,
                } => eye_in_hand(gripper_se3_rig, base_se3_target),
                HandeyeSolution::EyeToHand {
                    rig_se3_base,
                    gripper_se3_target,
                } => eye_to_hand(rig_se3_base, gripper_se3_target),
            };
            ensure!(
                e.cameras.len() == ids.len() && e.cam_se3_rig.len() == ids.len(),
                "the result has {} cameras for a manifest of {}",
                e.cameras.len(),
                ids.len()
            );
            let cameras = e
                .cameras
                .into_iter()
                .zip(e.cam_se3_rig)
                .enumerate()
                .map(|(i, (p, x))| camera(&ids[i], p, sizes[i], x))
                .collect();
            let robot = e.robot_deltas.and(e.robot_pose_sigma);
            (cameras, handeye, views, e.uncertainty, robot)
        }
        other => bail!(
            "a {other:?} dataset has no robot: the twin is of a hand-eye calibration \
             (single_cam_handeye, rig_handeye)"
        ),
    };
    ensure!(
        views.len() == summary.usable_views,
        "the detection cache gives {} views, the run calibrated {}: the images changed since",
        views.len(),
        summary.usable_views
    );
    let pixel = predicted(&uncertainty)
        .and_then(|p| p.noise_px)
        .context("the result states no reprojection noise σ (in pixels)")?;
    let noise = ObservationNoise {
        pixel_sigma_px: pixel,
        robot_rot_sigma_rad: robot.as_ref().map_or(0.0, |r| r.rot_sigma),
        robot_trans_sigma_m: robot.as_ref().map_or(0.0, |r| r.trans_sigma),
        ..ObservationNoise::default()
    };
    let gt = etendue_synth::twin::twin(spec.topology, &cameras, handeye, &views)?;
    Ok(Twin {
        source,
        real,
        gt,
        noise,
        view_scatter: uncertainty.covariance().and_then(|c| c.view_scatter),
    })
}

/// A view's target points, in the detector's order.
fn points(obs: &CorrespondenceView) -> Vec<[f64; 3]> {
    obs.points_3d.iter().map(|p| [p.x, p.y, p.z]).collect()
}

fn camera(
    id: &str,
    params: vision_calibration_core::CameraParams,
    resolution: [u32; 2],
    cam_se3_rig: Isometry3<f64>,
) -> TwinCamera {
    TwinCamera {
        id: id.to_owned(),
        params,
        resolution,
        cam_se3_rig,
    }
}

/// A real cell's robot is known by no model: the hand-eye names its link generically.
const ROBOT: &str = "real";
const TCP: &str = "tcp";

fn eye_in_hand(gripper_se3_rig: Isometry3<f64>, base_se3_target: Isometry3<f64>) -> GtHandeye {
    GtHandeye::EyeInHand {
        robot: ROBOT.into(),
        tcp_link: TCP.into(),
        gripper_se3_rig,
        base_se3_target,
    }
}

fn eye_to_hand(rig_se3_base: Isometry3<f64>, gripper_se3_target: Isometry3<f64>) -> GtHandeye {
    GtHandeye::EyeToHand {
        robot: ROBOT.into(),
        tcp_link: TCP.into(),
        rig_se3_base,
        gripper_se3_target,
    }
}
