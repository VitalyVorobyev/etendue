//! The system error of an estimated cell: what the planner predicts the σ of.

use etendue_kinematics::RobotModel;
use etendue_scene::{SceneSpec, SphereGenerator};
use nalgebra::Isometry3;
use vision_calibration_core::CameraParams;

use super::bad;
use super::information::{pixel, ref_se3_base};
use crate::board::layout;
use crate::gt::{VisibilitySpec, project_points};
use crate::poses::{CandidateStatus, Generated, Posed, mounting};
use crate::{PixelCentre, Result};

/// An estimated cell, as a calibration reports it: each camera's parameters and pose in the rig
/// (`cam_se3_ref`, the first camera's frame), and the hand-eye (`gripper_se3_ref` eye-in-hand,
/// `ref_se3_base` eye-to-hand).
#[derive(Clone, Debug)]
pub struct EstimatedCell {
    /// Per camera (in `sphere.require.cameras` order, or the scene's): its parameters and
    /// `cam_se3_ref`.
    pub cameras: Vec<(CameraParams, Isometry3<f64>)>,
    /// `gripper_se3_ref` eye-in-hand, `ref_se3_base` eye-to-hand.
    pub handeye: Isometry3<f64>,
}

/// The system error of `estimate`: the RMS, over every sighting of the target's points by every
/// feasible candidate, of how far (mm, across the line of sight) the estimated cell places a
/// point known in the base frame — the quantity [`plan`] predicts the σ of.
///
/// # Errors
///
/// As [`plan`], and for an estimate with another number of cameras.
pub fn system_error(
    scene: &SceneSpec,
    robots: &[RobotModel],
    generated: &Generated,
    sphere: &SphereGenerator,
    estimate: &EstimatedCell,
) -> Result<f64> {
    let posed = Posed::new(scene, robots)?;
    let r = scene
        .robots
        .iter()
        .position(|x| x.id == sphere.robot)
        .ok_or_else(|| bad(format!("no robot `{}`", sphere.robot)))?;
    let model = robots
        .get(r)
        .ok_or_else(|| bad("a robot model per scene robot"))?;
    let names = model.links();
    let base_frame = posed.index(&format!("{}/{}", sphere.robot, names[model.base_link()]))?;
    let tcp_frame = posed.index(&format!("{}/{}", sphere.robot, names[model.tcp_link()]))?;
    let target = scene
        .targets
        .iter()
        .find(|t| t.id == sphere.target)
        .ok_or_else(|| bad(format!("no target `{}`", sphere.target)))?;
    let target_frame = posed.index(&target.id)?;
    let mounting = mounting(&posed, r, model, &sphere.look, &target.id)?;
    let board = layout(&target.geometry)?
        .ok_or_else(|| bad(format!("target `{}` has no printed layout", target.id)))?;
    let ids: Vec<String> = match &sphere.require.cameras {
        Some(c) => c.clone(),
        None => scene.cameras.iter().map(|c| c.id.clone()).collect(),
    };
    if ids.len() != estimate.cameras.len() {
        return Err(bad("the estimate has another number of cameras"));
    }
    let truth = ids
        .iter()
        .map(|id| {
            let c = scene
                .cameras
                .iter()
                .find(|c| &c.id == id)
                .ok_or_else(|| bad(format!("no camera `{id}`")))?;
            Ok((
                posed.index(id)?,
                c.params.build()?,
                c.params.intrinsics.fx,
                c.resolution,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let estimated = estimate
        .cameras
        .iter()
        .map(|(p, pose)| Ok((p.build()?, *pose)))
        .collect::<Result<Vec<_>>>()?;
    let visibility = VisibilitySpec {
        margin_px: sphere.require.margin_px,
    };
    let edge = PixelCentre::Integer.edge();
    let (mut sum, mut count) = (0.0, 0usize);
    for c in generated
        .candidates
        .iter()
        .filter(|c| c.status == CandidateStatus::Ok && c.capture.is_some())
    {
        let f = posed.at(
            r,
            c.q.as_deref()
                .ok_or_else(|| bad("a feasible candidate has no joints"))?,
        );
        let base_se3_tcp = f[base_frame].inverse() * f[tcp_frame];
        let base_se3_target = f[base_frame].inverse() * f[target_frame];
        for ((frame, true_model, fx, resolution), (model, cam_se3_ref)) in
            truth.iter().zip(&estimated)
        {
            let seen = project_points(
                true_model,
                *resolution,
                &f[*frame],
                &f[target_frame],
                &board.points,
                &visibility,
                edge,
            );
            let true_chain = f[*frame].inverse() * f[target_frame];
            let est_chain = cam_se3_ref
                * ref_se3_base(mounting, &estimate.handeye, &base_se3_tcp)
                * base_se3_target;
            for (gt, b) in seen.iter().zip(&board.points) {
                let (true_px, Some(est_px)) = (gt.pixel, pixel(model, &est_chain, &b.xyz())) else {
                    continue;
                };
                let Some(true_px) = true_px.filter(|_| gt.visible()) else {
                    continue;
                };
                let depth = (true_chain * b.xyz()).z;
                let du = (est_px[0] - true_px[0], est_px[1] - true_px[1]);
                let mm = depth / fx * 1e3;
                // Per image axis, as the predicted σ² is a mean over pixel rows.
                sum += (du.0 * mm).powi(2) + (du.1 * mm).powi(2);
                count += 2;
            }
        }
    }
    if count == 0 {
        return Err(bad("no candidate sees the target"));
    }
    Ok((sum / count as f64).sqrt())
}
