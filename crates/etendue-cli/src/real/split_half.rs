//! The planner on real poses (P20c): the P18 planner's predicted σ for a calibration's own pose
//! set, against the σ the calibration reports and against a split-half estimate of its scatter.
//!
//! - **Planner σ:** `etendue_synth::plan::predict` on the twin (the planner's model of the cell):
//!   the twin's views, the real run's pixel noise, and its robot pose error as the prior the robot
//!   pose corrections are weighed with. The P18 model as it was (robot poses exact) is reported
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

use std::path::Path;

use anyhow::{Context, Result, ensure};
use etendue_synth::dataset::GroundTruth;
use etendue_synth::observe::Observations;
use etendue_synth::plan::{Prediction, RobotPrior, predict, system_difference};
use serde::{Deserialize, Serialize};

use super::splits::{Split, splits};
use super::{Twin, twin};
use crate::calibrate::{Report, RunOptions, RunProcedure, cell, covariance, run_trials, values};
use crate::progress::Control;

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
    /// The pixel noise σ the planner assumes (px).
    pub noise_px: f64,
    /// The robot pose error the planner marginalises (`None`: exact poses).
    pub robot: Option<RobotPrior>,
    /// The planner's system σ of all the views (mm).
    pub planner_mm: f64,
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
/// twin: the planner's noise is the real run's ([`Twin::noise`]).
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
    let n = &twin.noise;
    let robot =
        (n.robot_rot_sigma_rad > 0.0 && n.robot_trans_sigma_m > 0.0).then_some(RobotPrior {
            rot_sigma_rad: n.robot_rot_sigma_rad,
            trans_sigma_m: n.robot_trans_sigma_m,
        });
    let result = split_half(
        &twin.gt,
        &twin.observed,
        n.pixel_sigma_px,
        robot,
        options,
        ctl,
    )?;
    Ok((twin, result))
}

/// The split-half check of `obs`, observations of `gt`'s captures: every view calibrated, then each
/// split's halves, against the planner's prediction at `noise_px` (and `robot`).
///
/// # Errors
///
/// A ground truth the planner cannot model, a calibration of every view that fails, or no split
/// whose halves both calibrate.
pub fn split_half(
    gt: &GroundTruth,
    obs: &Observations,
    noise_px: f64,
    robot: Option<RobotPrior>,
    options: &SplitHalfOptions,
    ctl: &Control,
) -> Result<SplitHalf> {
    let procedure = options
        .procedure
        .clone()
        .or_else(|| planner_procedure(gt.cameras.len()));
    let run_options = RunOptions {
        detected: true,
        trials: 1,
        procedure: procedure.clone(),
        ..RunOptions::default()
    };
    let calibrate = |keep: &[usize]| -> Result<Report> {
        let (g, o) = subset(gt, obs, keep);
        let mut run = run_trials(&g, Some(o), &run_options, ctl)?;
        Ok(run.reports.remove(0))
    };
    let views = gt.captures.len();
    let all: Vec<usize> = (0..views).collect();
    let full = calibrate(&all).context("calibrating every view")?;
    let prediction = predict(gt, noise_px, robot)?;
    let exact = predict(gt, noise_px, None)?;
    let sigma = |p: &Prediction, set: &[usize]| {
        p.system_sigma_mm(set)
            .context("the views cannot determine the calibration")
    };
    let planner_mm = sigma(&prediction, &all)?;
    let planner_exact_robot_mm = sigma(&exact, &all)?;
    let real = covariance(&full);
    let real_mm = real
        .as_ref()
        .map(|(labels, cov, _)| prediction.system_sigma_of(labels, cov));
    let view_scatter = real.as_ref().and_then(|(.., s)| *s);

    // The planner's σ per parameter, in the statistics' labels and units.
    let labelled = |s: Vec<f64>| -> Vec<(String, f64)> {
        prediction
            .labels()
            .iter()
            .zip(s)
            .filter_map(|(l, s)| stats_label(gt, l).map(|(label, k)| (label, s * k)))
            .collect()
    };
    let planner_params = labelled(prediction.sigma(&all).context("no covariance")?);

    let mut rows = Vec::new();
    let mut failed = Vec::new();
    // Per parameter label: Σ d², Σ p².
    let mut sums: Vec<(f64, f64)> = vec![(0.0, 0.0); planner_params.len()];
    for Split { name, a, b } in splits(views, options.random, options.seed) {
        let halves = calibrate(&a).and_then(|ra| Ok((ra, calibrate(&b)?)));
        let (ra, rb) = match halves {
            Ok(r) => r,
            Err(e) => {
                failed.push(format!("{name}: {e:#}"));
                continue;
            }
        };
        let (pa, pb) = (sigma(&prediction, &a)?, sigma(&prediction, &b)?);
        rows.push(SplitRow {
            split: name,
            views: [a.len(), b.len()],
            measured_mm: system_difference(gt, &cell(gt, &ra)?, &cell(gt, &rb)?)?,
            predicted_mm: pa.hypot(pb),
        });
        let (sa, sb) = (
            labelled(prediction.sigma(&a).context("no covariance")?),
            labelled(prediction.sigma(&b).context("no covariance")?),
        );
        let (va, vb) = (values(gt, &ra), values(gt, &rb));
        let value = |v: &[(String, &'static str, f64, Option<f64>)], label: &str| {
            v.iter().find(|x| x.0 == label).map(|x| x.2)
        };
        for (k, (label, _)) in planner_params.iter().enumerate() {
            if let (Some(x), Some(y)) = (value(&va, label), value(&vb, label)) {
                sums[k].0 += (x - y).powi(2);
                sums[k].1 += sa[k].1.powi(2) + sb[k].1.powi(2);
            }
        }
    }
    ensure!(
        !rows.is_empty(),
        "no split's halves both calibrate: {}",
        failed.join("; ")
    );
    let pooled = |d: f64, p: f64| (d / p).sqrt();
    let rho = pooled(
        rows.iter().map(|r| r.measured_mm.powi(2)).sum(),
        rows.iter().map(|r| r.predicted_mm.powi(2)).sum(),
    );
    let real_values = values(gt, &full);
    let params = planner_params
        .into_iter()
        .zip(sums)
        .filter(|(_, (_, p))| *p > 0.0)
        .map(|((label, planner), (d, p))| {
            let found = real_values.iter().find(|x| x.0 == label);
            ParamRow {
                unit: found.map_or("", |x| x.1).to_owned(),
                real: found.and_then(|x| x.3),
                split_half: pooled(d, p) * planner,
                planner,
                label,
            }
        })
        .collect();
    Ok(SplitHalf {
        procedure: full.procedure.clone(),
        views,
        noise_px,
        robot,
        planner_mm,
        planner_exact_robot_mm,
        real_mm,
        view_scatter,
        split_half_mm: rho * planner_mm,
        splits: rows,
        failed,
        params,
    })
}

/// `gt` and `obs` with the captures `keep` only.
fn subset(gt: &GroundTruth, obs: &Observations, keep: &[usize]) -> (GroundTruth, Observations) {
    let mut g = gt.clone();
    g.captures = keep.iter().map(|&k| gt.captures[k].clone()).collect();
    let mut o = obs.clone();
    o.captures = keep.iter().map(|&k| obs.captures[k].clone()).collect();
    (g, o)
}

/// A planner label as the parameter statistics name it, with the factor to their unit: a camera's
/// `cam0.fx` is `<id> fx` (a tilt in degrees), the hand-eye's `gripper_se3_rig.rx` is
/// `hand-eye rx` (degrees; millimetres for `tx`…). The rig and the target have none.
fn stats_label(gt: &GroundTruth, label: &str) -> Option<(String, f64)> {
    let (head, tail) = label.rsplit_once('.')?;
    if let Some(j) = head
        .strip_prefix("cam")
        .and_then(|j| j.parse::<usize>().ok())
    {
        let k = if tail.starts_with("tilt") {
            1f64.to_degrees()
        } else {
            1.0
        };
        return Some((format!("{} {tail}", gt.cameras.get(j)?.id), k));
    }
    if head == "gripper_se3_rig" || head == "rig_se3_base" {
        let k = if tail.starts_with('r') {
            1f64.to_degrees()
        } else {
            1e3
        };
        return Some((format!("hand-eye {tail}"), k));
    }
    None
}
