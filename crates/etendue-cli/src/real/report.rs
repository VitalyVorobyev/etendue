//! The report of a real dataset's calibration: [`Report`]'s no-truth form. No rows, no gate, no
//! offsets; residuals per feature and per view, calibration-rs's σ and quality verdict, the views
//! the runner left out, and with lasers the planes and each camera's stripe residuals.

use std::collections::BTreeMap;

use anyhow::{Result, bail};
use nalgebra::Isometry3;
use serde::{Deserialize, Serialize};
use vision_calibration_dataset::DatasetSpec;
use vision_calibration_pipeline::common::{CalibratedCameras, HandeyeSolution};
use vision_calibration_pipeline::dataset_runner::{CalibrationExport, DatasetCalibration};
use vision_calibration_pipeline::single_cam_handeye::SingleCamHandeyeSolution;

use super::{LaserResiduals, laser_residuals};
use crate::calibrate::{
    Estimate, FeatureResidual, REPORT_VERSION, Report, Residuals, Workflow, camera_of, exported,
    plane_of, predicted, residuals,
};

/// What a real dataset's calibration has that a synthetic one's does not: the manifest's view
/// counts, the views left out, and residuals per view.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RealSummary {
    /// The manifest's topology (calibration-rs's `snake_case` name).
    pub topology: String,
    /// Views that entered the calibration.
    pub usable_views: usize,
    /// Views the manifest's images formed before screening.
    pub total_views: usize,
    /// Each detection the runner left out, with its reason.
    pub rejected: Vec<RejectedView>,
    /// Residuals of each calibrated view and camera, by view then camera.
    pub views: Vec<ViewResiduals>,
    /// A laser workflow's stripe residuals, per camera with laser pixels.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lasers: Vec<LaserResiduals>,
}

/// A detection the dataset runner left out of the calibration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RejectedView {
    /// The camera, by id.
    pub camera: String,
    /// The image, relative to the manifest's folder when it is under it.
    pub image: String,
    /// calibration-rs's reason.
    pub reason: String,
}

/// The reprojection residuals of one camera in one calibrated view.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ViewResiduals {
    /// calibration-rs's view index (the usable views, in pairing order).
    pub view: usize,
    /// The camera, by id.
    pub camera: String,
    /// Points with a residual.
    pub count: usize,
    /// Mean Euclidean error (px).
    pub mean_px: f64,
    /// Root mean square per image axis (px).
    pub rms_axis_px: f64,
    /// Largest Euclidean error (px).
    pub max_px: f64,
}

/// [`Report`]'s no-truth form for `run`, a calibration of the dataset `spec` describes, whose
/// relative paths are read from `base_dir`.
///
/// # Errors
///
/// An export or a camera model etendue does not read.
pub fn report(
    spec: &DatasetSpec,
    base_dir: &std::path::Path,
    run: &DatasetCalibration,
) -> Result<Report> {
    let ids: Vec<String> = spec.cameras.iter().map(|c| c.id.clone()).collect();
    let id = |i: usize| ids.get(i).cloned().unwrap_or_else(|| format!("camera{i}"));
    let (workflow, mean_px, handeye) = match &run.export {
        CalibrationExport::PlanarIntrinsics(e) => {
            (Workflow::PlanarIntrinsics, e.mean_reproj_error, None)
        }
        CalibrationExport::SingleCamHandeye(e) => (
            Workflow::SingleCamHandeye,
            e.mean_reproj_error,
            Some(match e.handeye {
                SingleCamHandeyeSolution::EyeInHand {
                    gripper_se3_camera, ..
                } => gripper_se3_camera,
                SingleCamHandeyeSolution::EyeToHand {
                    camera_se3_base, ..
                } => camera_se3_base,
            }),
        ),
        CalibrationExport::RigExtrinsics(e) => (Workflow::RigExtrinsics, e.mean_reproj_error, None),
        CalibrationExport::RigHandeye(e) => (
            Workflow::RigHandeye,
            e.mean_reproj_error,
            Some(match e.handeye {
                HandeyeSolution::EyeInHand {
                    gripper_se3_rig, ..
                } => gripper_se3_rig,
                HandeyeSolution::EyeToHand { rig_se3_base, .. } => rig_se3_base,
            }),
        ),
        CalibrationExport::LaserlineDevice(e) => {
            (Workflow::LaserlineDevice, e.mean_reproj_error, None)
        }
        CalibrationExport::RigLaserlineDevice(e) => {
            (Workflow::RigLaserlineDevice, e.mean_reproj_error, None)
        }
        CalibrationExport::RigHandeyeLaserline(e) => (
            Workflow::RigHandeyeLaserline,
            e.mean_reproj_error,
            Some(match e.handeye {
                HandeyeSolution::EyeInHand {
                    gripper_se3_rig, ..
                } => gripper_se3_rig,
                HandeyeSolution::EyeToHand { rig_se3_base, .. } => rig_se3_base,
            }),
        ),
        other => bail!("a {:?} export, which etendue does not read", other.kind()),
    };
    let export = &run.export;
    let params = export.camera_params();
    let cam_se3_rig = export.cam_se3_rig();
    let cameras = params
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let pose = cam_se3_rig
                .get(i)
                .copied()
                .unwrap_or_else(Isometry3::identity);
            camera_of(p, &id(i), pose)
        })
        .collect::<Result<Vec<_>>>()?;
    let mut residuals = residuals(export.diagnostics(), mean_px, params.len());
    let views = per_view(&residuals.features, &residuals.errors_px, &id);
    name_features(&mut residuals, &id);
    let (export_json, quality) = exported(export, &run.image_sizes)?;
    let lasers = laser_residuals(export.diagnostics(), params.len(), &id);
    let laser_pixels = lasers.iter().map(|l| l.pixels).sum();
    let planes = export.laser_planes_cam();
    let laser_planes = (0..cameras.len())
        .map(|i| planes.get(i).map(plane_of))
        .collect();
    let summary = RealSummary {
        topology: serde_json::to_value(spec.topology)?
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        usable_views: run.usable_views,
        total_views: run.total_views,
        rejected: run
            .rejected
            .iter()
            .map(|r| RejectedView {
                camera: id(r.camera),
                image: r
                    .path
                    .strip_prefix(base_dir)
                    .unwrap_or(&r.path)
                    .to_string_lossy()
                    .into_owned(),
                reason: r.reason.to_string(),
            })
            .collect(),
        views,
        lasers,
    };
    Ok(Report {
        version: REPORT_VERSION,
        workflow,
        gate: String::new(),
        gated: false,
        pass: true,
        views: run.usable_views,
        points: residuals.count,
        laser_pixels,
        noise: None,
        rows: Vec::new(),
        estimate: Estimate {
            laser_planes,
            cameras,
            handeye,
            offsets: None,
        },
        cameras: ids.iter().take(params.len()).cloned().collect(),
        residuals,
        quality,
        uncertainty: predicted(export.parameter_uncertainty()),
        procedure: String::new(),
        stages: Vec::new(),
        export: export_json,
        rig_se3_target: Vec::new(),
        real: Some(summary),
    })
}

/// Each residual's names: its view index (calibration-rs's), its camera's id, and its feature's
/// index in the view.
fn name_features(residuals: &mut Residuals, id: &impl Fn(usize) -> String) {
    for f in &mut residuals.features {
        let [view, cam, feature] = f.raw;
        f.capture = view.to_string();
        f.camera = id(cam);
        f.point = feature;
        f.raw = [0; 3];
    }
}

/// The residuals of each (view, camera), by view then camera. `errors` are the features'
/// Euclidean errors, in the same order.
fn per_view(
    features: &[FeatureResidual],
    errors: &[f64],
    id: &impl Fn(usize) -> String,
) -> Vec<ViewResiduals> {
    // (view, camera) → (errors, Σ du² + dv²)
    let mut groups: BTreeMap<(usize, usize), (Vec<f64>, f64)> = BTreeMap::new();
    for (f, &e) in features.iter().zip(errors) {
        let [view, cam, _] = f.raw;
        let (du, dv) = (
            f.projected_px[0] - f.observed_px[0],
            f.projected_px[1] - f.observed_px[1],
        );
        let g = groups.entry((view, cam)).or_default();
        g.0.push(e);
        g.1 += du * du + dv * dv;
    }
    groups
        .into_iter()
        .map(|((view, cam), (errors, sq))| {
            let n = errors.len() as f64;
            ViewResiduals {
                view,
                camera: id(cam),
                count: errors.len(),
                mean_px: errors.iter().sum::<f64>() / n,
                rms_axis_px: (sq / (2.0 * n)).sqrt(),
                max_px: errors.iter().copied().fold(0.0, f64::max),
            }
        })
        .collect()
}
