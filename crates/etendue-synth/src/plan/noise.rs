//! The noise the planner's prediction for a ground truth's captures ([`super::predict`]) assumes,
//! and how it is learnt from a real calibration (P20c2).

use serde::{Deserialize, Serialize};

use super::bad;
use super::information::RobotPrior;
use super::predict::predict;
use crate::Result;
use crate::dataset::GroundTruth;

/// What the planner assumes about a calibration's errors.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoiseModel {
    /// White pixel noise, σ per image axis (px).
    pub pixel_sigma_px: f64,
    /// The robot pose error the calibration refines each view's robot pose under; `None` for
    /// exact robot poses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub robot: Option<RobotPrior>,
    /// The view-correlated error (P20c2): how many times more a view's errors scatter the
    /// estimate than the white model above predicts, `k ≥ 1`. Each view's information (its pixels
    /// and its robot prior together) is divided by `k`, so every σ grows by `√k`. It is learnt
    /// from a real calibration as calibration-rs's view scatter: the delete-one-view jackknife of
    /// its shared parameters over their white-noise covariance, which calibration-rs scales its
    /// own covariance by. 1 (the default) is the white model, and leaves every prediction as it
    /// was, bit for bit.
    #[serde(
        default = "NoiseModel::white_views",
        skip_serializing_if = "NoiseModel::is_white"
    )]
    pub view_scatter: f64,
}

impl NoiseModel {
    /// White pixel noise of `pixel_sigma_px` and exact robot poses: P18's model.
    #[must_use]
    pub fn white(pixel_sigma_px: f64) -> Self {
        Self {
            pixel_sigma_px,
            robot: None,
            view_scatter: 1.0,
        }
    }

    fn white_views() -> f64 {
        1.0
    }

    #[allow(clippy::trivially_copy_pass_by_ref)] // serde's `skip_serializing_if` passes a reference
    fn is_white(k: &f64) -> bool {
        *k == 1.0
    }

    /// The noise model a real calibration of `gt`'s captures implies (P20c2), from what it
    /// estimated: its pixel noise σ, the robot pose error it weighed its pose corrections with
    /// (`robot`, calibration-rs's `robot_pose_sigma`; `None` when it took the poses as exact),
    /// and its view scatter (`None` or below 1: the views agree with the white model).
    ///
    /// The view scatter becomes the per-view term. The robot error is calibration-rs's estimate
    /// with the view-correlated pixel error taken out of it
    /// ([`super::Prediction::robot_prior_under_view_scatter`]): its estimate counts each pose
    /// correction's pixel part at its white size, and with view-correlated residuals that part is
    /// `k` times larger. With no view scatter the model is the calibration's own, unchanged.
    ///
    /// # Errors
    ///
    /// As [`predict`], for a non-positive pixel noise or a ground truth the planner cannot model.
    pub fn learnt(
        gt: &GroundTruth,
        pixel_sigma_px: f64,
        robot: Option<RobotPrior>,
        view_scatter: Option<f64>,
    ) -> Result<Self> {
        let white = Self {
            robot,
            ..Self::white(pixel_sigma_px)
        };
        let k = match view_scatter {
            Some(k) if k.is_finite() && k > 1.0 => k,
            _ => return Ok(white),
        };
        let robot = match robot {
            Some(_) => predict(gt, &white)?.robot_prior_under_view_scatter(k),
            None => None,
        };
        Ok(Self {
            robot,
            view_scatter: k,
            ..white
        })
    }

    pub(super) fn validate(&self) -> Result<()> {
        if !(self.pixel_sigma_px.is_finite() && self.pixel_sigma_px > 0.0) {
            return Err(bad("the pixel noise σ must be positive"));
        }
        if !(self.view_scatter.is_finite() && self.view_scatter >= 1.0) {
            return Err(bad("the view scatter must be at least 1"));
        }
        Ok(())
    }
}
