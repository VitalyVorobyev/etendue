//! A set of split calibrations judged against the planner's prediction for one noise model.

use anyhow::{Context, Result, ensure};
use etendue_synth::dataset::GroundTruth;
use etendue_synth::plan::{NoiseModel, Prediction, predict, system_difference};

use super::calibrations::SplitCalibrations;
use super::{ParamRow, SplitHalf, SplitRow};
use crate::calibrate::{cell, covariance, values};

/// The split-half check of `cal` (calibrations of `gt`'s captures and of halves of them) against
/// the planner's prediction under `model`; with `white`, that model's system σ is given beside it
/// (P20c's: the calibration's own robot error, white views).
///
/// # Errors
///
/// A ground truth the planner cannot model, or no split whose halves both calibrated.
pub fn judge(
    gt: &GroundTruth,
    cal: &SplitCalibrations,
    model: &NoiseModel,
    white: Option<&NoiseModel>,
) -> Result<SplitHalf> {
    ensure!(
        !cal.splits.is_empty(),
        "no split's halves both calibrate: {}",
        cal.failed.join("; ")
    );
    let views = gt.captures.len();
    let all: Vec<usize> = (0..views).collect();
    let prediction = predict(gt, model)?;
    let exact = predict(gt, &NoiseModel::white(model.pixel_sigma_px))?;
    let sigma = |p: &Prediction, set: &[usize]| {
        p.system_sigma_mm(set)
            .context("the views cannot determine the calibration")
    };
    let planner_mm = sigma(&prediction, &all)?;
    let planner_exact_robot_mm = sigma(&exact, &all)?;
    let planner_white_mm = match white {
        Some(w) => Some(sigma(&predict(gt, w)?, &all)?),
        None => None,
    };
    let real = covariance(&cal.full);
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
    // Per parameter label: Σ d², Σ p².
    let mut sums: Vec<(f64, f64)> = vec![(0.0, 0.0); planner_params.len()];
    for (split, ra, rb) in &cal.splits {
        let (a, b) = (&split.a, &split.b);
        let (pa, pb) = (sigma(&prediction, a)?, sigma(&prediction, b)?);
        rows.push(SplitRow {
            split: split.name.clone(),
            views: [a.len(), b.len()],
            measured_mm: system_difference(gt, &cell(gt, ra)?, &cell(gt, rb)?)?,
            predicted_mm: pa.hypot(pb),
        });
        let (sa, sb) = (
            labelled(prediction.sigma(a).context("no covariance")?),
            labelled(prediction.sigma(b).context("no covariance")?),
        );
        let (va, vb) = (values(gt, ra), values(gt, rb));
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
    let pooled = |d: f64, p: f64| (d / p).sqrt();
    let rho = pooled(
        rows.iter().map(|r| r.measured_mm.powi(2)).sum(),
        rows.iter().map(|r| r.predicted_mm.powi(2)).sum(),
    );
    let real_values = values(gt, &cal.full);
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
        procedure: cal.full.procedure.clone(),
        views,
        model: *model,
        white_model: white.copied(),
        planner_mm,
        planner_white_mm,
        planner_exact_robot_mm,
        real_mm,
        view_scatter,
        split_half_mm: rho * planner_mm,
        splits: rows,
        failed: cal.failed.clone(),
        params,
    })
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
