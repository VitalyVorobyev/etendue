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
    CorrespondenceView, Pt2, Pt3, RigDataset, RigView, RigViewObs, SensorParams, View,
};
use vision_calibration_pipeline::session::CalibrationSession;
use vision_calibration_pipeline::{
    laserline_device, rig_handeye, rig_handeye_laserline, single_cam_handeye,
};

/// Report version.
pub const REPORT_VERSION: u32 = 2;

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
                self.gate,
                if self.pass { "PASS" } else { "FAIL" }
            );
        } else {
            let all = self.rows.iter().all(Row::within);
            let _ = writeln!(
                out,
                "  {} limits (not gated on this input): {}",
                self.gate,
                if all { "all within" } else { "not all within" }
            );
        }
        out
    }
}

/// What to calibrate.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Calibrate this camera of a rig alone (single-camera hand-eye).
    pub camera: Option<String>,
    /// The observations are a detector's ([`from_features`]), so no gate applies.
    pub detected: bool,
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
struct Truth<'a> {
    gt: &'a GroundTruth,
    /// `cam_se3_rig` per camera (identity without a rig).
    cam_se3_rig: Vec<Isometry3<f64>>,
}

impl<'a> Truth<'a> {
    fn new(gt: &'a GroundTruth) -> Self {
        Self {
            gt,
            cam_se3_rig: gt
                .cameras
                .iter()
                .map(|c| c.cam_se3_rig.unwrap_or_else(Isometry3::identity))
                .collect(),
        }
    }

    fn eye_in_hand(&self) -> Result<bool> {
        match &self.gt.handeye {
            Some(GtHandeye::EyeInHand { .. }) => Ok(true),
            Some(GtHandeye::EyeToHand { .. }) => Ok(false),
            None => bail!("the dataset has no robot: there is no hand-eye to calibrate"),
        }
    }

    /// The hand-eye transform of the frame `ref_se3_rig` maps into (the rig, a camera of it).
    fn handeye(&self, ref_se3_rig: &Isometry3<f64>) -> Isometry3<f64> {
        match self.gt.handeye.as_ref().expect("checked") {
            GtHandeye::EyeInHand {
                gripper_se3_rig, ..
            } => gripper_se3_rig * ref_se3_rig.inverse(),
            GtHandeye::EyeToHand { rig_se3_base, .. } => ref_se3_rig * rig_se3_base,
        }
    }

    /// The target in the frame the workflow reports it in: the base eye-in-hand, the gripper
    /// eye-to-hand.
    fn target(&self) -> (&'static str, Isometry3<f64>) {
        match self.gt.handeye.as_ref().expect("checked") {
            GtHandeye::EyeInHand {
                base_se3_target, ..
            } => ("base_se3_target", *base_se3_target),
            GtHandeye::EyeToHand {
                gripper_se3_target, ..
            } => ("gripper_se3_target", *gripper_se3_target),
        }
    }

    fn scheimpflug(&self, cameras: &[usize]) -> bool {
        cameras.iter().any(|&i| {
            matches!(
                self.gt.cameras[i].params.sensor,
                SensorParams::Scheimpflug { .. }
            )
        })
    }

    /// The truth's plane of camera `i`'s own laser (the only laser, if none names it).
    fn plane(&self, i: usize) -> Option<&GtPlane> {
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

fn handeye_mode(eye_in_hand: bool) -> Value {
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

fn merge(into: &mut Value, patch: &Value) {
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
fn nominal_seed(cameras: &[&GtCamera]) -> Value {
    let mut intrinsics = Vec::new();
    let mut sensors = Vec::new();
    for c in cameras {
        let p = serde_json::to_value(&c.params).unwrap_or_default();
        let k = &p["intrinsics"];
        let f = ((k["fx"].as_f64().unwrap_or(0.0) + k["fy"].as_f64().unwrap_or(0.0)) / 2.0 / 100.0)
            .round_ties_even()
            * 100.0;
        let [w, h] = c.resolution.map(f64::from);
        intrinsics.push(json!({"fx": f, "fy": f, "cx": w / 2.0, "cy": h / 2.0, "skew": 0.0}));
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

fn camera_of(v: &Value, id: &str, cam_se3_ref: Isometry3<f64>, tilt: [f64; 2]) -> EstimatedCamera {
    let n = |x: &Value| x.as_f64().unwrap_or(f64::NAN);
    let k = &v["k"];
    let d = &v["dist"];
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
    for r in export["per_feature_residuals"]["target"]
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
        errors_px: errors,
    }
}

// ── Workflows ─────────────────────────────────────────────────────────────────

/// Calibrate `obs` of `gt`'s dataset and compare with the truth. `gated`: the observations are
/// the exact analytic ones, so G5.1 (G5.3) applies.
///
/// # Errors
///
/// When the dataset has no robot, the observations do not match the truth's captures, or a
/// calibration-rs workflow fails.
pub fn calibrate(gt: &GroundTruth, obs: &Observations, options: &Options) -> Result<Report> {
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
    let truth = Truth::new(gt);
    let eye_in_hand = truth.eye_in_hand()?;
    let poses: Vec<Isometry3<f64>> = obs
        .captures
        .iter()
        .map(|c| c.base_se3_tcp.context("a capture has no robot pose"))
        .collect::<Result<_>>()?;
    let gated = obs.noise == ObservationNoise::default() && !options.detected;
    let ids: Vec<&str> = gt.cameras.iter().map(|c| c.id.as_str()).collect();
    if let Some(name) = &options.camera {
        ensure!(
            gt.lasers.is_empty(),
            "--camera does not apply to a laser dataset"
        );
        let i = ids
            .iter()
            .position(|c| c == name)
            .with_context(|| format!("no camera `{name}` (cameras: {ids:?})"))?;
        return single(&truth, obs, &poses, i, eye_in_hand, gated);
    }
    match (gt.cameras.len(), gt.lasers.is_empty()) {
        (1, true) => single(&truth, obs, &poses, 0, eye_in_hand, gated),
        (_, true) => rig(&truth, obs, &poses, eye_in_hand, gated),
        (1, false) => head(&truth, obs, &poses, eye_in_hand, gated),
        (_, false) => rig_laser(&truth, obs, &poses, eye_in_hand, gated),
    }
}

fn run_single(
    obs: &Observations,
    poses: &[Isometry3<f64>],
    i: usize,
    eye_in_hand: bool,
) -> Result<(Value, usize, usize)> {
    let mut views = Vec::new();
    let mut points = 0;
    for (c, pose) in obs.captures.iter().zip(poses) {
        if let Some(v) = correspondences(&c.views[i]) {
            points += v.len();
            views.push(View::new(
                v,
                single_cam_handeye::HandeyeMeta {
                    base_se3_gripper: *pose,
                },
            ));
        }
    }
    let n = views.len();
    let mut session = CalibrationSession::<single_cam_handeye::SingleCamHandeyeProblem>::new();
    session.set_input(single_cam_handeye::SingleCamHandeyeInput::new(views)?)?;
    session.set_config(config(
        &json!({"handeye_init": {"handeye_mode": handeye_mode(eye_in_hand)}}),
    )?)?;
    single_cam_handeye::run_calibration(&mut session)?;
    Ok((serde_json::to_value(session.export()?)?, n, points))
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

fn offsets(
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
        residuals,
        mut estimate,
        target,
    } = solved;
    estimate.offsets = offsets(truth, &cameras, &estimate, target);
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
    }
}

fn single(
    truth: &Truth,
    obs: &Observations,
    poses: &[Isometry3<f64>],
    i: usize,
    eye_in_hand: bool,
    gated: bool,
) -> Result<Report> {
    let (export, views, points) = run_single(obs, poses, i, eye_in_hand)?;
    let camera = camera_of(
        &export["camera"],
        &truth.gt.cameras[i].id,
        Isometry3::identity(),
        [0.0, 0.0],
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
        },
    ))
}

/// The rig rows shared by the rig workflows: per camera intrinsics, `cam_se3_rig`, `extra`; then
/// hand-eye and target.
fn rig_estimate(truth: &Truth, export: &Value, tilts: &[[f64; 2]]) -> Result<Vec<EstimatedCamera>> {
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
                tilts.get(i).copied().unwrap_or([0.0, 0.0]),
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

fn rig(
    truth: &Truth,
    obs: &Observations,
    poses: &[Isometry3<f64>],
    eye_in_hand: bool,
    gated: bool,
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
                meta: rig_handeye_laserline::RobotPoseMeta {
                    base_se3_gripper: *pose,
                },
            });
        }
    }
    let count = views.len();
    let mut session = CalibrationSession::<rig_handeye::RigHandeyeProblem>::new();
    session.set_input(RigDataset::new(views, n)?)?;
    session.set_config(config(
        &json!({"handeye_init": {"handeye_mode": handeye_mode(eye_in_hand)}}),
    )?)?;
    rig_handeye::run_calibration(&mut session)?;
    let export = serde_json::to_value(session.export()?)?;
    let cameras = rig_estimate(truth, &export, &[])?;
    let (rows, (he, target)) = rig_rows(truth, &export, &cameras, eye_in_hand, |_| vec![])?;
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
            cameras: all.clone(),
            rows,
            counts: (count, points, 0),
            residuals: residuals(&export, n),
            estimate,
            target,
        },
    ))
}

fn rig_laser(
    truth: &Truth,
    obs: &Observations,
    poses: &[Isometry3<f64>],
    eye_in_hand: bool,
    gated: bool,
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
    let scheimpflug = truth.scheimpflug(&all);
    let input: rig_handeye_laserline::RigHandeyeLaserlineInput =
        serde_json::from_value(json!({"views": views, "num_cameras": n}))
            .context("laserline input")?;
    let mut patch =
        json!({"handeye": {"handeye_init": {"handeye_mode": handeye_mode(eye_in_hand)}}});
    if scheimpflug {
        let cams: Vec<&GtCamera> = truth.gt.cameras.iter().collect();
        merge(
            &mut patch,
            &json!({"handeye": {
                "manual_init": nominal_seed(&cams),
                "sensor": scheimpflug_mode(),
                "rig": {"refine_intrinsics_in_rig_ba": true},
            }}),
        );
    }
    let mut session =
        CalibrationSession::<rig_handeye_laserline::RigHandeyeLaserlineProblem>::new();
    session.set_input(input)?;
    session.set_config(config(&patch)?)?;
    rig_handeye_laserline::run_calibration(&mut session)?;
    let export = serde_json::to_value(session.export()?)?;
    let tilts: Vec<[f64; 2]> = export["sensors"]
        .as_array()
        .map(|s| {
            s.iter()
                .map(|t| {
                    [
                        t["tilt_x"].as_f64().unwrap_or(0.0),
                        t["tilt_y"].as_f64().unwrap_or(0.0),
                    ]
                })
                .collect()
        })
        .unwrap_or_default();
    let cameras = rig_estimate(truth, &export, &tilts)?;
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
        },
    ))
}

fn scheimpflug_mode() -> Value {
    json!({"kind": "Scheimpflug", "refine_scheimpflug_in_rig_ba": true})
}

/// One camera carrying its laser: the laserline device workflow for intrinsics, tilt and plane,
/// then the single-camera hand-eye workflow on the same views for the hand-eye.
fn head(
    truth: &Truth,
    obs: &Observations,
    poses: &[Isometry3<f64>],
    eye_in_hand: bool,
    gated: bool,
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
    let tilt = match truth.gt.cameras[0].params.sensor {
        SensorParams::Scheimpflug { params } => {
            params.tilt_x.to_degrees().round_ties_even().to_radians()
        }
        _ => 0.0,
    };
    let mut session = CalibrationSession::<laserline_device::LaserlineDeviceProblem>::new();
    session.set_input(input)?;
    laserline_device::run_calibration(
        &mut session,
        Some(config(&json!({
            "sensor_init": {"tilt_x": tilt, "tilt_y": 0.0},
            // The workflow's default fixes the first target pose at its linear estimate; a
            // single camera has no gauge to fix, and that pose is not exact.
            "optimize": {"fix_poses": []},
            "solver": {"max_iters": 200},
        }))?),
    )?;
    let device = serde_json::to_value(session.export()?)?;
    let p = &device["estimate"]["params"];
    let mut camera = camera_of(
        &json!({"k": p["intrinsics"], "dist": p["distortion"]}),
        &truth.gt.cameras[0].id,
        Isometry3::identity(),
        [
            p["sensor"]["tilt_x"].as_f64().unwrap_or(0.0),
            p["sensor"]["tilt_y"].as_f64().unwrap_or(0.0),
        ],
    );
    let plane = plane_of(&p["plane"]);
    let mut rows = vec![Row::new(
        "mean reprojection px",
        device["mean_reproj_error"].as_f64().unwrap_or(f64::NAN),
        Some(gate::MEAN_REPROJ_PX),
    )];
    rows.extend(intrinsics_rows(&camera, &truth.gt.cameras[0])?);
    if truth.scheimpflug(&[0]) {
        rows.extend(tilt_rows(&camera, &truth.gt.cameras[0]));
    }
    if let (Some(e), Some(t)) = (&plane, truth.plane(0)) {
        rows.extend(plane_rows(e, t, &truth.gt.cameras[0].id));
    }
    // The hand-eye, from the same target views (a pinhole workflow: on a Scheimpflug camera its
    // intrinsics absorb the tilt, so only its hand-eye is kept).
    let (he_export, _, _) = run_single(obs, poses, 0, eye_in_hand)?;
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
#[derive(Clone, Debug, Default, Deserialize)]
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
}

/// What [`run_dir`] found: every trial's report and, with several, each row's spread.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Run {
    pub reports: Vec<Report>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_stats: Option<Vec<RowStats>>,
}

/// The file [`run_dir`] writes into the dataset directory.
pub const CALIBRATION: &str = "calibration.json";

/// Calibrate the dataset in `dir` (`gt.json`, and `etendue detect`'s files when `detected`),
/// reporting each trial through `ctl`, and write the run to `dir/calibration.json`.
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
        let report = calibrate(&gt, &obs, &calibrate_options)?;
        ctl.log(format!(
            "trial {}: {:?}, mean reprojection {:.4} px, {}",
            k + 1,
            report.workflow,
            report.residuals.mean_px,
            if !report.gated {
                "not gated"
            } else if report.pass {
                "PASS"
            } else {
                "FAIL"
            }
        ));
        reports.push(report);
    }
    ctl.step(Stage::Calibrate, trials as usize, trials as usize);
    let run = Run {
        row_stats: (reports.len() > 1).then(|| row_stats(&reports)),
        reports,
    };
    let path = dir.join(CALIBRATION);
    std::fs::write(&path, serde_json::to_string_pretty(&run)? + "\n")
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(run)
}
