//! Statistics over trials: each row's spread, and each parameter's against its truth and
//! calibration-rs's predicted σ.

use std::collections::BTreeMap;

use etendue_synth::dataset::GroundTruth;
use nalgebra::Vector3;
use serde::{Deserialize, Serialize};
use vision_calibration_core::SensorParams;

use super::report::Report;
use super::truth::Truth;

/// Statistics of a row over trials.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RowStats {
    pub label: String,
    pub mean: f64,
    pub std: f64,
    pub max: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<f64>,
}

/// One estimated parameter over trials, beside its truth and calibration-rs's predicted σ.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ParamStats {
    /// `cam_left fx`, `hand-eye tx`, …
    pub label: String,
    /// `px`, `deg`, `mm`, or empty (distortion coefficients).
    pub unit: String,
    pub truth: f64,
    pub mean: f64,
    /// Sample standard deviation over the trials.
    pub std: f64,
    /// `mean − truth`.
    pub bias: f64,
    /// Root mean square of `value − truth`.
    pub rmse: f64,
    /// calibration-rs's σ for it, the mean over the trials that have one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predicted_sigma: Option<f64>,
    /// Trials it was estimated in.
    pub trials: usize,
}

/// A parameter's value in one trial: label, unit, truth, value, predicted σ.
type Sample = (String, &'static str, f64, f64, Option<f64>);

/// The rotation vector (radians) of `r`.
fn rotation_vector(r: &nalgebra::UnitQuaternion<f64>) -> Vector3<f64> {
    r.scaled_axis()
}

/// The parameters of one report, beside the truth.
fn samples(truth: &Truth, r: &Report) -> Vec<Sample> {
    let sigma = |labels: &[String]| -> Option<f64> {
        let u = r.uncertainty.as_ref()?;
        labels
            .iter()
            .find_map(|l| u.labels.iter().position(|x| x == l))
            .map(|i| u.sigma[i])
    };
    let mut out = Vec::new();
    for (j, e) in r.estimate.cameras.iter().enumerate() {
        let Some(t) = truth.gt.cameras.iter().find(|c| c.id == e.id) else {
            continue;
        };
        let k = &t.params.intrinsics;
        let names = [
            ("fx", k.fx, e.intrinsics[0]),
            ("fy", k.fy, e.intrinsics[1]),
            ("cx", k.cx, e.intrinsics[2]),
            ("cy", k.cy, e.intrinsics[3]),
        ];
        for (n, tv, ev) in names {
            let s = sigma(&[format!("cam{j}.{n}"), n.to_owned()]);
            out.push((format!("{} {n}", e.id), "px", tv, ev, s));
        }
        if let vision_calibration_core::DistortionParams::BrownConrady5 { params: d } =
            &t.params.distortion
        {
            for (n, tv, ev) in [
                ("k1", d.k1, e.distortion[0]),
                ("k2", d.k2, e.distortion[1]),
                ("k3", d.k3, e.distortion[2]),
                ("p1", d.p1, e.distortion[3]),
                ("p2", d.p2, e.distortion[4]),
            ] {
                let s = sigma(&[format!("cam{j}.{n}"), n.to_owned()]);
                out.push((format!("{} {n}", e.id), "", tv, ev, s));
            }
        }
        if let SensorParams::Scheimpflug { params } = t.params.sensor {
            for (n, tv, ev) in [
                ("tilt_x", params.tilt_x, e.tilt[0]),
                ("tilt_y", params.tilt_y, e.tilt[1]),
            ] {
                let s = sigma(&[format!("cam{j}.{n}"), n.to_owned()]).map(f64::to_degrees);
                out.push((
                    format!("{} {n}", e.id),
                    "deg",
                    tv.to_degrees(),
                    ev.to_degrees(),
                    s,
                ));
            }
        }
    }
    if let (Some(he), Some(first)) = (r.estimate.handeye, r.cameras.first())
        && let Some(i) = truth.gt.cameras.iter().position(|c| &c.id == first)
        && truth.gt.handeye.is_some()
    {
        let t = truth.handeye(&truth.cam_se3_rig[i]);
        // calibration-rs's label of the hand-eye: the one transform that is not the target's.
        let prefix = r.uncertainty.as_ref().and_then(|u| {
            u.labels
                .iter()
                .filter_map(|l| l.strip_suffix(".tx"))
                .find(|p| {
                    (p.starts_with("gripper_se3_") || p.ends_with("_se3_base"))
                        && !p.ends_with("target")
                })
                .map(str::to_owned)
        });
        let s = |c: &str| prefix.as_ref().and_then(|p| sigma(&[format!("{p}.{c}")]));
        // The error in calibration-rs's tangent: `estimate = truth · Exp(δ)`, the rotation part
        // `log(R_trueᵀ R)` and the translation part `R_trueᵀ (t − t_true)`, both in the
        // transform's own frame (checked against its covariance: docs/measurements/g17_sim_lab.md).
        let dr = rotation_vector(&(t.rotation.inverse() * he.rotation));
        let dt = t.rotation.inverse() * (he.translation.vector - t.translation.vector);
        for (c, v) in ["rx", "ry", "rz"].iter().zip(dr.iter()) {
            out.push((
                format!("hand-eye {c}"),
                "deg",
                0.0,
                v.to_degrees(),
                s(c).map(f64::to_degrees),
            ));
        }
        for (c, v) in ["tx", "ty", "tz"].iter().zip(dt.iter()) {
            out.push((
                format!("hand-eye {c}"),
                "mm",
                0.0,
                v * 1e3,
                s(c).map(|v| v * 1e3),
            ));
        }
    }
    out
}

/// One parameter's samples over trials: unit, truth, values, predicted σ.
type Gathered = (&'static str, f64, Vec<f64>, Vec<f64>);

/// Each estimated parameter over `reports` (trials of one dataset): its spread against its truth
/// and against calibration-rs's predicted σ.
#[must_use]
pub fn param_stats(gt: &GroundTruth, reports: &[Report]) -> Vec<ParamStats> {
    let truth = Truth::new(gt);
    let mut order: Vec<String> = Vec::new();
    let mut by: BTreeMap<String, Gathered> = BTreeMap::new();
    for r in reports {
        for (label, unit, t, v, s) in samples(&truth, r) {
            let entry = by.entry(label.clone()).or_insert_with(|| {
                order.push(label);
                (unit, t, Vec::new(), Vec::new())
            });
            entry.2.push(v);
            if let Some(s) = s {
                entry.3.push(s);
            }
        }
    }
    order
        .into_iter()
        .map(|label| {
            let (unit, truth, values, sigmas) = &by[&label];
            let n = values.len() as f64;
            let mean = values.iter().sum::<f64>() / n;
            let var = if values.len() > 1 {
                values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0)
            } else {
                0.0
            };
            let mse = values.iter().map(|v| (v - truth).powi(2)).sum::<f64>() / n;
            ParamStats {
                unit: (*unit).to_owned(),
                truth: *truth,
                mean,
                std: var.sqrt(),
                bias: mean - truth,
                rmse: mse.sqrt(),
                predicted_sigma: (!sigmas.is_empty())
                    .then(|| sigmas.iter().sum::<f64>() / sigmas.len() as f64),
                trials: values.len(),
                label,
            }
        })
        .collect()
}

/// Per row, the mean, standard deviation and maximum over `reports` (rows matched by label).
#[must_use]
pub fn row_stats(reports: &[Report]) -> Vec<RowStats> {
    let mut by: BTreeMap<String, (Vec<f64>, Option<f64>, usize)> = BTreeMap::new();
    for (order, r) in reports.iter().flat_map(|r| r.rows.iter()).enumerate() {
        let e = by
            .entry(r.label.clone())
            .or_insert((Vec::new(), r.limit, order));
        e.0.push(r.value);
    }
    let mut out: Vec<(usize, RowStats)> = by
        .into_iter()
        .map(|(label, (v, limit, order))| {
            let n = v.len() as f64;
            let mean = v.iter().sum::<f64>() / n;
            let std = (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n).sqrt();
            let max = v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            (
                order,
                RowStats {
                    label,
                    mean,
                    std,
                    max,
                    limit,
                },
            )
        })
        .collect();
    out.sort_by_key(|(o, _)| *o);
    out.into_iter().map(|(_, s)| s).collect()
}
