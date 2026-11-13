//! The report of a calibration: rows, residuals, the estimate and calibration-rs's uncertainty.

use etendue_synth::observe::ObservationNoise;
use nalgebra::Isometry3;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use vision_calibration_pipeline::analysis::QualityReport;

/// One compared quantity: its error, and the gate's limit if it has one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Row {
    pub label: String,
    pub value: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<f64>,
}

impl Row {
    pub(super) fn new(label: impl Into<String>, value: f64, limit: Option<f64>) -> Self {
        Self {
            label: label.into(),
            value,
            limit,
        }
    }

    /// Whether the value is within its limit (always, without one).
    #[must_use]
    pub fn within(&self) -> bool {
        self.limit.is_none_or(|l| self.value <= l)
    }
}

/// The calibration-rs workflow a dataset was solved with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Workflow {
    SingleCamHandeye,
    RigHandeye,
    RigHandeyeLaserline,
    LaserlineDevice,
    PlanarIntrinsics,
    RigExtrinsics,
}

impl std::fmt::Display for Workflow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::SingleCamHandeye => "single-camera hand-eye",
            Self::RigHandeye => "rig hand-eye",
            Self::RigHandeyeLaserline => "rig hand-eye laserline",
            Self::LaserlineDevice => "laserline device",
            Self::PlanarIntrinsics => "planar intrinsics",
            Self::RigExtrinsics => "rig extrinsics",
        })
    }
}

/// One target point's reprojection residual.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FeatureResidual {
    /// The capture, by id (a real dataset's: calibration-rs's view index).
    pub capture: String,
    /// The camera, by id.
    pub camera: String,
    /// The target point, by its index in the ground truth's target (a real dataset's: the
    /// feature's index in its view).
    pub point: usize,
    /// The observed pixel.
    pub observed_px: [f64; 2],
    /// The pixel the estimate projects it to.
    pub projected_px: [f64; 2],
    /// calibration-rs's view, camera and feature indices, until the report names them.
    #[serde(skip)]
    pub(crate) raw: [usize; 3],
}

/// calibration-rs's predicted uncertainty of the estimate (its covariance's diagonal).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Predicted {
    /// calibration-rs's parameter labels (`cam0.fx`, `gripper_se3_rig.tx`, …), `camN` being the
    /// report's `cameras[N]`.
    pub labels: Vec<String>,
    /// The σ of each, in calibration-rs's units (px, radians, metres).
    pub sigma: Vec<f64>,
    /// The parameters held fixed (the σ are conditional on them).
    pub fixed: Vec<String>,
    /// The reprojection noise σ the residuals imply (px).
    pub noise_px: Option<f64>,
}

/// Reprojection residuals of the target points.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Residuals {
    /// Points with a residual.
    pub count: usize,
    /// calibration-rs's mean Euclidean error (px).
    pub mean_px: f64,
    /// Root mean square per image axis (px): `√(Σ(du² + dv²) / 2n)`.
    pub rms_axis_px: f64,
    /// Largest Euclidean error (px).
    pub max_px: f64,
    /// Mean Euclidean error per camera (px), in the ground truth's camera order.
    pub per_camera_mean_px: Vec<f64>,
    /// Every point's residual with where it is (the first trial's only, in a run of several).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<FeatureResidual>,
    /// Every point's Euclidean error (px), view-major.
    pub errors_px: Vec<f64>,
}

/// What the calibration found, for drawing beside the truth.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Estimate {
    /// Per calibrated camera: `[fx, fy, cx, cy]`, `[k1, k2, k3, p1, p2]`, `[tilt_x, tilt_y]`.
    pub cameras: Vec<EstimatedCamera>,
    /// The hand-eye transform the workflow solves (`gripper_se3_camera`/`_rig` eye-in-hand,
    /// `camera_se3_base`/`rig_se3_base` eye-to-hand), its rig frame the reference camera's.
    #[schemars(with = "Option<vision_calibration_core::Iso3Schema>")]
    pub handeye: Option<Isometry3<f64>>,
    /// Laser planes in each camera's frame, `n·p + d = 0`.
    pub laser_planes: Vec<Option<Plane>>,
    /// The estimate in the truth's frames, to draw beside it; `None` without a hand-eye or a
    /// target pose.
    pub offsets: Option<Offsets>,
}

/// Where the estimate is relative to the truth. A drawing places each true frame where the
/// scene has it and the estimate at that frame times its offset, at any robot pose.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Offsets {
    /// Per calibrated camera: the estimated camera in the true camera's frame
    /// (`true_se3_estimate`, projection frames), through the hand-eye and the rig.
    #[schemars(with = "Vec<vision_calibration_core::Iso3Schema>")]
    pub cameras: Vec<Isometry3<f64>>,
    /// The estimated target in the true target's frame.
    #[schemars(with = "vision_calibration_core::Iso3Schema")]
    pub target: Isometry3<f64>,
    /// Per calibrated camera, its laser plane: true, then estimated, both in the true camera's
    /// frame with unit normals pointing the same way.
    pub planes: Vec<Option<[Plane; 2]>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EstimatedCamera {
    pub id: String,
    pub intrinsics: [f64; 4],
    pub distortion: [f64; 5],
    pub tilt: [f64; 2],
    /// `cam_se3_ref`: the camera from the reference camera (identity for it).
    #[schemars(with = "vision_calibration_core::Iso3Schema")]
    pub cam_se3_ref: Isometry3<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Plane {
    pub normal: [f64; 3],
    pub distance: f64,
}

/// A calibration and its comparison with the truth.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Report {
    pub version: u32,
    pub workflow: Workflow,
    /// `"G5.1"`, or `"G5.3"` with lasers; empty without a truth (a real dataset).
    pub gate: String,
    /// Whether the gate applies (exact analytic observations).
    pub gated: bool,
    /// Whether every gated row is within its limit.
    pub pass: bool,
    /// The calibrated cameras.
    pub cameras: Vec<String>,
    /// Views given to the solver, target points, laser pixels.
    pub views: usize,
    pub points: usize,
    pub laser_pixels: usize,
    pub noise: Option<ObservationNoise>,
    pub rows: Vec<Row>,
    pub residuals: Residuals,
    pub estimate: Estimate,
    /// calibration-rs's quality verdict on the estimate (`analysis::quality`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<QualityReport>,
    /// calibration-rs's predicted uncertainty of the estimate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uncertainty: Option<Predicted>,
    /// The procedure that ran (ADR 0011): a built-in's id (`auto_rig`, …) or a library item's.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub procedure: String,
    /// What each stage gave, in order (the last is the report's own).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stages: Vec<StageSummary>,
    /// calibration-rs's own export of the workflow (both workflows' for a single head, by
    /// name): what a run keeps as `result.json`, not part of the report.
    #[serde(skip)]
    pub export: Value,
    /// A rig workflow's per-view target poses in the rig frame (what seeds a later rig stage);
    /// not part of the report.
    #[serde(skip)]
    pub rig_se3_target: Vec<Isometry3<f64>>,
    /// A real dataset's calibration (P20a), which has no truth: then there are no rows and no
    /// offsets, `gate` is empty and nothing is gated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub real: Option<crate::real::RealSummary>,
}

/// One stage of a procedure's run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct StageSummary {
    pub id: String,
    pub workflow: Workflow,
    pub cameras: Vec<String>,
    pub views: usize,
    pub points: usize,
    pub mean_reproj_px: f64,
}

impl Report {
    /// The rows as the script prints them.
    #[must_use]
    pub fn table(&self) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();
        for r in &self.rows {
            let limit = r.limit.map_or(String::new(), |l| format!("≤ {l}"));
            let verdict = match (r.limit, self.gated, r.within()) {
                (None, _, _) => "",
                (Some(_), true, true) => "PASS",
                (Some(_), true, false) => "FAIL",
                (Some(_), false, true) => "",
                (Some(_), false, false) => "outside",
            };
            let _ = writeln!(
                out,
                "  {:<28} {:12.3e}  {limit:<10} {verdict}",
                r.label, r.value
            );
        }
        if self.gated {
            let _ = writeln!(
                out,
                "  {}: {}",
                self.check_name(),
                if self.pass { "PASS" } else { "FAIL" }
            );
        } else {
            let all = self.rows.iter().all(Row::within);
            let _ = writeln!(
                out,
                "  {} not applied (noisy or detected observations); limits {}",
                self.check_name(),
                if all { "all within" } else { "not all within" }
            );
        }
        out
    }

    /// The gate as users read it: [`Report::gate`] is the id the measurements use.
    #[must_use]
    pub fn check_name(&self) -> String {
        if self.gate == "G5.3" {
            "closed-loop check (target and laser plane)".into()
        } else {
            "closed-loop check (target)".into()
        }
    }
}
