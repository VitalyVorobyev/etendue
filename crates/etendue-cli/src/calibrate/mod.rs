//! Native calibration of a dataset's observations, compared with its ground truth (P13).
//!
//! This is `tools/closed-loop/closed_loop.py` in Rust, on the same calibration-rs workflows
//! (`vision-calibration-pipeline`) with the same configuration and the same comparison rows:
//! - **single-camera hand-eye**: one camera on the robot (or `camera` picks one of a rig);
//! - **rig hand-eye**: several cameras on one rig;
//! - **rig hand-eye laserline**: a rig with lasers (seeded from nominal values when a camera is
//!   Scheimpflug, as the script does);
//! - **laserline device**: one camera carrying its laser (a triangulation head), with the
//!   hand-eye solved on the same views by the single-camera workflow.
//!
//! Eye-to-hand datasets are calibrated too (the script handles eye-in-hand only).
//!
//! The observations are [`Observations`]: the ground truth's own with controlled noise
//! (`etendue_synth::observe`), or a detector's ([`from_features`]). The rows are gated by G5.1
//! (and G5.3 with lasers) only on exact analytic observations, as the script gates them.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail, ensure};
use etendue_synth::dataset::{GroundTruth, GtCamera, GtHandeye, GtLaser};
use etendue_synth::laser::GtPlane;
use etendue_synth::observe::{
    ObservationNoise, Observations, ObservedCapture, ObservedPoint, ObservedView,
};
use nalgebra::{Isometry3, Point2, Point3, Vector3};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use vision_calibration_core::{
    BrownConrady5, CameraParams, CorrespondenceView, DistortionParams, FxFyCxCy, LaserPlane,
    NoMeta, NoiseLevel, PlanarDataset, Pt2, Pt3, ResidualKind, RigDataset, RigView, RigViewObs,
    ScheimpflugParams, SensorParams, Uncertainty, View,
};
use vision_calibration_pipeline::common::{ExportDiagnostics, HandeyeSolution, RobotPoseMeta};
use vision_calibration_pipeline::laserline_device::DeviceManualInit;
use vision_calibration_pipeline::session::CalibrationSession;
use vision_calibration_pipeline::single_cam_handeye::SingleCamHandeyeSolution;
use vision_calibration_pipeline::{
    laserline_device, planar_intrinsics, rig_extrinsics, rig_handeye, rig_handeye_laserline,
    single_cam_handeye,
};

/// Report version.
pub const REPORT_VERSION: u32 = 3;

/// G5.1's limits (noise-free analytic input), as `closed_loop.py` has them.
pub mod gate {
    pub const MEAN_REPROJ_PX: f64 = 0.05;
    pub const FOCAL_REL: f64 = 1e-4;
    pub const PRINCIPAL_PX: f64 = 0.05;
    pub const HANDEYE_ROT_DEG: f64 = 0.01;
    pub const HANDEYE_TRANS_MM: f64 = 0.05;
    /// G5.3.
    pub const PLANE_NORMAL_DEG: f64 = 0.01;
    pub const PLANE_DISTANCE_MM: f64 = 0.05;
    /// G5.3c: the largest error of a stripe point triangulated in the robot's frame.
    pub const TRIANGULATION_MM: f64 = 0.05;
}

/// One compared quantity: its error, and the gate's limit if it has one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub label: String,
    pub value: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<f64>,
}

impl Row {
    fn new(label: impl Into<String>, value: f64, limit: Option<f64>) -> Self {
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FeatureResidual {
    /// The capture, by id.
    pub capture: String,
    /// The camera, by id.
    pub camera: String,
    /// The target point, by its index in the ground truth's target.
    pub point: usize,
    /// The observed pixel.
    pub observed_px: [f64; 2],
    /// The pixel the estimate projects it to.
    pub projected_px: [f64; 2],
    /// calibration-rs's view, camera and feature indices, until [`finish`] names them.
    #[serde(skip)]
    raw: [usize; 3],
}

/// calibration-rs's predicted uncertainty of the estimate (its covariance's diagonal).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Estimate {
    /// Per calibrated camera: `[fx, fy, cx, cy]`, `[k1, k2, k3, p1, p2]`, `[tilt_x, tilt_y]`.
    pub cameras: Vec<EstimatedCamera>,
    /// The hand-eye transform the workflow solves (`gripper_se3_camera`/`_rig` eye-in-hand,
    /// `camera_se3_base`/`rig_se3_base` eye-to-hand), its rig frame the reference camera's.
    pub handeye: Option<Isometry3<f64>>,
    /// Laser planes in each camera's frame, `n·p + d = 0`.
    pub laser_planes: Vec<Option<Plane>>,
    /// The estimate in the truth's frames, to draw beside it; `None` without a hand-eye or a
    /// target pose.
    pub offsets: Option<Offsets>,
}

/// Where the estimate is relative to the truth. A drawing places each true frame where the
/// scene has it and the estimate at that frame times its offset, at any robot pose.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Offsets {
    /// Per calibrated camera: the estimated camera in the true camera's frame
    /// (`true_se3_estimate`, projection frames), through the hand-eye and the rig.
    pub cameras: Vec<Isometry3<f64>>,
    /// The estimated target in the true target's frame.
    pub target: Isometry3<f64>,
    /// Per calibrated camera, its laser plane: true, then estimated, both in the true camera's
    /// frame with unit normals pointing the same way.
    pub planes: Vec<Option<[Plane; 2]>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EstimatedCamera {
    pub id: String,
    pub intrinsics: [f64; 4],
    pub distortion: [f64; 5],
    pub tilt: [f64; 2],
    /// `cam_se3_ref`: the camera from the reference camera (identity for it).
    pub cam_se3_ref: Isometry3<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Plane {
    pub normal: [f64; 3],
    pub distance: f64,
}

/// A calibration and its comparison with the truth.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub version: u32,
    pub workflow: Workflow,
    /// `"G5.1"`, or `"G5.3"` with lasers.
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
    pub quality: Option<Value>,
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
}

/// One stage of a procedure's run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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

/// What to calibrate.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Calibrate this camera of a rig alone (single-camera hand-eye).
    pub camera: Option<String>,
    /// The observations are a detector's ([`from_features`]), so no gate applies.
    pub detected: bool,
    /// The procedure to run (its id and stages); `None` for `auto`'s choice by the dataset's shape.
    pub procedure: Option<(String, etendue_scene::ProcedurePart)>,
}

/// Observations from `etendue detect`: `features.json`, and `laser_features.json` for a laser
/// dataset. The robot poses are the ground truth's (exact, as the script reads them).
///
/// # Errors
///
/// When the files do not match the ground truth's captures and cameras.
pub fn from_features(
    gt: &GroundTruth,
    features: &crate::detect::Features,
    laser: Option<&crate::detect::laser::LaserFeatures>,
) -> Result<Observations> {
    ensure!(
        features.version == 1,
        "unsupported features.json version {}",
        features.version
    );
    ensure!(
        features.captures.len() == gt.captures.len()
            && features.captures.iter().zip(&gt.captures).all(|(f, g)| {
                f.id == g.id
                    && f.views
                        .iter()
                        .map(|v| &v.camera)
                        .eq(g.views.iter().map(|v| &v.camera))
            }),
        "features.json does not match gt.json's captures and cameras"
    );
    if let Some(l) = laser {
        ensure!(
            l.version == 1,
            "unsupported laser_features.json version {}",
            l.version
        );
        ensure!(
            l.captures
                .iter()
                .map(|c| &c.id)
                .eq(gt.captures.iter().map(|c| &c.id)),
            "laser_features.json does not match gt.json's captures"
        );
    }
    let captures = features
        .captures
        .iter()
        .zip(&gt.captures)
        .enumerate()
        .map(|(k, (f, g))| ObservedCapture {
            id: f.id.clone(),
            base_se3_tcp: g.base_se3_tcp,
            views: f
                .views
                .iter()
                .map(|v| {
                    let ok = v.status == crate::detect::ViewStatus::Ok;
                    let points = if ok {
                        v.points
                            .iter()
                            .map(|p| {
                                let b = &gt.target.points[p.point];
                                ObservedPoint {
                                    point: p.point,
                                    target_xyz: b.xyz_m.unwrap_or([
                                        b.position_m[0],
                                        b.position_m[1],
                                        0.0,
                                    ]),
                                    pixel: p.pixel,
                                    outlier: false,
                                }
                            })
                            .collect()
                    } else {
                        Vec::new()
                    };
                    let stripe = laser
                        .and_then(|l| l.captures[k].views.iter().find(|s| s.camera == v.camera))
                        .map(|s| s.pixels.clone())
                        .unwrap_or_default();
                    ObservedView {
                        camera: v.camera.clone(),
                        points: if points.len() >= 4 {
                            points
                        } else {
                            Vec::new()
                        },
                        laser: if stripe.len() >= 2 {
                            stripe
                        } else {
                            Vec::new()
                        },
                    }
                })
                .collect(),
        })
        .collect();
    Ok(Observations {
        noise: ObservationNoise::default(),
        captures,
    })
}

// ── Pose differences ──────────────────────────────────────────────────────────

/// Rotation angle (deg) and translation distance (mm) between two poses, as the script measures
/// them: the angle of `truth⁻¹·estimate` from its matrix (atan2, precise near zero), and the
/// distance between the translations.
fn difference(estimate: &Isometry3<f64>, truth: &Isometry3<f64>) -> (f64, f64) {
    let d = (truth.inverse() * estimate).rotation.to_rotation_matrix();
    let r = d.matrix();
    let sin = 0.5
        * Vector3::new(
            r[(2, 1)] - r[(1, 2)],
            r[(0, 2)] - r[(2, 0)],
            r[(1, 0)] - r[(0, 1)],
        )
        .norm();
    let angle = sin.atan2((r.trace() - 1.0) / 2.0).to_degrees();
    (
        angle,
        (estimate.translation.vector - truth.translation.vector).norm() * 1e3,
    )
}

fn intrinsics_rows(e: &EstimatedCamera, truth: &GtCamera) -> Vec<Row> {
    let k = &truth.params.intrinsics;
    let name = &truth.id;
    let [fx, fy, cx, cy] = e.intrinsics;
    let mut rows = vec![
        Row::new(
            format!("{name} fx rel"),
            (fx - k.fx).abs() / k.fx,
            Some(gate::FOCAL_REL),
        ),
        Row::new(
            format!("{name} fy rel"),
            (fy - k.fy).abs() / k.fy,
            Some(gate::FOCAL_REL),
        ),
        Row::new(
            format!("{name} cx px"),
            (cx - k.cx).abs(),
            Some(gate::PRINCIPAL_PX),
        ),
        Row::new(
            format!("{name} cy px"),
            (cy - k.cy).abs(),
            Some(gate::PRINCIPAL_PX),
        ),
    ];
    if let Some(d) = distortion_of(&truth.params) {
        for (i, c) in ["k1", "k2", "k3", "p1", "p2"].iter().enumerate() {
            rows.push(Row::new(
                format!("{name} {c} abs"),
                (e.distortion[i] - d[i]).abs(),
                None,
            ));
        }
    }
    rows
}

fn tilt_rows(e: &EstimatedCamera, truth: &GtCamera) -> Vec<Row> {
    let [tx, ty] = tilt_of(&truth.params);
    vec![
        Row::new(
            format!("{} tilt_x deg", truth.id),
            (e.tilt[0] - tx).to_degrees().abs(),
            None,
        ),
        Row::new(
            format!("{} tilt_y deg", truth.id),
            (e.tilt[1] - ty).to_degrees().abs(),
            None,
        ),
    ]
}

/// Normal angle and offset difference of two planes `n·p + d = 0`, sign-free.
fn plane_rows(e: &Plane, truth: &GtPlane, name: &str) -> Vec<Row> {
    let (mut ne, mut de) = (Vector3::from(e.normal), e.distance);
    let (mut nt, mut dt) = (Vector3::from(truth.normal), truth.distance);
    let (se, st) = (ne.norm(), nt.norm());
    ne /= se;
    de /= se;
    nt /= st;
    dt /= st;
    if ne.dot(&nt) < 0.0 {
        ne = -ne;
        de = -de;
    }
    let angle = ne.cross(&nt).norm().atan2(ne.dot(&nt)).to_degrees();
    vec![
        Row::new(
            format!("{name} laser normal deg"),
            angle,
            Some(gate::PLANE_NORMAL_DEG),
        ),
        Row::new(
            format!("{name} laser distance mm"),
            (de - dt).abs() * 1e3,
            Some(gate::PLANE_DISTANCE_MM),
        ),
    ]
}

// ── Inputs ────────────────────────────────────────────────────────────────────

fn correspondences(view: &ObservedView) -> Option<CorrespondenceView> {
    if view.points.len() < 4 {
        return None;
    }
    let p3 = view
        .points
        .iter()
        .map(|p| Pt3::new(p.target_xyz[0], p.target_xyz[1], p.target_xyz[2]));
    let p2 = view.points.iter().map(|p| Pt2::new(p.pixel[0], p.pixel[1]));
    CorrespondenceView::new(p3.collect(), p2.collect()).ok()
}

fn laser_pixels(view: &ObservedView) -> Option<Vec<[f64; 2]>> {
    (view.laser.len() >= 2).then(|| view.laser.clone())
}

/// The truth's frames, mapped onto calibration-rs's: its rig frame is the reference camera's.
pub(crate) struct Truth<'a> {
    pub(crate) gt: &'a GroundTruth,
    /// `cam_se3_rig` per camera (identity without a rig).
    pub(crate) cam_se3_rig: Vec<Isometry3<f64>>,
}

impl<'a> Truth<'a> {
    pub(crate) fn new(gt: &'a GroundTruth) -> Self {
        Self {
            gt,
            cam_se3_rig: gt
                .cameras
                .iter()
                .map(|c| c.cam_se3_rig.unwrap_or_else(Isometry3::identity))
                .collect(),
        }
    }

    pub(crate) fn eye_in_hand(&self) -> Result<bool> {
        match &self.gt.handeye {
            Some(GtHandeye::EyeInHand { .. }) => Ok(true),
            Some(GtHandeye::EyeToHand { .. }) => Ok(false),
            None => bail!("the dataset has no robot: there is no hand-eye to calibrate"),
        }
    }

    /// The hand-eye transform of the frame `ref_se3_rig` maps into (the rig, a camera of it).
    pub(crate) fn handeye(&self, ref_se3_rig: &Isometry3<f64>) -> Isometry3<f64> {
        match self.gt.handeye.as_ref().expect("checked") {
            GtHandeye::EyeInHand {
                gripper_se3_rig, ..
            } => gripper_se3_rig * ref_se3_rig.inverse(),
            GtHandeye::EyeToHand { rig_se3_base, .. } => ref_se3_rig * rig_se3_base,
        }
    }

    /// The target in the frame the workflow reports it in: the base eye-in-hand, the gripper
    /// eye-to-hand.
    pub(crate) fn target(&self) -> (&'static str, Isometry3<f64>) {
        match self.gt.handeye.as_ref().expect("checked") {
            GtHandeye::EyeInHand {
                base_se3_target, ..
            } => ("base_se3_target", *base_se3_target),
            GtHandeye::EyeToHand {
                gripper_se3_target, ..
            } => ("gripper_se3_target", *gripper_se3_target),
        }
    }

    pub(crate) fn scheimpflug(&self, cameras: &[usize]) -> bool {
        cameras.iter().any(|&i| {
            matches!(
                self.gt.cameras[i].params.sensor,
                SensorParams::Scheimpflug { .. }
            )
        })
    }

    /// Camera `i`'s own laser (the only laser, if none names it).
    pub(crate) fn laser(&self, i: usize) -> Option<&GtLaser> {
        let id = &self.gt.cameras[i].id;
        self.gt
            .lasers
            .iter()
            .find(|l| l.camera.as_deref() == Some(id.as_str()))
            .or_else(|| (self.gt.lasers.len() == 1).then(|| &self.gt.lasers[0]))
    }

    /// The truth's plane of camera `i`'s own laser.
    pub(crate) fn plane(&self, i: usize) -> Option<&GtPlane> {
        let id = &self.gt.cameras[i].id;
        self.laser(i)?.camera_planes.iter().find(|p| &p.frame == id)
    }
}

pub(crate) fn handeye_mode(eye_in_hand: bool) -> Value {
    json!(if eye_in_hand {
        "EyeInHand"
    } else {
        "EyeToHand"
    })
}

/// A config: `T::default()` with JSON `patch` merged in (the configs are `non_exhaustive`). A
/// key of the patch that the config does not have is an error, not ignored.
fn config<T: Serialize + DeserializeOwned + Default>(patch: &Value) -> Result<T> {
    let mut v = serde_json::to_value(T::default())?;
    merge(&mut v, patch);
    strict(v, "calibration config")
}

/// `v` as calibration-rs's `T`, refusing any key `T` does not read. calibration-rs's types
/// ignore unknown fields, so a key renamed upstream (or misspelt here) would otherwise be dropped
/// silently and the workflow would run on its default. The check: every key of `v` with a
/// non-empty value survives a round trip through `T`.
pub(crate) fn strict<T: Serialize + DeserializeOwned>(v: Value, what: &str) -> Result<T> {
    let typed: T = serde_json::from_value(v.clone()).with_context(|| what.to_owned())?;
    let back = serde_json::to_value(&typed)?;
    let mut lost = Vec::new();
    dropped(&v, &back, "", &mut lost);
    ensure!(
        lost.is_empty(),
        "{what}: calibration-rs does not read {}",
        lost.join(", ")
    );
    Ok(typed)
}

/// The keys of `given` with a non-empty value that `back` (its round trip) lost, by path.
fn dropped(given: &Value, back: &Value, at: &str, lost: &mut Vec<String>) {
    let empty = |v: &Value| match v {
        Value::Null => true,
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        _ => false,
    };
    match (given, back) {
        (Value::Object(g), Value::Object(b)) => {
            for (k, v) in g {
                let path = if at.is_empty() {
                    k.clone()
                } else {
                    format!("{at}.{k}")
                };
                match b.get(k) {
                    Some(w) => dropped(v, w, &path, lost),
                    None if !empty(v) => lost.push(format!("`{path}`")),
                    None => {}
                }
            }
        }
        (Value::Array(g), Value::Array(b)) if g.len() == b.len() => {
            for (i, (v, w)) in g.iter().zip(b).enumerate() {
                dropped(v, w, &format!("{at}[{i}]"), lost);
            }
        }
        _ => {}
    }
}

pub(crate) fn merge(into: &mut Value, patch: &Value) {
    match (into, patch) {
        (Value::Object(a), Value::Object(b)) => {
            for (k, v) in b {
                merge(a.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (a, b) => *a = b.clone(),
    }
}

/// The values a user would enter for a Scheimpflug rig from datasheets, as the script makes them:
/// per camera, the focal length rounded to 100 px with the principal point at the image centre,
/// and the tilt about x rounded to a whole degree.
pub(crate) fn nominal_values(cameras: &[&GtCamera]) -> (Vec<[f64; 4]>, Vec<[f64; 2]>) {
    cameras
        .iter()
        .map(|c| {
            let k = &c.params.intrinsics;
            let f = ((k.fx + k.fy) / 2.0 / 100.0).round_ties_even() * 100.0;
            let [w, h] = c.resolution.map(f64::from);
            let tilt = tilt_of(&c.params)[0];
            (
                [f, f, w / 2.0, h / 2.0],
                [tilt.to_degrees().round_ties_even().to_radians(), 0.0],
            )
        })
        .unzip()
}

/// A camera's Brown-Conrady coefficients `[k1, k2, k3, p1, p2]` (zero without distortion);
/// `None` for another distortion model, which etendue does not calibrate.
pub(crate) fn distortion_of(p: &CameraParams) -> Option<[f64; 5]> {
    match &p.distortion {
        DistortionParams::BrownConrady5 { params: d } => Some([d.k1, d.k2, d.k3, d.p1, d.p2]),
        DistortionParams::None => Some([0.0; 5]),
        _ => None,
    }
}

/// A camera's Scheimpflug tilt `[tilt_x, tilt_y]`, zero for an untilted sensor.
pub(crate) fn tilt_of(p: &CameraParams) -> [f64; 2] {
    match p.sensor {
        SensorParams::Scheimpflug { params } => [params.tilt_x, params.tilt_y],
        _ => [0.0, 0.0],
    }
}

/// An estimated camera from an export's camera.
fn camera_of(p: &CameraParams, id: &str, cam_se3_ref: Isometry3<f64>) -> Result<EstimatedCamera> {
    let k = &p.intrinsics;
    Ok(EstimatedCamera {
        id: id.to_owned(),
        intrinsics: [k.fx, k.fy, k.cx, k.cy],
        distortion: distortion_of(p).with_context(|| {
            format!("camera `{id}`: calibration-rs returned a distortion model that is not Brown-Conrady")
        })?,
        tilt: tilt_of(p),
        cam_se3_ref,
    })
}

/// The calibration-rs model of an estimated camera: `like`'s (the truth's) projection with the
/// estimate's values, Brown-Conrady distortion, and a Scheimpflug sensor when either is tilted.
fn params_of(cam: &EstimatedCamera, like: &CameraParams) -> CameraParams {
    let [fx, fy, cx, cy] = cam.intrinsics;
    let [k1, k2, k3, p1, p2] = cam.distortion;
    let iters = match &like.distortion {
        DistortionParams::BrownConrady5 { params } => params.iters,
        _ => 8,
    };
    let [tilt_x, tilt_y] = cam.tilt;
    let tilted = matches!(like.sensor, SensorParams::Scheimpflug { .. });
    CameraParams {
        projection: like.projection.clone(),
        distortion: DistortionParams::BrownConrady5 {
            params: BrownConrady5 {
                k1,
                k2,
                k3,
                p1,
                p2,
                iters,
            },
        },
        sensor: if tilted || tilt_x != 0.0 || tilt_y != 0.0 {
            SensorParams::Scheimpflug {
                params: ScheimpflugParams { tilt_x, tilt_y },
            }
        } else {
            like.sensor.clone()
        },
        intrinsics: FxFyCxCy { fx, fy, cx, cy },
    }
}

/// Each laser camera's stripe triangulated as the calibrated head measures it, against the truth
/// (G5.3c): every true stripe pixel back-projected with the estimated camera, met with the
/// estimated laser plane, and carried into the robot base through the estimated hand-eye and the
/// true robot pose. Unlike the camera-frame rows, this does not depend on how an estimate splits
/// the projection among sensor tilt, principal point and camera frame
/// (`docs/derivations/scheimpflug_observability.md` §5): it is what a measurement inherits. It
/// takes a report's estimate as a whole, so it also catches parts that come from different camera
/// solutions.
///
/// # Errors
///
/// When an estimated camera does not make a model.
pub(crate) fn triangulation_rows(truth: &Truth, estimate: &Estimate) -> Result<Vec<Row>> {
    let (Some(he), Some(handeye)) = (estimate.handeye, truth.gt.handeye.as_ref()) else {
        return Ok(vec![]);
    };
    let mut rows = Vec::new();
    for (cam, plane) in estimate.cameras.iter().zip(&estimate.laser_planes) {
        let Some(plane) = plane else { continue };
        let Some(i) = truth.gt.cameras.iter().position(|c| c.id == cam.id) else {
            continue;
        };
        let Some(laser) = truth.laser(i) else {
            continue;
        };
        let model = params_of(cam, &truth.gt.cameras[i].params).build()?;
        let (n, d) = (Vector3::from(plane.normal), plane.distance);
        let ref_se3_cam = cam.cam_se3_ref.inverse();
        let (mut sq, mut max, mut count) = (0.0, 0.0_f64, 0_usize);
        for c in &truth.gt.captures {
            let Some(base_se3_tcp) = c.base_se3_tcp else {
                continue;
            };
            let (base_se3_cam, base_se3_target) = match handeye {
                GtHandeye::EyeInHand {
                    base_se3_target, ..
                } => (base_se3_tcp * he * ref_se3_cam, *base_se3_target),
                GtHandeye::EyeToHand {
                    gripper_se3_target, ..
                } => (
                    he.inverse() * ref_se3_cam,
                    base_se3_tcp * gripper_se3_target,
                ),
            };
            let stripe = c.laser_stripes.iter().find(|s| s.laser == laser.id);
            let seen = c.views[i].laser_points.iter().find(|s| s.laser == laser.id);
            let (Some(stripe), Some(seen)) = (stripe, seen) else {
                continue;
            };
            for p in seen.points.iter().filter(|p| p.occluded.is_none()) {
                let (Some([u, v]), Some(b)) = (p.pixel, stripe.points.get(p.point)) else {
                    continue;
                };
                let ray = model.backproject_pixel(&Point2::new(u, v)).direction;
                let measured = base_se3_cam * Point3::from(ray * (-d / n.dot(&ray)));
                let [x, y, z] = b.xyz_m.unwrap_or([b.position_m[0], b.position_m[1], 0.0]);
                let e = (measured - base_se3_target * Point3::new(x, y, z)).norm();
                sq += e * e;
                max = max.max(e);
                count += 1;
            }
        }
        if count > 0 {
            rows.push(Row::new(
                format!("{} triangulated rms mm", cam.id),
                (sq / count as f64).sqrt() * 1e3,
                None,
            ));
            rows.push(Row::new(
                format!("{} triangulated max mm", cam.id),
                max * 1e3,
                Some(gate::TRIANGULATION_MM),
            ));
        }
    }
    Ok(rows)
}

/// `report` with its [`triangulation_rows`]: the result of a procedure, as a whole.
pub(crate) fn with_triangulation(truth: &Truth, mut report: Report) -> Result<Report> {
    let rows = triangulation_rows(truth, &report.estimate)?;
    report.rows.extend(rows);
    report.pass = !report.gated || report.rows.iter().all(Row::within);
    Ok(report)
}

fn plane_of(p: &LaserPlane) -> Plane {
    Plane {
        normal: p.normal().into_inner().into(),
        distance: p.distance(),
    }
}

/// The residual figures of an export's diagnostics, for `cameras` cameras; `mean_px` is the
/// export's own mean reprojection error.
fn residuals(diagnostics: &ExportDiagnostics, mean_px: f64, cameras: usize) -> Residuals {
    let mut errors = Vec::new();
    let mut sq = 0.0;
    let mut per_camera = vec![(0.0, 0usize); cameras];
    let mut features = Vec::new();
    for r in &diagnostics.per_feature_residuals.target {
        // A point whose projection failed has no residual.
        let (o, Some(p)) = (r.observed_px, r.projected_px) else {
            continue;
        };
        let (du, dv) = (p[0] - o[0], p[1] - o[1]);
        let e = du.hypot(dv);
        sq += du * du + dv * dv;
        errors.push(e);
        if let Some(slot) = per_camera.get_mut(r.camera) {
            slot.0 += e;
            slot.1 += 1;
        }
        features.push(FeatureResidual {
            capture: String::new(),
            camera: String::new(),
            point: 0,
            observed_px: o,
            projected_px: p,
            raw: [r.pose, r.camera, r.feature],
        });
    }
    let n = errors.len();
    Residuals {
        count: n,
        mean_px,
        rms_axis_px: if n == 0 {
            f64::NAN
        } else {
            (sq / (2 * n) as f64).sqrt()
        },
        max_px: errors.iter().copied().fold(0.0, f64::max),
        per_camera_mean_px: per_camera
            .iter()
            .map(|&(s, k)| if k == 0 { f64::NAN } else { s / k as f64 })
            .collect(),
        features,
        errors_px: errors,
    }
}

/// The captures a workflow's views come from, in order: those `used` admits.
fn used_views(obs: &Observations, used: impl Fn(&ObservedCapture) -> bool) -> Vec<usize> {
    (0..obs.captures.len())
        .filter(|&k| used(&obs.captures[k]))
        .collect()
}

/// An export as JSON (what a run keeps), with calibration-rs's quality verdict for cameras of
/// `sizes`.
fn exported<E>(export: &E, sizes: &[[u32; 2]]) -> Result<(Value, Option<Value>)>
where
    E: Serialize + vision_calibration_pipeline::common::CalibratedCameras,
{
    let quality = vision_calibration_pipeline::analysis::assess(
        export,
        sizes,
        &vision_calibration_pipeline::analysis::QualityThresholds::default(),
    )
    .ok()
    .and_then(|q| serde_json::to_value(q).ok());
    Ok((serde_json::to_value(export)?, quality))
}

/// The image sizes of dataset cameras `cams`.
fn sizes(truth: &Truth, cams: &[usize]) -> Vec<[u32; 2]> {
    cams.iter()
        .map(|&i| truth.gt.cameras[i].resolution)
        .collect()
}

/// calibration-rs's covariance, as σ; `None` when it has none.
fn predicted(uncertainty: &Uncertainty) -> Option<Predicted> {
    let u = uncertainty.covariance()?;
    let sigma = (0..u.labels.len())
        .map(|i| {
            u.covariance
                .get(i)
                .and_then(|r| r.get(i))
                .copied()
                .unwrap_or(f64::NAN)
                .max(0.0)
                .sqrt()
        })
        .collect();
    Some(Predicted {
        labels: u.labels.clone(),
        sigma,
        fixed: u.fixed.clone(),
        noise_px: u
            .noise
            .iter()
            .find(|n| n.kind == ResidualKind::Reprojection)
            .and_then(|n| match n.level {
                NoiseLevel::Sigma(s) => Some(s),
                _ => None,
            }),
    })
}

// ── Stages ────────────────────────────────────────────────────────────────────

/// A stage's starting values, per dataset camera (indexed as `gt.cameras`).
#[derive(Clone, Debug, Default)]
pub(crate) struct SeedValues {
    /// `[fx, fy, cx, cy]`.
    pub intrinsics: Option<Vec<[f64; 4]>>,
    /// `[k1, k2, k3, p1, p2]`, where known.
    pub distortion: Option<Vec<Option<[f64; 5]>>>,
    /// Scheimpflug `[tilt_x, tilt_y]` (zero for an untilted camera).
    pub sensor: Option<Vec<[f64; 2]>>,
    /// `cam_se3_rig`, the reference camera (index 0) at the identity.
    pub cam_se3_rig: Option<Vec<Isometry3<f64>>>,
    /// The target in the rig's frame per view, which calibration-rs takes with `cam_se3_rig`
    /// (the earlier stage's views: the same captures, in the same order).
    pub rig_se3_target: Option<Vec<Isometry3<f64>>>,
    /// The hand-eye in the stage's frame: `gripper_se3_<frame>` eye-in-hand,
    /// `<frame>_se3_base` eye-to-hand, where the frame is the rig's (camera 0's) for a rig stage
    /// and the camera's for a single-camera stage.
    pub handeye: Option<Isometry3<f64>>,
}

impl SeedValues {
    fn none(&self) -> bool {
        self.intrinsics.is_none()
            && self.sensor.is_none()
            && self.cam_se3_rig.is_none()
            && self.handeye.is_none()
    }

    /// calibration-rs's per-camera intrinsics seed for `cameras` (`IntrinsicsManualInit` of the
    /// rig workflows).
    pub(crate) fn rig_manual_init(&self, cameras: &[usize]) -> Option<Value> {
        if self.intrinsics.is_none() && self.sensor.is_none() {
            return None;
        }
        let pick = |v: &Vec<[f64; 4]>| cameras.iter().map(|&i| k_json(&v[i])).collect::<Vec<_>>();
        let distortion = self.distortion.as_ref().and_then(|d| {
            cameras
                .iter()
                .map(|&i| d[i].map(|d| d_json(&d)))
                .collect::<Option<Vec<_>>>()
        });
        Some(json!({
            "per_cam_intrinsics": self.intrinsics.as_ref().map(pick),
            "per_cam_distortion": distortion,
            "per_cam_sensors": self.sensor.as_ref().map(|s| cameras.iter().map(|&i| tilt_json(s[i])).collect::<Vec<_>>()),
        }))
    }

    /// The single-camera intrinsics seed for camera `i` (`IntrinsicsManualInit` of
    /// `planar_intrinsics` and `single_cam_handeye`, and `DeviceManualInit`), read through
    /// [`strict`].
    fn camera_init(&self, i: usize) -> Value {
        let mut seed = json!({});
        if let Some(k) = &self.intrinsics {
            seed["intrinsics"] = k_json(&k[i]);
        }
        if let Some(d) = self.distortion.as_ref().and_then(|d| d[i]) {
            seed["distortion"] = d_json(&d);
        }
        if let Some(t) = &self.sensor {
            seed["sensor"] = tilt_json(t[i]);
        }
        seed
    }
}

fn k_json(k: &[f64; 4]) -> Value {
    json!({"fx": k[0], "fy": k[1], "cx": k[2], "cy": k[3]})
}

/// A Brown-Conrady seed as calibration-rs's `DistortionParams` (tagged, since 0.12; its kind must
/// be the camera's configured model, which is Brown-Conrady for every camera etendue calibrates).
fn d_json(d: &[f64; 5]) -> Value {
    json!({"type": "brown_conrady5", "k1": d[0], "k2": d[1], "k3": d[2], "p1": d[3], "p2": d[4], "iters": 8})
}

fn tilt_json(t: [f64; 2]) -> Value {
    json!({"tilt_x": t[0], "tilt_y": t[1]})
}

/// A stage resolved against a dataset (`crate::procedure`): calibration-rs's config patch, the
/// seeds, and whether the stage's camera model is Scheimpflug.
#[derive(Clone, Debug, Default)]
pub(crate) struct StageRun {
    pub patch: Value,
    pub seeds: SeedValues,
    pub scheimpflug: bool,
}

/// The views of camera `i`, as planar correspondences.
fn planar_views(obs: &Observations, i: usize) -> (Vec<CorrespondenceView>, usize) {
    let views: Vec<CorrespondenceView> = obs
        .captures
        .iter()
        .filter_map(|c| correspondences(&c.views[i]))
        .collect();
    let points = views.iter().map(CorrespondenceView::len).sum();
    (views, points)
}

/// Checks that hold whatever the procedure: the observations are the truth's.
pub(crate) fn check_observations(gt: &GroundTruth, obs: &Observations) -> Result<()> {
    ensure!(
        obs.captures.len() == gt.captures.len()
            && obs.captures.iter().zip(&gt.captures).all(|(o, g)| {
                o.id == g.id
                    && o.views
                        .iter()
                        .map(|v| &v.camera)
                        .eq(g.views.iter().map(|v| &v.camera))
            }),
        "the observations do not match the ground truth's captures and cameras"
    );
    Ok(())
}

/// The robot poses of the captures (a hand-eye stage needs them).
pub(crate) fn robot_poses(obs: &Observations) -> Result<Vec<Isometry3<f64>>> {
    obs.captures
        .iter()
        .map(|c| c.base_se3_tcp.context("a capture has no robot pose"))
        .collect()
}

// ── Workflows ─────────────────────────────────────────────────────────────────

/// Calibrate `obs` of `gt`'s dataset with the procedure the dataset's shape calls for (`auto`,
/// [`crate::procedure::auto`]) and compare with the truth. Rows are gated by G5.1 (G5.3) on the
/// exact analytic observations only.
///
/// # Errors
///
/// When the dataset has no robot, the observations do not match the truth's captures, or a
/// calibration-rs workflow fails.
pub fn calibrate(gt: &GroundTruth, obs: &Observations, options: &Options) -> Result<Report> {
    let procedure = match &options.procedure {
        Some(p) => p.clone(),
        None => crate::procedure::auto(gt, options)?,
    };
    crate::procedure::run(gt, obs, options, &procedure.0, &procedure.1)
}

/// What a single-camera hand-eye run gives: the export (typed and as JSON), its quality verdict,
/// views and points.
type SingleRun = (
    single_cam_handeye::SingleCamHandeyeExport,
    Value,
    Option<Value>,
    usize,
    usize,
);

fn run_single(
    obs: &Observations,
    poses: &[Isometry3<f64>],
    i: usize,
    eye_in_hand: bool,
    run: &StageRun,
    size: [u32; 2],
) -> Result<SingleRun> {
    let mut views = Vec::new();
    let mut points = 0;
    for (c, pose) in obs.captures.iter().zip(poses) {
        if let Some(v) = correspondences(&c.views[i]) {
            points += v.len();
            views.push(View::new(
                v,
                RobotPoseMeta {
                    base_se3_gripper: *pose,
                },
            ));
        }
    }
    let n = views.len();
    let mut session = CalibrationSession::<single_cam_handeye::SingleCamHandeyeProblem>::new();
    session.set_input(single_cam_handeye::SingleCamHandeyeInput::new(views)?)?;
    session.set_config(config(&run.patch)?)?;
    let s = &run.seeds;
    if s.none() {
        single_cam_handeye::run_calibration(&mut session)?;
    } else {
        if s.intrinsics.is_some() || s.sensor.is_some() {
            let manual = strict(s.camera_init(i), "intrinsics seed")?;
            single_cam_handeye::step_intrinsics_init_with_seed(&mut session, manual, None)?;
        } else {
            single_cam_handeye::step_intrinsics_init(&mut session, None)?;
        }
        single_cam_handeye::step_intrinsics_optimize(&mut session, None)?;
        if let Some(he) = &s.handeye {
            let key = if eye_in_hand {
                "gripper_se3_camera"
            } else {
                "camera_se3_base"
            };
            let manual = strict(json!({ key: he }), "hand-eye seed")?;
            single_cam_handeye::step_handeye_init_with_seed(&mut session, manual, None)?;
        } else {
            single_cam_handeye::step_handeye_init(&mut session, None)?;
        }
        single_cam_handeye::step_handeye_optimize(&mut session, None)?;
    }
    let export = session.export()?;
    let (json, quality) = exported(&export, &[size])?;
    Ok((export, json, quality, n, points))
}

/// A single camera's hand-eye and target pose, in the frames of `truth`'s mode.
fn single_handeye(
    truth: &Truth,
    solution: &SingleCamHandeyeSolution,
) -> Result<(Isometry3<f64>, Isometry3<f64>)> {
    let pair = match *solution {
        SingleCamHandeyeSolution::EyeInHand {
            gripper_se3_camera,
            base_se3_target,
        } => (true, gripper_se3_camera, base_se3_target),
        SingleCamHandeyeSolution::EyeToHand {
            camera_se3_base,
            gripper_se3_target,
        } => (false, camera_se3_base, gripper_se3_target),
    };
    ensure!(
        pair.0 == truth.eye_in_hand()?,
        "calibration-rs solved the hand-eye in the other mode"
    );
    Ok((pair.1, pair.2))
}

/// A rig's hand-eye and target pose, in the frames of `truth`'s mode.
fn rig_handeye(
    truth: &Truth,
    solution: &HandeyeSolution,
) -> Result<(Isometry3<f64>, Isometry3<f64>)> {
    let pair = match *solution {
        HandeyeSolution::EyeInHand {
            gripper_se3_rig,
            base_se3_target,
        } => (true, gripper_se3_rig, base_se3_target),
        HandeyeSolution::EyeToHand {
            rig_se3_base,
            gripper_se3_target,
        } => (false, rig_se3_base, gripper_se3_target),
    };
    ensure!(
        pair.0 == truth.eye_in_hand()?,
        "calibration-rs solved the hand-eye in the other mode"
    );
    Ok((pair.1, pair.2))
}

fn handeye_rows(
    truth: &Truth,
    (he, target): (Isometry3<f64>, Isometry3<f64>),
    ref_se3_rig: &Isometry3<f64>,
) -> Vec<Row> {
    let (r, t) = difference(&he, &truth.handeye(ref_se3_rig));
    let (name, target_truth) = truth.target();
    let (tr, tt) = difference(&target, &target_truth);
    vec![
        Row::new("hand-eye rotation deg", r, Some(gate::HANDEYE_ROT_DEG)),
        Row::new("hand-eye translation mm", t, Some(gate::HANDEYE_TRANS_MM)),
        Row::new(format!("{name} rot deg"), tr, None),
        Row::new(format!("{name} trans mm"), tt, None),
    ]
}

/// A workflow's result before it is reported.
struct Solved {
    workflow: Workflow,
    cameras: Vec<usize>,
    rows: Vec<Row>,
    /// Views, target points, laser pixels.
    counts: (usize, usize, usize),
    residuals: Residuals,
    estimate: Estimate,
    /// The target pose the workflow reports (base frame eye-in-hand, gripper eye-to-hand).
    target: Option<Isometry3<f64>>,
    export: Value,
    /// The capture of each of calibration-rs's views, in order.
    views: Vec<usize>,
    /// calibration-rs's quality verdict.
    quality: Option<Value>,
    /// calibration-rs's predicted uncertainty.
    uncertainty: Option<Predicted>,
    /// A rig workflow's per-view target poses in the rig frame.
    rig_se3_target: Vec<Isometry3<f64>>,
}

/// The estimated camera `j` (truth camera `i`) in the true camera's frame.
fn camera_offset(
    truth: &Truth,
    estimate: &Estimate,
    handeye: &Isometry3<f64>,
    i: usize,
    j: usize,
) -> Option<Isometry3<f64>> {
    let cam_se3_ref = estimate.cameras.get(j)?.cam_se3_ref;
    let true_x = truth.handeye(&truth.cam_se3_rig[i]);
    Some(match truth.gt.handeye.as_ref()? {
        // gripper_se3_camera
        GtHandeye::EyeInHand { .. } => true_x.inverse() * (handeye * cam_se3_ref.inverse()),
        // camera_se3_base
        GtHandeye::EyeToHand { .. } => true_x * (cam_se3_ref * handeye).inverse(),
    })
}

/// A plane with a unit normal, `n·p + d = 0`.
fn unit(p: &Plane) -> Plane {
    let n = Vector3::from(p.normal);
    let s = n.norm();
    Plane {
        normal: (n / s).into(),
        distance: p.distance / s,
    }
}

pub(crate) fn offsets(
    truth: &Truth,
    cameras: &[usize],
    estimate: &Estimate,
    target: Option<Isometry3<f64>>,
) -> Option<Offsets> {
    let handeye = estimate.handeye?;
    let cams: Vec<Isometry3<f64>> = cameras
        .iter()
        .enumerate()
        .map(|(j, &i)| camera_offset(truth, estimate, &handeye, i, j))
        .collect::<Option<_>>()?;
    let planes = cameras
        .iter()
        .enumerate()
        .map(|(j, &i)| {
            let e = estimate.laser_planes.get(j)?.as_ref()?;
            let t = truth.plane(i)?;
            let t = unit(&Plane {
                normal: t.normal,
                distance: t.distance,
            });
            // n·p_e + d = 0 with p_e = R⁻¹(p − t): (R n)·p + d − (R n)·t = 0.
            let e = unit(e);
            let n = cams[j].rotation * Vector3::from(e.normal);
            let mut e = Plane {
                normal: n.into(),
                distance: e.distance - n.dot(&cams[j].translation.vector),
            };
            if n.dot(&Vector3::from(t.normal)) < 0.0 {
                e = Plane {
                    normal: (-n).into(),
                    distance: -e.distance,
                };
            }
            Some([t, e])
        })
        .collect();
    Some(Offsets {
        cameras: cams,
        target: truth.target().1.inverse() * target?,
        planes,
    })
}

fn finish(truth: &Truth, obs: &Observations, gated: bool, solved: Solved) -> Report {
    let Solved {
        workflow,
        cameras,
        rows,
        counts,
        mut residuals,
        mut estimate,
        target,
        export,
        views,
        quality,
        uncertainty,
        rig_se3_target,
    } = solved;
    estimate.offsets = offsets(truth, &cameras, &estimate, target);
    // Name each residual's capture, camera and target point.
    residuals.features.retain_mut(|f| {
        let [view, cam, feature] = f.raw;
        let (Some(&k), Some(&i)) = (views.get(view), cameras.get(cam)) else {
            return false;
        };
        let capture = &obs.captures[k];
        let Some(point) = capture.views[i].points.get(feature) else {
            return false;
        };
        f.capture.clone_from(&capture.id);
        f.camera.clone_from(&truth.gt.cameras[i].id);
        f.point = point.point;
        // Named now: the indices are not kept (nor written).
        f.raw = [0; 3];
        true
    });
    let laser = !truth.gt.lasers.is_empty();
    let pass = rows.iter().all(Row::within);
    Report {
        version: REPORT_VERSION,
        workflow,
        gate: if laser { "G5.3" } else { "G5.1" }.into(),
        gated,
        pass: !gated || pass,
        cameras: cameras
            .iter()
            .map(|&i| truth.gt.cameras[i].id.clone())
            .collect(),
        views: counts.0,
        points: counts.1,
        laser_pixels: counts.2,
        noise: (obs.noise != ObservationNoise::default()).then_some(obs.noise),
        rows,
        residuals,
        estimate,
        quality,
        uncertainty,
        procedure: String::new(),
        stages: Vec::new(),
        export,
        rig_se3_target,
    }
}

/// `report` with the laser planes (and their rows) of `from`, an earlier stage's report, when
/// its own workflow estimated none: a procedure's result keeps the latest planes it has.
pub(crate) fn with_planes(truth: &Truth, mut report: Report, from: &Report) -> Report {
    let has = |r: &Report| r.estimate.laser_planes.iter().any(Option::is_some);
    if has(&report) || !has(from) {
        return report;
    }
    let plane_rows: Vec<Row> = from
        .rows
        .iter()
        .filter(|r| {
            r.label.ends_with(" laser normal deg") || r.label.ends_with(" laser distance mm")
        })
        .cloned()
        .collect();
    let at = report
        .rows
        .iter()
        .position(|r| r.label == "hand-eye rotation deg")
        .unwrap_or(report.rows.len());
    report.rows.splice(at..at, plane_rows);
    report.estimate.laser_planes = from.estimate.laser_planes.clone();
    let cameras: Vec<usize> = report
        .cameras
        .iter()
        .filter_map(|id| truth.gt.cameras.iter().position(|c| &c.id == id))
        .collect();
    let target = report
        .estimate
        .offsets
        .as_ref()
        .map(|o| truth.target().1 * o.target);
    report.estimate.offsets = offsets(truth, &cameras, &report.estimate, target);
    report.pass = !report.gated || report.rows.iter().all(Row::within);
    report
}

pub(crate) fn single(
    truth: &Truth,
    obs: &Observations,
    poses: &[Isometry3<f64>],
    i: usize,
    eye_in_hand: bool,
    gated: bool,
    run: &StageRun,
) -> Result<Report> {
    let (export, json, quality, views, points) = run_single(
        obs,
        poses,
        i,
        eye_in_hand,
        run,
        truth.gt.cameras[i].resolution,
    )?;
    let camera = camera_of(
        &export.camera,
        &truth.gt.cameras[i].id,
        Isometry3::identity(),
    )?;
    let (he, target) = single_handeye(truth, &export.handeye)?;
    let mut rows = vec![reprojection_row(export.mean_reproj_error)];
    rows.extend(intrinsics_rows(&camera, &truth.gt.cameras[i]));
    if run.scheimpflug {
        rows.extend(tilt_rows(&camera, &truth.gt.cameras[i]));
    }
    rows.extend(handeye_rows(truth, (he, target), &truth.cam_se3_rig[i]));
    let estimate = Estimate {
        cameras: vec![camera],
        handeye: Some(he),
        laser_planes: vec![],
        offsets: None,
    };
    Ok(finish(
        truth,
        obs,
        gated,
        Solved {
            workflow: Workflow::SingleCamHandeye,
            cameras: vec![i],
            rows,
            counts: (views, points, 0),
            residuals: residuals(&export.diagnostics, export.mean_reproj_error, 1),
            estimate,
            target: Some(target),
            export: json,
            views: used_views(obs, |c| correspondences(&c.views[i]).is_some()),
            quality,
            uncertainty: predicted(&export.uncertainty),
            rig_se3_target: Vec::new(),
        },
    ))
}

fn reprojection_row(mean_px: f64) -> Row {
    Row::new("mean reprojection px", mean_px, Some(gate::MEAN_REPROJ_PX))
}

/// The estimated cameras of a rig export, in the truth's camera order.
fn rig_estimate(
    truth: &Truth,
    cameras: &[CameraParams],
    cam_se3_rig: &[Isometry3<f64>],
) -> Result<Vec<EstimatedCamera>> {
    let n = truth.gt.cameras.len();
    ensure!(
        cameras.len() == n && cam_se3_rig.len() == n,
        "calibration-rs returned {} cameras and {} rig poses for a rig of {n}",
        cameras.len(),
        cam_se3_rig.len()
    );
    truth
        .gt
        .cameras
        .iter()
        .zip(cameras)
        .zip(cam_se3_rig)
        .map(|((c, p), x)| camera_of(p, &c.id, *x))
        .collect()
}

/// The rows of a rig workflow: the mean reprojection error; per camera its intrinsics, its
/// `cam_se3_rig` and `per_camera`'s; then the hand-eye's, with one.
fn rig_rows(
    truth: &Truth,
    mean_px: f64,
    cameras: &[EstimatedCamera],
    handeye: Option<(Isometry3<f64>, Isometry3<f64>)>,
    mut per_camera: impl FnMut(usize) -> Vec<Row>,
) -> Vec<Row> {
    let ref_se3_rig = truth.cam_se3_rig[0];
    let rig_se3_ref = ref_se3_rig.inverse();
    let mut rows = vec![reprojection_row(mean_px)];
    for (i, (e, t)) in cameras.iter().zip(&truth.gt.cameras).enumerate() {
        rows.extend(intrinsics_rows(e, t));
        if i > 0 {
            let (r, d) = difference(&e.cam_se3_ref, &(truth.cam_se3_rig[i] * rig_se3_ref));
            rows.push(Row::new(format!("{} cam_se3_rig deg", t.id), r, None));
            rows.push(Row::new(format!("{} cam_se3_rig mm", t.id), d, None));
        }
        rows.extend(per_camera(i));
    }
    if let Some(pair) = handeye {
        rows.extend(handeye_rows(truth, pair, &ref_se3_rig));
    }
    rows
}

/// The captures a rig sees the target in, each with every camera's correspondences (by capture
/// index), and the target points in them.
type RigCaptures = (Vec<(usize, Vec<Option<CorrespondenceView>>)>, usize);

fn rig_captures(obs: &Observations) -> RigCaptures {
    let mut points = 0;
    let captures = obs
        .captures
        .iter()
        .enumerate()
        .filter_map(|(k, c)| {
            let cams: Vec<Option<CorrespondenceView>> =
                c.views.iter().map(correspondences).collect();
            cams.iter().any(Option::is_some).then(|| {
                points += cams
                    .iter()
                    .flatten()
                    .map(CorrespondenceView::len)
                    .sum::<usize>();
                (k, cams)
            })
        })
        .collect();
    (captures, points)
}

fn rig_views_used(obs: &Observations) -> Vec<usize> {
    used_views(obs, |c| {
        c.views.iter().any(|v| correspondences(v).is_some())
    })
}

pub(crate) fn rig(
    truth: &Truth,
    obs: &Observations,
    poses: &[Isometry3<f64>],
    gated: bool,
    run: &StageRun,
) -> Result<Report> {
    let n = truth.gt.cameras.len();
    let all: Vec<usize> = (0..n).collect();
    let (captures, points) = rig_captures(obs);
    let views: Vec<_> = captures
        .into_iter()
        .map(|(k, cameras)| RigView {
            obs: RigViewObs { cameras },
            meta: RobotPoseMeta {
                base_se3_gripper: poses[k],
            },
        })
        .collect();
    let count = views.len();
    let mut session = CalibrationSession::<rig_handeye::RigHandeyeProblem>::new();
    session.set_input(RigDataset::new(views, n)?)?;
    session.set_config(config(&run.patch)?)?;
    let s = &run.seeds;
    if s.cam_se3_rig.is_none() && s.handeye.is_none() {
        // Intrinsics seeds, if any, are in the config's `manual_init`.
        rig_handeye::run_calibration(&mut session)?;
    } else {
        let manual = strict(
            s.rig_manual_init(&all).unwrap_or_else(|| json!({})),
            "intrinsics seed",
        )?;
        rig_handeye::step_intrinsics_init_all_with_seed(&mut session, manual, None)?;
        rig_handeye::step_intrinsics_optimize_all(&mut session, None)?;
        if let Some(rigs) = &s.cam_se3_rig {
            let manual = strict(
                json!({ "cam_se3_rig": rigs, "rig_se3_target": s.rig_se3_target }),
                "rig seed",
            )?;
            rig_handeye::step_rig_init_with_seed(&mut session, manual, None)?;
        } else {
            rig_handeye::step_rig_init(&mut session, None)?;
        }
        rig_handeye::step_rig_optimize(&mut session, None)?;
        if let Some(he) = &s.handeye {
            let manual = strict(json!({ "handeye": he }), "hand-eye seed")?;
            rig_handeye::step_handeye_init_with_seed(&mut session, manual, None)?;
        } else {
            rig_handeye::step_handeye_init(&mut session, None)?;
        }
        rig_handeye::step_handeye_optimize(&mut session, None)?;
    }
    let export = session.export()?;
    let (json, quality) = exported(&export, &sizes(truth, &all))?;
    let cameras = rig_estimate(truth, &export.cameras, &export.cam_se3_rig)?;
    let (he, target) = rig_handeye(truth, &export.handeye)?;
    let rows = rig_rows(
        truth,
        export.mean_reproj_error,
        &cameras,
        Some((he, target)),
        |i| {
            if run.scheimpflug {
                tilt_rows(&cameras[i], &truth.gt.cameras[i])
            } else {
                vec![]
            }
        },
    );
    let estimate = Estimate {
        cameras,
        handeye: Some(he),
        laser_planes: vec![],
        offsets: None,
    };
    Ok(finish(
        truth,
        obs,
        gated,
        Solved {
            workflow: Workflow::RigHandeye,
            cameras: all,
            rows,
            counts: (count, points, 0),
            residuals: residuals(&export.diagnostics, export.mean_reproj_error, n),
            estimate,
            target: Some(target),
            export: json,
            views: rig_views_used(obs),
            quality,
            uncertainty: predicted(&export.uncertainty),
            rig_se3_target: export.rig_se3_target.clone(),
        },
    ))
}

pub(crate) fn rig_laser(
    truth: &Truth,
    obs: &Observations,
    poses: &[Isometry3<f64>],
    gated: bool,
    run: &StageRun,
) -> Result<Report> {
    let n = truth.gt.cameras.len();
    let all: Vec<usize> = (0..n).collect();
    let (captures, points) = rig_captures(obs);
    let mut pixels = 0;
    let views: Vec<Value> = captures
        .into_iter()
        .map(|(k, cams)| {
            let lasers: Vec<Option<Vec<[f64; 2]>>> =
                obs.captures[k].views.iter().map(laser_pixels).collect();
            pixels += lasers.iter().flatten().map(Vec::len).sum::<usize>();
            json!({
                "obs": {"cameras": cams, "laser_pixels": lasers},
                "meta": {"base_se3_gripper": poses[k]},
            })
        })
        .collect();
    let count = views.len();
    let input: rig_handeye_laserline::RigHandeyeLaserlineInput =
        strict(json!({"views": views, "num_cameras": n}), "laserline input")?;
    let mut session =
        CalibrationSession::<rig_handeye_laserline::RigHandeyeLaserlineProblem>::new();
    session.set_input(input)?;
    session.set_config(config(&run.patch)?)?;
    rig_handeye_laserline::run_calibration(&mut session)?;
    let export = session.export()?;
    let (json, quality) = exported(&export, &sizes(truth, &all))?;
    let cameras = rig_estimate(truth, &export.cameras, &export.cam_se3_rig)?;
    let planes: Vec<Option<Plane>> = (0..n)
        .map(|i| export.laser_planes_cam.get(i).map(plane_of))
        .collect();
    let (he, target) = rig_handeye(truth, &export.handeye)?;
    let rows = rig_rows(
        truth,
        export.mean_reproj_error,
        &cameras,
        Some((he, target)),
        |i| {
            let mut rows = if run.scheimpflug {
                tilt_rows(&cameras[i], &truth.gt.cameras[i])
            } else {
                vec![]
            };
            if let (Some(e), Some(t)) = (&planes[i], truth.plane(i)) {
                rows.extend(plane_rows(e, t, &truth.gt.cameras[i].id));
            }
            rows
        },
    );
    let estimate = Estimate {
        cameras,
        handeye: Some(he),
        laser_planes: planes,
        offsets: None,
    };
    Ok(finish(
        truth,
        obs,
        gated,
        Solved {
            workflow: Workflow::RigHandeyeLaserline,
            cameras: all,
            rows,
            counts: (count, points, pixels),
            residuals: residuals(&export.diagnostics, export.mean_reproj_error, n),
            estimate,
            target: Some(target),
            export: json,
            views: rig_views_used(obs),
            quality,
            uncertainty: predicted(&export.uncertainty),
            rig_se3_target: export.rig_se3_target.clone(),
        },
    ))
}

/// One camera carrying its laser: the laserline device workflow (`device`) for intrinsics, tilt
/// and plane, then the single-camera hand-eye workflow on the same views for the hand-eye, with
/// the device's camera held. The plane is in that camera's frame; a camera re-estimated for the
/// hand-eye would settle on another frame, and the plane and the hand-eye would not compose
/// (`docs/measurements/head_triangulation.md`).
pub(crate) fn head(
    truth: &Truth,
    obs: &Observations,
    poses: &[Isometry3<f64>],
    eye_in_hand: bool,
    gated: bool,
    device_run: &StageRun,
) -> Result<Report> {
    let mut views = Vec::new();
    let mut points = 0;
    let mut pixels = 0;
    for c in &obs.captures {
        let v = &c.views[0];
        if let (Some(cv), Some(px)) = (correspondences(v), laser_pixels(v)) {
            points += cv.len();
            pixels += px.len();
            views.push(json!({"obs": cv, "meta": {"laser_pixels": px, "laser_weights": vec![1.0; px.len()]}}));
        }
    }
    let count = views.len();
    let input: laserline_device::LaserlineDeviceInput =
        strict(Value::Array(views), "laserline device input")?;
    let tilted = device_run.scheimpflug;
    let mut session = CalibrationSession::<laserline_device::LaserlineDeviceProblem>::new();
    session.set_input(input)?;
    session.set_config(config(&device_run.patch)?)?;
    let seed: DeviceManualInit = strict(device_run.seeds.camera_init(0), "device seed")?;
    laserline_device::step_init_with_seed(&mut session, seed, None)?;
    laserline_device::step_optimize(&mut session, None)?;
    let device = session.export()?;
    let (device_json, quality) = exported(&device, &sizes(truth, &[0]))?;
    let camera = camera_of(
        &device.params.camera,
        &truth.gt.cameras[0].id,
        Isometry3::identity(),
    )?;
    let plane = plane_of(&device.params.plane);
    let mut rows = vec![reprojection_row(device.mean_reproj_error)];
    rows.extend(intrinsics_rows(&camera, &truth.gt.cameras[0]));
    if tilted {
        rows.extend(tilt_rows(&camera, &truth.gt.cameras[0]));
    }
    if let Some(t) = truth.plane(0) {
        rows.extend(plane_rows(&plane, t, &truth.gt.cameras[0].id));
    }
    // The hand-eye, from the same target views, with the device's camera held.
    let mut held = json!({"fix": {
        "intrinsics": ["fx", "fy", "cx", "cy"],
        "distortion": ["k1", "k2", "k3", "p1", "p2"],
    }});
    if tilted {
        held["model"] = json!({"sensor": "scheimpflug"});
        held["fix"]["sensor"] = json!(["tilt_x", "tilt_y"]);
    }
    let handeye_run = StageRun {
        patch: json!({"handeye_init": {"handeye_mode": handeye_mode(eye_in_hand)}, "camera": held}),
        seeds: SeedValues {
            intrinsics: Some(vec![camera.intrinsics]),
            distortion: Some(vec![Some(camera.distortion)]),
            sensor: Some(vec![camera.tilt]),
            ..SeedValues::default()
        },
        scheimpflug: tilted,
    };
    let (he_export, he_json, _, _, _) = run_single(
        obs,
        poses,
        0,
        eye_in_hand,
        &handeye_run,
        truth.gt.cameras[0].resolution,
    )?;
    let (he, target) = single_handeye(truth, &he_export.handeye)?;
    rows.extend(handeye_rows(truth, (he, target), &truth.cam_se3_rig[0]));
    let estimate = Estimate {
        cameras: vec![camera],
        handeye: Some(he),
        laser_planes: vec![Some(plane)],
        offsets: None,
    };
    Ok(finish(
        truth,
        obs,
        gated,
        Solved {
            workflow: Workflow::LaserlineDevice,
            cameras: vec![0],
            rows,
            counts: (count, points, pixels),
            residuals: residuals(&device.diagnostics, device.mean_reproj_error, 1),
            estimate,
            target: Some(target),
            export: json!({"laserline_device": device_json, "single_cam_handeye": he_json}),
            views: used_views(obs, |c| {
                correspondences(&c.views[0]).is_some() && laser_pixels(&c.views[0]).is_some()
            }),
            quality,
            uncertainty: predicted(&device.uncertainty)
                .or_else(|| predicted(&he_export.uncertainty)),
            rig_se3_target: Vec::new(),
        },
    ))
}

/// One camera's intrinsics from its planar views, no robot (`planar_intrinsics`).
pub(crate) fn intrinsics(
    truth: &Truth,
    obs: &Observations,
    i: usize,
    gated: bool,
    run: &StageRun,
) -> Result<Report> {
    let (views, points) = planar_views(obs, i);
    let count = views.len();
    let mut session = CalibrationSession::<planar_intrinsics::PlanarIntrinsicsProblem>::new();
    session.set_input(PlanarDataset::new(
        views.into_iter().map(|v| View::new(v, NoMeta)).collect(),
    )?)?;
    session.set_config(config(&run.patch)?)?;
    let s = &run.seeds;
    if s.intrinsics.is_some() || s.sensor.is_some() {
        let manual = strict(s.camera_init(i), "intrinsics seed")?;
        planar_intrinsics::step_init_with_seed(&mut session, manual, None)?;
        planar_intrinsics::step_optimize(&mut session, None)?;
    } else {
        planar_intrinsics::run_calibration(&mut session)?;
    }
    let export = session.export()?;
    let (json, quality) = exported(&export, &sizes(truth, &[i]))?;
    let camera = camera_of(
        &export.params.camera,
        &truth.gt.cameras[i].id,
        Isometry3::identity(),
    )?;
    let mut rows = vec![reprojection_row(export.mean_reproj_error)];
    rows.extend(intrinsics_rows(&camera, &truth.gt.cameras[i]));
    if run.scheimpflug {
        rows.extend(tilt_rows(&camera, &truth.gt.cameras[i]));
    }
    let estimate = Estimate {
        cameras: vec![camera],
        handeye: None,
        laser_planes: vec![],
        offsets: None,
    };
    Ok(finish(
        truth,
        obs,
        gated,
        Solved {
            workflow: Workflow::PlanarIntrinsics,
            cameras: vec![i],
            rows,
            counts: (count, points, 0),
            residuals: residuals(&export.diagnostics, export.mean_reproj_error, 1),
            estimate,
            target: None,
            export: json,
            views: used_views(obs, |c| correspondences(&c.views[i]).is_some()),
            quality,
            uncertainty: predicted(&export.uncertainty),
            rig_se3_target: Vec::new(),
        },
    ))
}

/// A rig's cameras and extrinsics from planar views, no robot (`rig_extrinsics`).
pub(crate) fn rig_extrinsics(
    truth: &Truth,
    obs: &Observations,
    gated: bool,
    run: &StageRun,
) -> Result<Report> {
    let n = truth.gt.cameras.len();
    let all: Vec<usize> = (0..n).collect();
    let (captures, points) = rig_captures(obs);
    let views: Vec<_> = captures
        .into_iter()
        .map(|(_, cameras)| RigView {
            obs: RigViewObs { cameras },
            meta: NoMeta,
        })
        .collect();
    let count = views.len();
    let mut session = CalibrationSession::<rig_extrinsics::RigExtrinsicsProblem>::new();
    session.set_input(RigDataset::new(views, n)?)?;
    session.set_config(config(&run.patch)?)?;
    let s = &run.seeds;
    if s.none() {
        rig_extrinsics::run_calibration(&mut session)?;
    } else {
        let manual = strict(
            s.rig_manual_init(&all).unwrap_or_else(|| json!({})),
            "intrinsics seed",
        )?;
        rig_extrinsics::step_intrinsics_init_all_with_seed(&mut session, manual, None)?;
        rig_extrinsics::step_intrinsics_optimize_all(&mut session, None)?;
        if let Some(rigs) = &s.cam_se3_rig {
            let manual = strict(
                json!({ "cam_se3_rig": rigs, "rig_se3_target": s.rig_se3_target }),
                "rig seed",
            )?;
            rig_extrinsics::step_rig_init_with_seed(&mut session, manual, None)?;
        } else {
            rig_extrinsics::step_rig_init(&mut session, None)?;
        }
        rig_extrinsics::step_rig_optimize(&mut session, None)?;
    }
    let export = session.export()?;
    let (json, quality) = exported(&export, &sizes(truth, &all))?;
    let cameras = rig_estimate(truth, &export.cameras, &export.cam_se3_rig)?;
    let rows = rig_rows(truth, export.mean_reproj_error, &cameras, None, |i| {
        if run.scheimpflug {
            tilt_rows(&cameras[i], &truth.gt.cameras[i])
        } else {
            vec![]
        }
    });
    let estimate = Estimate {
        cameras,
        handeye: None,
        laser_planes: vec![],
        offsets: None,
    };
    Ok(finish(
        truth,
        obs,
        gated,
        Solved {
            workflow: Workflow::RigExtrinsics,
            cameras: all,
            rows,
            counts: (count, points, 0),
            residuals: residuals(&export.diagnostics, export.mean_reproj_error, n),
            estimate,
            target: None,
            export: json,
            views: rig_views_used(obs),
            quality,
            uncertainty: predicted(&export.uncertainty),
            rig_se3_target: export.rig_se3_target.clone(),
        },
    ))
}

/// Statistics of a row over trials.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RowStats {
    pub label: String,
    pub mean: f64,
    pub std: f64,
    pub max: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<f64>,
}

/// One estimated parameter over trials, beside its truth and calibration-rs's predicted σ.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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

/// How [`run_dir`] calibrates a dataset directory.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct RunOptions {
    /// Noise on the analytic observations (`seed` is the first trial's).
    pub noise: ObservationNoise,
    /// Trials, with seeds `seed, seed + 1, …` (at least one).
    pub trials: u32,
    /// Calibrate this camera of a rig alone.
    pub camera: Option<String>,
    /// Calibrate `etendue detect`'s observations (`features.json`, `laser_features.json`)
    /// instead of the ground truth's.
    pub detected: bool,
    /// The procedure to run, by value (so a kept run says exactly what ran); `None` for `auto`.
    pub procedure: Option<RunProcedure>,
}

/// A procedure as a run keeps it: its id and its stages.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunProcedure {
    pub id: String,
    pub procedure: etendue_scene::ProcedurePart,
}

/// What [`run_dir`] found: every trial's report and, with several, each row's spread.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Run {
    pub reports: Vec<Report>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_stats: Option<Vec<RowStats>>,
    /// With several trials, each estimated parameter's spread ([`param_stats`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub param_stats: Option<Vec<ParamStats>>,
}

/// Calibrate the dataset in `dir` (`gt.json`, and `etendue detect`'s files when `detected`),
/// reporting each trial through `ctl`. [`write_run`] keeps the run.
///
/// # Errors
///
/// Unreadable files, bad noise, a failed workflow, or cancellation (`progress::Cancelled`).
pub fn run_dir(
    dir: &std::path::Path,
    options: &RunOptions,
    ctl: &crate::progress::Control,
) -> Result<Run> {
    use crate::progress::Stage;
    let gt: GroundTruth = crate::read_json(&dir.join("gt.json"), "ground truth")?;
    let problems = options.noise.problems();
    ensure!(
        problems.is_empty(),
        "observation noise: {}",
        problems.join("; ")
    );
    let trials = options.trials.max(1);
    ensure!(
        !options.detected || (options.noise == ObservationNoise::default() && trials == 1),
        "noise and trials apply to the analytic observations, not to detected ones"
    );
    let detected = if options.detected {
        let found: crate::detect::Features =
            crate::read_json(&dir.join("features.json"), "features")?;
        let stripes: Option<crate::detect::laser::LaserFeatures> = if gt.lasers.is_empty() {
            None
        } else {
            Some(crate::read_json(
                &dir.join(crate::detect::LASER_FEATURES),
                "laser features",
            )?)
        };
        Some(from_features(&gt, &found, stripes.as_ref())?)
    } else {
        None
    };
    let calibrate_options = Options {
        camera: options.camera.clone(),
        detected: options.detected,
        procedure: options
            .procedure
            .as_ref()
            .map(|p| (p.id.clone(), p.procedure.clone())),
    };
    let mut reports = Vec::with_capacity(trials as usize);
    for k in 0..trials {
        ctl.check()?;
        ctl.step(Stage::Calibrate, k as usize, trials as usize);
        let obs = match &detected {
            Some(obs) => obs.clone(),
            None => etendue_synth::observe::observe(
                &gt,
                &ObservationNoise {
                    seed: options.noise.seed.wrapping_add(u64::from(k)),
                    ..options.noise
                },
            ),
        };
        let mut report = calibrate(&gt, &obs, &calibrate_options)?;
        if k > 0 {
            // Every point's residual is kept for the first trial only.
            report.residuals.features.clear();
        }
        ctl.log(format!(
            "trial {}: {}, mean reprojection {:.4} px, {}",
            k + 1,
            report.workflow,
            report.residuals.mean_px,
            if !report.gated {
                "closed-loop check not applied"
            } else if report.pass {
                "closed-loop check: PASS"
            } else {
                "closed-loop check: FAIL"
            }
        ));
        reports.push(report);
    }
    ctl.step(Stage::Calibrate, trials as usize, trials as usize);
    Ok(Run {
        row_stats: (reports.len() > 1).then(|| row_stats(&reports)),
        param_stats: (reports.len() > 1).then(|| param_stats(&gt, &reports)),
        reports,
    })
}

/// A kept run's version.
pub const RUN_VERSION: u32 = 1;
/// A kept run's files ([ADR 0009](https://github.com/VitalyVorobyev/etendue/blob/main/docs/adrs/0009-project-folder.md),
/// `runs/<name>/`): what was calibrated and how; the report ([`Run`]); calibration-rs's own
/// export of the first trial.
pub const RUN_CONFIG: &str = "config.json";
pub const RUN_REPORT: &str = "report.json";
pub const RUN_RESULT: &str = "result.json";

/// What a kept run calibrated, and how.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunConfig {
    pub version: u32,
    /// The dataset directory, as given.
    pub dataset: String,
    pub options: RunOptions,
}

/// Keep `run` of `dataset` in `folder` (created): [`RUN_CONFIG`], [`RUN_REPORT`] and
/// [`RUN_RESULT`].
///
/// # Errors
///
/// When a file cannot be written.
pub fn write_run(
    folder: &std::path::Path,
    dataset: &std::path::Path,
    options: &RunOptions,
    run: &Run,
) -> Result<()> {
    std::fs::create_dir_all(folder).with_context(|| format!("creating {}", folder.display()))?;
    let config = RunConfig {
        version: RUN_VERSION,
        dataset: dataset.to_string_lossy().into_owned(),
        options: options.clone(),
    };
    let result = run
        .reports
        .first()
        .map_or(Value::Null, |r| r.export.clone());
    for (name, text) in [
        (RUN_CONFIG, serde_json::to_string_pretty(&config)?),
        (RUN_REPORT, serde_json::to_string_pretty(run)?),
        (RUN_RESULT, serde_json::to_string_pretty(&result)?),
    ] {
        let path = folder.join(name);
        std::fs::write(&path, text + "\n")
            .with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}

/// A kept run, read back.
#[derive(Clone, Debug, Serialize)]
pub struct StoredRun {
    /// Its folder's name.
    pub name: String,
    pub config: RunConfig,
    pub run: Run,
}

/// The runs kept under `folder` (one subfolder each), by name; a folder that does not hold a
/// readable run is skipped. A missing `folder` has none.
///
/// # Errors
///
/// When `folder` exists but cannot be listed.
pub fn read_runs(folder: &std::path::Path) -> Result<Vec<StoredRun>> {
    if !folder.is_dir() {
        return Ok(Vec::new());
    }
    let mut runs = Vec::new();
    for entry in
        std::fs::read_dir(folder).with_context(|| format!("listing {}", folder.display()))?
    {
        let path = entry?.path();
        let read = || -> Result<StoredRun> {
            Ok(StoredRun {
                name: path
                    .file_name()
                    .context("a run folder has a name")?
                    .to_string_lossy()
                    .into_owned(),
                config: crate::read_json(&path.join(RUN_CONFIG), "run config")?,
                run: crate::read_json(&path.join(RUN_REPORT), "run report")?,
            })
        };
        if path.is_dir()
            && let Ok(run) = read()
        {
            runs.push(run);
        }
    }
    runs.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(runs)
}
