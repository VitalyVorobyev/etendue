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
use etendue_synth::dataset::{GroundTruth, GtCamera, GtHandeye};
use etendue_synth::laser::GtPlane;
use etendue_synth::observe::{
    ObservationNoise, Observations, ObservedCapture, ObservedPoint, ObservedView,
};
use nalgebra::{Isometry3, Vector3};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use vision_calibration_core::{
    CorrespondenceView, NoMeta, PlanarDataset, Pt2, Pt3, RigDataset, RigView, RigViewObs,
    SensorParams, View,
};
use vision_calibration_pipeline::common::RobotPoseMeta;
use vision_calibration_pipeline::laserline_device::DeviceManualInit;
use vision_calibration_pipeline::session::CalibrationSession;
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

fn intrinsics_rows(e: &EstimatedCamera, truth: &GtCamera) -> Result<Vec<Row>> {
    let p = serde_json::to_value(&truth.params)?;
    let k = &p["intrinsics"];
    let d = &p["distortion"];
    let num = |v: &Value, key: &str| v[key].as_f64().unwrap_or(0.0);
    let name = &truth.id;
    let [fx, fy, cx, cy] = e.intrinsics;
    let mut rows = vec![
        Row::new(
            format!("{name} fx rel"),
            (fx - num(k, "fx")).abs() / num(k, "fx"),
            Some(gate::FOCAL_REL),
        ),
        Row::new(
            format!("{name} fy rel"),
            (fy - num(k, "fy")).abs() / num(k, "fy"),
            Some(gate::FOCAL_REL),
        ),
        Row::new(
            format!("{name} cx px"),
            (cx - num(k, "cx")).abs(),
            Some(gate::PRINCIPAL_PX),
        ),
        Row::new(
            format!("{name} cy px"),
            (cy - num(k, "cy")).abs(),
            Some(gate::PRINCIPAL_PX),
        ),
    ];
    for (i, c) in ["k1", "k2", "k3", "p1", "p2"].iter().enumerate() {
        rows.push(Row::new(
            format!("{name} {c} abs"),
            (e.distortion[i] - num(d, c)).abs(),
            None,
        ));
    }
    Ok(rows)
}

fn tilt_rows(e: &EstimatedCamera, truth: &GtCamera) -> Vec<Row> {
    let (tx, ty) = match truth.params.sensor {
        SensorParams::Scheimpflug { params } => (params.tilt_x, params.tilt_y),
        _ => (0.0, 0.0),
    };
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

    /// The truth's plane of camera `i`'s own laser (the only laser, if none names it).
    pub(crate) fn plane(&self, i: usize) -> Option<&GtPlane> {
        let id = &self.gt.cameras[i].id;
        let laser = self
            .gt
            .lasers
            .iter()
            .find(|l| l.camera.as_deref() == Some(id.as_str()))
            .or_else(|| (self.gt.lasers.len() == 1).then(|| &self.gt.lasers[0]))?;
        laser.camera_planes.iter().find(|p| &p.frame == id)
    }
}

pub(crate) fn handeye_mode(eye_in_hand: bool) -> Value {
    json!(if eye_in_hand {
        "EyeInHand"
    } else {
        "EyeToHand"
    })
}

/// A config: `T::default()` with JSON `patch` merged in (the configs are `non_exhaustive`).
fn config<T: Serialize + for<'de> Deserialize<'de> + Default>(patch: &Value) -> Result<T> {
    let mut v = serde_json::to_value(T::default())?;
    merge(&mut v, patch);
    serde_json::from_value(v).context("calibration config")
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

/// The seed a user would enter for a Scheimpflug rig from datasheets, as the script makes it:
/// the focal length rounded to 100 px, the principal point at the image centre, the tilt about x
/// rounded to a whole degree.
pub(crate) fn nominal_seed(cameras: &[&GtCamera]) -> Value {
    let mut intrinsics = Vec::new();
    let mut sensors = Vec::new();
    for c in cameras {
        let p = serde_json::to_value(&c.params).unwrap_or_default();
        let k = &p["intrinsics"];
        let f = ((k["fx"].as_f64().unwrap_or(0.0) + k["fy"].as_f64().unwrap_or(0.0)) / 2.0 / 100.0)
            .round_ties_even()
            * 100.0;
        let [w, h] = c.resolution.map(f64::from);
        intrinsics.push(json!({"fx": f, "fy": f, "cx": w / 2.0, "cy": h / 2.0}));
        let tilt = match c.params.sensor {
            SensorParams::Scheimpflug { params } => params.tilt_x,
            _ => 0.0,
        };
        sensors.push(
            json!({"tilt_x": tilt.to_degrees().round_ties_even().to_radians(), "tilt_y": 0.0}),
        );
    }
    json!({"per_cam_intrinsics": intrinsics, "per_cam_distortion": null, "per_cam_sensors": sensors})
}

/// An estimated camera from an export's `CameraParams` JSON (its tilt from `sensor`, zero for an
/// untilted one).
fn camera_of(v: &Value, id: &str, cam_se3_ref: Isometry3<f64>) -> EstimatedCamera {
    let n = |x: &Value| x.as_f64().unwrap_or(f64::NAN);
    let k = &v["intrinsics"];
    let d = &v["distortion"];
    let s = &v["sensor"];
    let tilt = [
        s["tilt_x"].as_f64().unwrap_or(0.0),
        s["tilt_y"].as_f64().unwrap_or(0.0),
    ];
    EstimatedCamera {
        id: id.to_owned(),
        intrinsics: [n(&k["fx"]), n(&k["fy"]), n(&k["cx"]), n(&k["cy"])],
        distortion: [
            n(&d["k1"]),
            n(&d["k2"]),
            n(&d["k3"]),
            n(&d["p1"]),
            n(&d["p2"]),
        ],
        tilt,
        cam_se3_ref,
    }
}

fn plane_of(v: &Value) -> Option<Plane> {
    let n = v["normal"].as_array()?;
    Some(Plane {
        normal: [
            n.first()?.as_f64()?,
            n.get(1)?.as_f64()?,
            n.get(2)?.as_f64()?,
        ],
        distance: v["distance"].as_f64()?,
    })
}

fn iso(v: &Value) -> Option<Isometry3<f64>> {
    if v.is_null() {
        return None;
    }
    serde_json::from_value(v.clone()).ok()
}

/// The residual figures of an export's `per_feature_residuals`, for `cameras` cameras.
fn residuals(export: &Value, cameras: usize) -> Residuals {
    let mut errors = Vec::new();
    let mut sq = 0.0;
    let mut per_camera = vec![(0.0, 0usize); cameras];
    let mut features = Vec::new();
    for r in export["diagnostics"]["per_feature_residuals"]["target"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let (Some(o), Some(p)) = (r["observed_px"].as_array(), r["projected_px"].as_array()) else {
            continue;
        };
        let f = |a: &[Value], i: usize| a.get(i).and_then(Value::as_f64).unwrap_or(f64::NAN);
        let (du, dv) = (f(p, 0) - f(o, 0), f(p, 1) - f(o, 1));
        let e = du.hypot(dv);
        sq += du * du + dv * dv;
        errors.push(e);
        let c = r["camera"].as_u64().unwrap_or(0) as usize;
        if let Some(slot) = per_camera.get_mut(c) {
            slot.0 += e;
            slot.1 += 1;
        }
        let index = |k: &str| r[k].as_u64().unwrap_or(0) as usize;
        features.push(FeatureResidual {
            capture: String::new(),
            camera: String::new(),
            point: 0,
            observed_px: [f(o, 0), f(o, 1)],
            projected_px: [f(p, 0), f(p, 1)],
            raw: [index("pose"), c, index("feature")],
        });
    }
    let n = errors.len();
    Residuals {
        count: n,
        mean_px: export["mean_reproj_error"].as_f64().unwrap_or(f64::NAN),
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

/// An export as JSON, with calibration-rs's quality verdict for cameras of `sizes`.
fn exported<E>(export: E, sizes: &[[u32; 2]]) -> Result<(Value, Option<Value>)>
where
    E: Serialize + vision_calibration_pipeline::common::CalibratedCameras,
{
    let quality = vision_calibration_pipeline::analysis::assess(
        &export,
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

/// calibration-rs's covariance in `export` (or in the first workflow's of several), as σ.
fn predicted(export: &Value) -> Option<Predicted> {
    let u = export
        .get("uncertainty")
        .filter(|u| u.get("labels").is_some())
        .or_else(|| {
            export
                .as_object()?
                .values()
                .find_map(|v| v.get("uncertainty").filter(|u| u.get("labels").is_some()))
        })?;
    let labels: Vec<String> = serde_json::from_value(u["labels"].clone()).ok()?;
    let cov: Vec<Vec<f64>> = serde_json::from_value(u["covariance"].clone()).ok()?;
    let sigma = (0..labels.len())
        .map(|i| {
            cov.get(i)
                .and_then(|r| r.get(i))
                .copied()
                .unwrap_or(f64::NAN)
                .max(0.0)
                .sqrt()
        })
        .collect();
    Some(Predicted {
        labels,
        sigma,
        fixed: serde_json::from_value(u["fixed"].clone()).unwrap_or_default(),
        noise_px: u["noise"]
            .as_array()
            .and_then(|n| n.iter().find(|x| x["kind"] == "reprojection"))
            .and_then(|x| x["sigma"].as_f64()),
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
    /// `planar_intrinsics` and `single_cam_handeye`, and `DeviceManualInit`).
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

/// What a single-camera hand-eye run gives: the export, its quality verdict, views and points.
type SingleRun = (Value, Option<Value>, usize, usize);

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
            let manual = serde_json::from_value(s.camera_init(i)).context("intrinsics seed")?;
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
            let manual = serde_json::from_value(json!({ key: he })).context("hand-eye seed")?;
            single_cam_handeye::step_handeye_init_with_seed(&mut session, manual, None)?;
        } else {
            single_cam_handeye::step_handeye_init(&mut session, None)?;
        }
        single_cam_handeye::step_handeye_optimize(&mut session, None)?;
    }
    let (export, quality) = exported(session.export()?, &[size])?;
    Ok((export, quality, n, points))
}

fn handeye_rows(
    truth: &Truth,
    estimate: Option<Isometry3<f64>>,
    target: Option<Isometry3<f64>>,
    ref_se3_rig: &Isometry3<f64>,
) -> Result<Vec<Row>> {
    let he = estimate.context("the workflow gave no hand-eye transform")?;
    let (r, t) = difference(&he, &truth.handeye(ref_se3_rig));
    let (name, target_truth) = truth.target();
    let (tr, tt) = difference(
        &target.context("the workflow gave no target pose")?,
        &target_truth,
    );
    Ok(vec![
        Row::new("hand-eye rotation deg", r, Some(gate::HANDEYE_ROT_DEG)),
        Row::new("hand-eye translation mm", t, Some(gate::HANDEYE_TRANS_MM)),
        Row::new(format!("{name} rot deg"), tr, None),
        Row::new(format!("{name} trans mm"), tt, None),
    ])
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
    let uncertainty = predicted(&export);
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
    let (export, quality, views, points) = run_single(
        obs,
        poses,
        i,
        eye_in_hand,
        run,
        truth.gt.cameras[i].resolution,
    )?;
    let camera = camera_of(
        &export["camera"],
        &truth.gt.cameras[i].id,
        Isometry3::identity(),
    );
    let (he, target) = if eye_in_hand {
        (
            iso(&export["gripper_se3_camera"]),
            iso(&export["base_se3_target"]),
        )
    } else {
        (
            iso(&export["camera_se3_base"]),
            iso(&export["gripper_se3_target"]),
        )
    };
    let mut rows = vec![Row::new(
        "mean reprojection px",
        export["mean_reproj_error"].as_f64().unwrap_or(f64::NAN),
        Some(gate::MEAN_REPROJ_PX),
    )];
    rows.extend(intrinsics_rows(&camera, &truth.gt.cameras[i])?);
    if run.scheimpflug {
        rows.extend(tilt_rows(&camera, &truth.gt.cameras[i]));
    }
    rows.extend(handeye_rows(truth, he, target, &truth.cam_se3_rig[i])?);
    let estimate = Estimate {
        cameras: vec![camera],
        handeye: he,
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
            residuals: residuals(&export, 1),
            estimate,
            target,
            export,
            views: used_views(obs, |c| correspondences(&c.views[i]).is_some()),
            quality,
        },
    ))
}

/// The rig rows shared by the rig workflows: per camera intrinsics, `cam_se3_rig`, `extra`; then
/// hand-eye and target.
fn rig_estimate(truth: &Truth, export: &Value) -> Result<Vec<EstimatedCamera>> {
    let cams = export["cameras"].as_array().context("export cameras")?;
    let rigs = export["cam_se3_rig"]
        .as_array()
        .context("export cam_se3_rig")?;
    Ok(truth
        .gt
        .cameras
        .iter()
        .enumerate()
        .map(|(i, c)| {
            camera_of(
                &cams[i],
                &c.id,
                rigs.get(i)
                    .and_then(iso)
                    .unwrap_or_else(Isometry3::identity),
            )
        })
        .collect())
}

/// The hand-eye and the target pose a workflow reports.
type Reported = (Option<Isometry3<f64>>, Option<Isometry3<f64>>);

fn rig_rows(
    truth: &Truth,
    export: &Value,
    cameras: &[EstimatedCamera],
    eye_in_hand: bool,
    mut per_camera: impl FnMut(usize) -> Vec<Row>,
) -> Result<(Vec<Row>, Reported)> {
    let ref_se3_rig = truth.cam_se3_rig[0];
    let rig_se3_ref = ref_se3_rig.inverse();
    let mut rows = vec![Row::new(
        "mean reprojection px",
        export["mean_reproj_error"].as_f64().unwrap_or(f64::NAN),
        Some(gate::MEAN_REPROJ_PX),
    )];
    for (i, (e, t)) in cameras.iter().zip(&truth.gt.cameras).enumerate() {
        rows.extend(intrinsics_rows(e, t)?);
        if i > 0 {
            let (r, d) = difference(&e.cam_se3_ref, &(truth.cam_se3_rig[i] * rig_se3_ref));
            rows.push(Row::new(format!("{} cam_se3_rig deg", t.id), r, None));
            rows.push(Row::new(format!("{} cam_se3_rig mm", t.id), d, None));
        }
        rows.extend(per_camera(i));
    }
    let (he, target) = if eye_in_hand {
        (
            iso(&export["gripper_se3_rig"]),
            iso(&export["base_se3_target"]),
        )
    } else {
        (
            iso(&export["rig_se3_base"]),
            iso(&export["gripper_se3_target"]),
        )
    };
    rows.extend(handeye_rows(truth, he, target, &ref_se3_rig)?);
    Ok((rows, (he, target)))
}

pub(crate) fn rig(
    truth: &Truth,
    obs: &Observations,
    poses: &[Isometry3<f64>],
    eye_in_hand: bool,
    gated: bool,
    run: &StageRun,
) -> Result<Report> {
    let n = truth.gt.cameras.len();
    let mut views = Vec::new();
    let mut points = 0;
    for (c, pose) in obs.captures.iter().zip(poses) {
        let cams: Vec<Option<CorrespondenceView>> = c.views.iter().map(correspondences).collect();
        if cams.iter().any(Option::is_some) {
            points += cams
                .iter()
                .flatten()
                .map(CorrespondenceView::len)
                .sum::<usize>();
            views.push(RigView {
                obs: RigViewObs { cameras: cams },
                meta: RobotPoseMeta {
                    base_se3_gripper: *pose,
                },
            });
        }
    }
    let count = views.len();
    let mut session = CalibrationSession::<rig_handeye::RigHandeyeProblem>::new();
    session.set_input(RigDataset::new(views, n)?)?;
    session.set_config(config(&run.patch)?)?;
    let s = &run.seeds;
    if s.cam_se3_rig.is_none() && s.handeye.is_none() {
        // Intrinsics seeds, if any, are in the config's `manual_init`.
        rig_handeye::run_calibration(&mut session)?;
    } else {
        let all: Vec<usize> = (0..n).collect();
        let manual = serde_json::from_value(s.rig_manual_init(&all).unwrap_or_else(|| json!({})))
            .context("intrinsics seed")?;
        rig_handeye::step_intrinsics_init_all_with_seed(&mut session, manual, None)?;
        rig_handeye::step_intrinsics_optimize_all(&mut session, None)?;
        if let Some(rigs) = &s.cam_se3_rig {
            let manual = serde_json::from_value(
                json!({ "cam_se3_rig": rigs, "rig_se3_target": s.rig_se3_target }),
            )
            .context("rig seed")?;
            rig_handeye::step_rig_init_with_seed(&mut session, manual, None)?;
        } else {
            rig_handeye::step_rig_init(&mut session, None)?;
        }
        rig_handeye::step_rig_optimize(&mut session, None)?;
        if let Some(he) = &s.handeye {
            let manual =
                serde_json::from_value(json!({ "handeye": he })).context("hand-eye seed")?;
            rig_handeye::step_handeye_init_with_seed(&mut session, manual, None)?;
        } else {
            rig_handeye::step_handeye_init(&mut session, None)?;
        }
        rig_handeye::step_handeye_optimize(&mut session, None)?;
    }
    let (export, quality) = exported(
        session.export()?,
        &sizes(truth, &(0..n).collect::<Vec<_>>()),
    )?;
    let cameras = rig_estimate(truth, &export)?;
    let scheimpflug = run.scheimpflug;
    let (rows, (he, target)) = rig_rows(truth, &export, &cameras, eye_in_hand, |i| {
        if scheimpflug {
            tilt_rows(&cameras[i], &truth.gt.cameras[i])
        } else {
            vec![]
        }
    })?;
    let all: Vec<usize> = (0..n).collect();
    let estimate = Estimate {
        cameras,
        handeye: he,
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
            residuals: residuals(&export, n),
            estimate,
            target,
            export,
            views: used_views(obs, |c| {
                c.views.iter().any(|v| correspondences(v).is_some())
            }),
            quality,
        },
    ))
}

pub(crate) fn rig_laser(
    truth: &Truth,
    obs: &Observations,
    poses: &[Isometry3<f64>],
    eye_in_hand: bool,
    gated: bool,
    run: &StageRun,
) -> Result<Report> {
    let n = truth.gt.cameras.len();
    let mut views = Vec::new();
    let mut points = 0;
    let mut pixels = 0;
    for (c, pose) in obs.captures.iter().zip(poses) {
        let cams: Vec<Option<CorrespondenceView>> = c.views.iter().map(correspondences).collect();
        if cams.iter().any(Option::is_some) {
            points += cams
                .iter()
                .flatten()
                .map(CorrespondenceView::len)
                .sum::<usize>();
            let lasers: Vec<Option<Vec<[f64; 2]>>> = c.views.iter().map(laser_pixels).collect();
            pixels += lasers.iter().flatten().map(Vec::len).sum::<usize>();
            views.push(json!({
                "obs": {"cameras": cams, "laser_pixels": lasers},
                "meta": {"base_se3_gripper": pose},
            }));
        }
    }
    let count = views.len();
    let all: Vec<usize> = (0..n).collect();
    let scheimpflug = run.scheimpflug;
    let input: rig_handeye_laserline::RigHandeyeLaserlineInput =
        serde_json::from_value(json!({"views": views, "num_cameras": n}))
            .context("laserline input")?;
    let mut session =
        CalibrationSession::<rig_handeye_laserline::RigHandeyeLaserlineProblem>::new();
    session.set_input(input)?;
    session.set_config(config(&run.patch)?)?;
    rig_handeye_laserline::run_calibration(&mut session)?;
    let (export, quality) = exported(
        session.export()?,
        &sizes(truth, &(0..n).collect::<Vec<_>>()),
    )?;
    let cameras = rig_estimate(truth, &export)?;
    let planes: Vec<Option<Plane>> = (0..n)
        .map(|i| export["laser_planes_cam"].get(i).and_then(plane_of))
        .collect();
    let (rows, (he, target)) = rig_rows(truth, &export, &cameras, eye_in_hand, |i| {
        let mut rows = if scheimpflug {
            tilt_rows(&cameras[i], &truth.gt.cameras[i])
        } else {
            vec![]
        };
        if let (Some(e), Some(t)) = (&planes[i], truth.plane(i)) {
            rows.extend(plane_rows(e, t, &truth.gt.cameras[i].id));
        }
        rows
    })?;
    let estimate = Estimate {
        cameras,
        handeye: he,
        laser_planes: planes,
        offsets: None,
    };
    Ok(finish(
        truth,
        obs,
        gated,
        Solved {
            workflow: Workflow::RigHandeyeLaserline,
            cameras: all.clone(),
            rows,
            counts: (count, points, pixels),
            residuals: residuals(&export, n),
            estimate,
            target,
            export,
            views: used_views(obs, |c| {
                c.views.iter().any(|v| correspondences(v).is_some())
            }),
            quality,
        },
    ))
}

/// One camera carrying its laser: the laserline device workflow (`device`) for intrinsics, tilt
/// and plane, then the single-camera hand-eye workflow (`handeye`) on the same views for the
/// hand-eye.
pub(crate) fn head(
    truth: &Truth,
    obs: &Observations,
    poses: &[Isometry3<f64>],
    eye_in_hand: bool,
    gated: bool,
    device_run: &StageRun,
    handeye_run: &StageRun,
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
        serde_json::from_value(Value::Array(views)).context("laserline device input")?;
    let tilted = device_run.scheimpflug;
    let mut session = CalibrationSession::<laserline_device::LaserlineDeviceProblem>::new();
    session.set_input(input)?;
    session.set_config(config(&device_run.patch)?)?;
    let seed: DeviceManualInit =
        serde_json::from_value(device_run.seeds.camera_init(0)).context("device seed")?;
    laserline_device::step_init_with_seed(&mut session, seed, None)?;
    laserline_device::step_optimize(&mut session, None)?;
    let (device, quality) = exported(session.export()?, &sizes(truth, &[0]))?;
    let p = &device["params"];
    let mut camera = camera_of(&p["camera"], &truth.gt.cameras[0].id, Isometry3::identity());
    let plane = plane_of(&p["plane"]);
    let mut rows = vec![Row::new(
        "mean reprojection px",
        device["mean_reproj_error"].as_f64().unwrap_or(f64::NAN),
        Some(gate::MEAN_REPROJ_PX),
    )];
    rows.extend(intrinsics_rows(&camera, &truth.gt.cameras[0])?);
    if tilted {
        rows.extend(tilt_rows(&camera, &truth.gt.cameras[0]));
    }
    if let (Some(e), Some(t)) = (&plane, truth.plane(0)) {
        rows.extend(plane_rows(e, t, &truth.gt.cameras[0].id));
    }
    // The hand-eye, from the same target views (by default a pinhole workflow: on a Scheimpflug
    // camera its intrinsics absorb the tilt, so only its hand-eye is kept).
    let (he_export, _, _, _) = run_single(
        obs,
        poses,
        0,
        eye_in_hand,
        handeye_run,
        truth.gt.cameras[0].resolution,
    )?;
    let (he, target) = if eye_in_hand {
        (
            iso(&he_export["gripper_se3_camera"]),
            iso(&he_export["base_se3_target"]),
        )
    } else {
        (
            iso(&he_export["camera_se3_base"]),
            iso(&he_export["gripper_se3_target"]),
        )
    };
    rows.extend(handeye_rows(truth, he, target, &truth.cam_se3_rig[0])?);
    camera.cam_se3_ref = Isometry3::identity();
    let estimate = Estimate {
        cameras: vec![camera],
        handeye: he,
        laser_planes: vec![plane],
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
            residuals: residuals(&device, 1),
            estimate,
            target,
            export: json!({"laserline_device": device, "single_cam_handeye": he_export}),
            views: used_views(obs, |c| {
                correspondences(&c.views[0]).is_some() && laser_pixels(&c.views[0]).is_some()
            }),
            quality,
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
        let manual = serde_json::from_value(s.camera_init(i)).context("intrinsics seed")?;
        planar_intrinsics::step_init_with_seed(&mut session, manual, None)?;
        planar_intrinsics::step_optimize(&mut session, None)?;
    } else {
        planar_intrinsics::run_calibration(&mut session)?;
    }
    let (export, quality) = exported(session.export()?, &sizes(truth, &[i]))?;
    let camera = camera_of(
        &export["params"]["camera"],
        &truth.gt.cameras[i].id,
        Isometry3::identity(),
    );
    let mut rows = vec![Row::new(
        "mean reprojection px",
        export["mean_reproj_error"].as_f64().unwrap_or(f64::NAN),
        Some(gate::MEAN_REPROJ_PX),
    )];
    rows.extend(intrinsics_rows(&camera, &truth.gt.cameras[i])?);
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
            residuals: residuals(&export, 1),
            estimate,
            target: None,
            export,
            views: used_views(obs, |c| correspondences(&c.views[i]).is_some()),
            quality,
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
    let mut views = Vec::new();
    let mut points = 0;
    for c in &obs.captures {
        let cams: Vec<Option<CorrespondenceView>> = c.views.iter().map(correspondences).collect();
        if cams.iter().any(Option::is_some) {
            points += cams
                .iter()
                .flatten()
                .map(CorrespondenceView::len)
                .sum::<usize>();
            views.push(RigView {
                obs: RigViewObs { cameras: cams },
                meta: NoMeta,
            });
        }
    }
    let count = views.len();
    let mut session = CalibrationSession::<rig_extrinsics::RigExtrinsicsProblem>::new();
    session.set_input(RigDataset::new(views, n)?)?;
    session.set_config(config(&run.patch)?)?;
    let s = &run.seeds;
    if s.none() {
        rig_extrinsics::run_calibration(&mut session)?;
    } else {
        let all: Vec<usize> = (0..n).collect();
        let manual = serde_json::from_value(s.rig_manual_init(&all).unwrap_or_else(|| json!({})))
            .context("intrinsics seed")?;
        rig_extrinsics::step_intrinsics_init_all_with_seed(&mut session, manual, None)?;
        rig_extrinsics::step_intrinsics_optimize_all(&mut session, None)?;
        if let Some(rigs) = &s.cam_se3_rig {
            let manual = serde_json::from_value(
                json!({ "cam_se3_rig": rigs, "rig_se3_target": s.rig_se3_target }),
            )
            .context("rig seed")?;
            rig_extrinsics::step_rig_init_with_seed(&mut session, manual, None)?;
        } else {
            rig_extrinsics::step_rig_init(&mut session, None)?;
        }
        rig_extrinsics::step_rig_optimize(&mut session, None)?;
    }
    let (export, quality) = exported(
        session.export()?,
        &sizes(truth, &(0..n).collect::<Vec<_>>()),
    )?;
    let cameras = rig_estimate(truth, &export)?;
    let ref_se3_rig = truth.cam_se3_rig[0];
    let rig_se3_ref = ref_se3_rig.inverse();
    let mut rows = vec![Row::new(
        "mean reprojection px",
        export["mean_reproj_error"].as_f64().unwrap_or(f64::NAN),
        Some(gate::MEAN_REPROJ_PX),
    )];
    for (i, (e, t)) in cameras.iter().zip(&truth.gt.cameras).enumerate() {
        rows.extend(intrinsics_rows(e, t)?);
        if i > 0 {
            let (r, d) = difference(&e.cam_se3_ref, &(truth.cam_se3_rig[i] * rig_se3_ref));
            rows.push(Row::new(format!("{} cam_se3_rig deg", t.id), r, None));
            rows.push(Row::new(format!("{} cam_se3_rig mm", t.id), d, None));
        }
        if run.scheimpflug {
            rows.extend(tilt_rows(e, t));
        }
    }
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
            cameras: (0..n).collect(),
            rows,
            counts: (count, points, 0),
            residuals: residuals(&export, n),
            estimate,
            target: None,
            export,
            views: used_views(obs, |c| {
                c.views.iter().any(|v| correspondences(v).is_some())
            }),
            quality,
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
