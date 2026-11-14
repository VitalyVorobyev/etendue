//! The planner on real poses (P20c): the P18 planner's predicted σ for a calibration's own pose
//! set, against the σ the calibration reports and against a split-half estimate of its scatter.
//!
//! - **Planner σ:** `etendue_synth::plan::predict` on the twin (the planner's model of the cell):
//!   the twin's views and the noise learnt from the real run ([`learnt_model`], P20c2): its pixel
//!   noise, its view scatter as a per-view term, and its robot pose error (without the
//!   view-correlated pixel error) as the prior the robot pose corrections are weighed with.
//!   P20c's model (the run's robot error, white views) and P18's (robot poses exact) are reported
//!   beside it.
//! - **Real σ:** calibration-rs's covariance of the calibration of all the real views with the
//!   same procedure (scaled by its view scatter), per parameter, and taken through the same
//!   sightings for the system σ.
//! - **Split-half σ:** the views are split into two halves several ways ([`splits`]) and each
//!   half is calibrated on its own. For split `s` the difference of the halves' estimates, `d_s`
//!   (a parameter's, or the system difference, `etendue_synth::plan::system_difference`), has the
//!   predicted variance `p_s² = σ_A² + σ_B²` (the planner's σ of each half). Pooled over the
//!   splits, `ρ = √(Σ d_s² / Σ p_s²)` is the measured scatter over the predicted one, and the
//!   split-half σ of the whole set is `ρ · σ`. With equal halves, `σ_A = σ_B ≈ √2 σ`, so `d_s` is
//!   compared with `√2 σ_half ≈ 2σ`. The splits share the data, so they are not independent: the
//!   pooling steadies the estimate against one split's luck, not its noise draw.
//!
//! **Gate P1:** the planner's system σ is within 25 % of the split-half σ ([`P1`]).

mod calibrations;
mod judge;

use std::path::Path;

use anyhow::Result;
use etendue_synth::dataset::GroundTruth;
use etendue_synth::observe::Observations;
use etendue_synth::plan::{NoiseModel, RobotPrior};
use serde::{Deserialize, Serialize};

use super::{Twin, twin};
use crate::calibrate::RunProcedure;
use crate::progress::Control;
pub use calibrations::{SplitCalibrations, calibrate_splits};
pub use judge::judge;

/// Gate P1's bar: the planner's system σ within this fraction of the split-half σ.
pub const P1: f64 = 0.25;

/// How to split and calibrate.
#[derive(Clone, Debug, PartialEq)]
pub struct SplitHalfOptions {
    /// Random splits beside the interleaved one and the halves in capture order.
    pub random: usize,
    /// The first random split's seed.
    pub seed: u64,
    /// The procedure every calibration runs; `None` for the planner's model
    /// ([`planner_procedure`]).
    pub procedure: Option<RunProcedure>,
}

impl Default for SplitHalfOptions {
    fn default() -> Self {
        Self {
            random: 8,
            seed: 0,
            procedure: None,
        }
    }
}

/// The calibration the planner models: for a rig, one `rig_handeye` stage with the cameras' rig
/// poses refined (`docs/measurements/g17_sim_lab.md`: the default holds them, and its covariance
/// is conditional on them); for one camera, `auto`'s (`None`), which holds nothing.
///
/// # Panics
///
/// Never: the procedure is a constant.
#[must_use]
pub fn planner_procedure(cameras: usize) -> Option<RunProcedure> {
    (cameras > 1).then(|| RunProcedure {
        id: "rig_handeye_refine_rig".into(),
        procedure: serde_json::from_value(serde_json::json!({
            "stages": [{"id": "rig_handeye", "problem": "rig_handeye", "free": {"refine_rig": true}}]
        }))
        .expect("a valid procedure"),
    })
}

/// One split's system difference, measured and predicted.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SplitRow {
    pub split: String,
    /// Views in each half.
    pub views: [usize; 2],
    /// The halves' system difference (mm).
    pub measured_mm: f64,
    /// Its predicted std, `√(σ_A² + σ_B²)` (mm).
    pub predicted_mm: f64,
}

/// One parameter: the planner's σ, the real calibration's, and the split-half σ.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParamRow {
    /// As `calibrate`'s parameter statistics name it (`cam0 fx`, `hand-eye rx`).
    pub label: String,
    /// `px`, `deg`, `mm`, or empty.
    pub unit: String,
    pub planner: f64,
    /// calibration-rs's σ of the calibration of every view, if it states one.
    pub real: Option<f64>,
    pub split_half: f64,
}

/// The planner's σ against the real calibration's and the split-half σ.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SplitHalf {
    /// The procedure of every calibration.
    pub procedure: String,
    pub views: usize,
    /// The noise the planner assumes.
    pub model: NoiseModel,
    /// The model the learnt one was learnt from (P20c's: the calibration's robot error, white
    /// views), if given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub white_model: Option<NoiseModel>,
    /// The planner's system σ of all the views (mm).
    pub planner_mm: f64,
    /// The system σ under [`Self::white_model`] (mm).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planner_white_mm: Option<f64>,
    /// The same with exact robot poses: P18's model as it was (mm).
    pub planner_exact_robot_mm: f64,
    /// The real calibration's covariance through the same sightings (mm).
    pub real_mm: Option<f64>,
    /// calibration-rs's view scatter of the real calibration (its covariance is scaled by it).
    pub view_scatter: Option<f64>,
    /// The split-half σ of all the views (mm).
    pub split_half_mm: f64,
    pub splits: Vec<SplitRow>,
    /// Splits a half of which did not calibrate, with why.
    pub failed: Vec<String>,
    pub params: Vec<ParamRow>,
}

impl SplitHalf {
    /// The planner's system σ over the split-half σ.
    #[must_use]
    pub fn ratio(&self) -> f64 {
        self.planner_mm / self.split_half_mm
    }

    /// Gate P1: [`Self::ratio`] within [`P1`] of 1.
    #[must_use]
    pub fn pass(&self) -> bool {
        (self.ratio() - 1.0).abs() <= P1
    }

    /// The parameters by group, every camera's together: focal lengths, principal points,
    /// distortion, the hand-eye's rotation and translation.
    #[must_use]
    pub fn groups(&self) -> Vec<GroupRow> {
        let groups: [(&str, &[&str]); 5] = [
            ("focal length (fx, fy)", &["fx", "fy"]),
            ("principal point (cx, cy)", &["cx", "cy"]),
            ("distortion (k1, k2, p1, p2)", &["k1", "k2", "p1", "p2"]),
            (
                "hand-eye rotation",
                &["hand-eye rx", "hand-eye ry", "hand-eye rz"],
            ),
            (
                "hand-eye translation",
                &["hand-eye tx", "hand-eye ty", "hand-eye tz"],
            ),
        ];
        let rms = |v: &[f64]| (v.iter().map(|x| x * x).sum::<f64>() / v.len() as f64).sqrt();
        groups
            .iter()
            .filter_map(|(name, suffixes)| {
                let member = |l: &str| {
                    suffixes
                        .iter()
                        .any(|x| l.ends_with(&format!(" {x}")) || l == *x)
                };
                let rows: Vec<&ParamRow> =
                    self.params.iter().filter(|p| member(&p.label)).collect();
                let first = rows.first()?;
                let same_unit = !first.unit.is_empty() && rows.iter().all(|r| r.unit == first.unit);
                let of = |f: &dyn Fn(&ParamRow) -> Option<f64>| -> Option<Vec<f64>> {
                    rows.iter().map(|r| f(r)).collect()
                };
                let sigma = |f: &dyn Fn(&ParamRow) -> Option<f64>| {
                    same_unit.then(|| of(f).map(|v| rms(&v))).flatten()
                };
                Some(GroupRow {
                    group: (*name).to_owned(),
                    unit: if same_unit {
                        first.unit.clone()
                    } else {
                        String::new()
                    },
                    parameters: rows.len(),
                    planner: sigma(&|r| Some(r.planner)),
                    real: sigma(&|r| r.real),
                    split_half: sigma(&|r| Some(r.split_half)),
                    real_ratio: of(&|r| r.real.map(|s| s / r.planner)).map(|v| rms(&v)),
                    split_ratio: rms(&rows
                        .iter()
                        .map(|r| r.split_half / r.planner)
                        .collect::<Vec<_>>()),
                })
            })
            .collect()
    }
}

/// A group of parameters: RMS over its members of each σ (when they share a unit), and of each
/// one's ratio to the planner's σ.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupRow {
    pub group: String,
    /// Empty when the members' units differ (distortion).
    pub unit: String,
    pub parameters: usize,
    pub planner: Option<f64>,
    pub real: Option<f64>,
    pub split_half: Option<f64>,
    /// Real σ over the planner's.
    pub real_ratio: Option<f64>,
    /// Split-half σ over the planner's.
    pub split_ratio: f64,
}

/// The split-half check of the real run kept in `run` (`etendue real calibrate`'s output), on its
/// twin. The planner's noise is learnt from the real run ([`learnt_model`]); P20c's model (the
/// run's own robot error, white views) is reported beside it.
///
/// # Errors
///
/// As [`twin`] and [`split_half`].
pub fn split_half_run(
    run: &Path,
    options: &SplitHalfOptions,
    ctl: &Control,
) -> Result<(Twin, SplitHalf)> {
    let twin = twin(run)?;
    let (white, model) = learnt_model(&twin)?;
    let cal = calibrate_splits(&twin.gt, &twin.observed, options, ctl)?;
    let result = judge(&twin.gt, &cal, &model, Some(&white))?;
    Ok((twin, result))
}

/// The real run's noise as the planner takes it: P20c's model (its pixel σ and its robot pose
/// error, white views), and the model learnt from it (P20c2, `NoiseModel::learnt`): the run's
/// view scatter as the per-view term, and its robot error without the view-correlated pixel
/// error.
///
/// # Errors
///
/// A twin the planner cannot model.
pub fn learnt_model(twin: &Twin) -> Result<(NoiseModel, NoiseModel)> {
    let n = &twin.noise;
    let robot =
        (n.robot_rot_sigma_rad > 0.0 && n.robot_trans_sigma_m > 0.0).then_some(RobotPrior {
            rot_sigma_rad: n.robot_rot_sigma_rad,
            trans_sigma_m: n.robot_trans_sigma_m,
        });
    let white = NoiseModel {
        robot,
        ..NoiseModel::white(n.pixel_sigma_px)
    };
    let model = NoiseModel::learnt(&twin.gt, n.pixel_sigma_px, robot, twin.view_scatter)?;
    Ok((white, model))
}

/// The split-half check of `obs`, observations of `gt`'s captures: every view calibrated, then each
/// split's halves ([`calibrate_splits`]), against the planner's prediction under `model`
/// ([`judge`]).
///
/// # Errors
///
/// A ground truth the planner cannot model, a calibration of every view that fails, or no split
/// whose halves both calibrate.
pub fn split_half(
    gt: &GroundTruth,
    obs: &Observations,
    model: &NoiseModel,
    options: &SplitHalfOptions,
    ctl: &Control,
) -> Result<SplitHalf> {
    judge(gt, &calibrate_splits(gt, obs, options, ctl)?, model, None)
}
