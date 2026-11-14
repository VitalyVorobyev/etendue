//! A report's estimate as the planner sees it (`etendue_synth::plan`): the estimated cell, and
//! calibration-rs's full covariance of it.

use anyhow::{Context, Result};
use etendue_synth::dataset::GroundTruth;
use etendue_synth::plan::EstimatedCell;
use nalgebra::DMatrix;
use vision_calibration_core::{BrownConrady5, DistortionParams, FxFyCxCy, SensorParams};

use super::report::Report;

/// The estimated cell `report` gives, in the planner's terms: each camera's model (`gt`'s, with
/// the estimated intrinsics, distortion and tilt) and `cam_se3_ref`, in `gt`'s camera order, and
/// the hand-eye.
///
/// # Errors
///
/// A report without a hand-eye, or of a camera `gt` does not have.
pub fn cell(gt: &GroundTruth, report: &Report) -> Result<EstimatedCell> {
    let cameras = gt
        .cameras
        .iter()
        .map(|truth| {
            let e = report
                .estimate
                .cameras
                .iter()
                .find(|e| e.id == truth.id)
                .with_context(|| format!("the report has no camera `{}`", truth.id))?;
            let mut p = truth.params.clone();
            let [fx, fy, cx, cy] = e.intrinsics;
            p.intrinsics = FxFyCxCy { fx, fy, cx, cy };
            let [k1, k2, k3, p1, p2] = e.distortion;
            p.distortion = DistortionParams::BrownConrady5 {
                params: BrownConrady5 {
                    k1,
                    k2,
                    k3,
                    p1,
                    p2,
                    iters: 8,
                },
            };
            if let SensorParams::Scheimpflug { params } = &mut p.sensor {
                params.tilt_x = e.tilt[0];
                params.tilt_y = e.tilt[1];
            }
            Ok((p, e.cam_se3_ref))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(EstimatedCell {
        cameras,
        handeye: report
            .estimate
            .handeye
            .context("the report has no hand-eye")?,
    })
}

/// calibration-rs's full covariance of `report`'s estimate, from its export: the labels in the
/// planner's names (`cam0.fx`, `gripper_se3_rig.rx`, `rig_se3_base.tx`, …: a single camera's
/// bare `fx` and `gripper_se3_camera` become the first camera's and the rig's), the matrix, and
/// its view scatter (how many times the parameters vary across views more than the residual noise
/// predicts; the covariance is scaled by it). `None` when the export has none.
#[must_use]
pub fn covariance(report: &Report) -> Option<(Vec<String>, DMatrix<f64>, Option<f64>)> {
    let u = report.export.get("uncertainty")?;
    let labels: Vec<String> = u
        .get("labels")?
        .as_array()?
        .iter()
        .map(|l| l.as_str().map(planner_label))
        .collect::<Option<_>>()?;
    let rows = u.get("covariance")?.as_array()?;
    let n = labels.len();
    if rows.len() != n {
        return None;
    }
    let mut m = DMatrix::<f64>::zeros(n, n);
    for (i, row) in rows.iter().enumerate() {
        let row = row.as_array()?;
        if row.len() != n {
            return None;
        }
        for (j, v) in row.iter().enumerate() {
            m[(i, j)] = v.as_f64()?;
        }
    }
    let scatter = u.get("view_scatter").and_then(serde_json::Value::as_f64);
    Some((labels, m, scatter))
}

/// A calibration-rs label in the planner's names.
fn planner_label(label: &str) -> String {
    let label = label
        .replace("gripper_se3_camera.", "gripper_se3_rig.")
        .replace("camera_se3_base.", "rig_se3_base.");
    if label.contains('.') {
        label
    } else {
        format!("cam0.{label}")
    }
}

#[cfg(test)]
mod tests {
    use super::planner_label;

    #[test]
    fn a_single_cameras_labels_are_the_first_cameras_and_the_rigs() {
        assert_eq!(planner_label("fx"), "cam0.fx");
        assert_eq!(planner_label("cam1.cx"), "cam1.cx");
        assert_eq!(planner_label("gripper_se3_camera.rx"), "gripper_se3_rig.rx");
        assert_eq!(planner_label("camera_se3_base.tz"), "rig_se3_base.tz");
        assert_eq!(planner_label("base_se3_target.ty"), "base_se3_target.ty");
    }
}
