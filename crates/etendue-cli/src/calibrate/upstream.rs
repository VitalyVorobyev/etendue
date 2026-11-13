//! The boundary with calibration-rs: what etendue sends (configs, seeds, inputs, checked by
//! [`strict`]) and what it reads back (exports, through calibration-rs's own types).

use anyhow::{Context, Result, ensure};
use nalgebra::Isometry3;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use vision_calibration_core::{
    BrownConrady5, CameraParams, DistortionParams, FxFyCxCy, LaserPlane, NoiseLevel, ResidualKind,
    ScheimpflugParams, SensorParams, Uncertainty,
};
use vision_calibration_pipeline::common::{ExportDiagnostics, HandeyeSolution};
use vision_calibration_pipeline::single_cam_handeye::SingleCamHandeyeSolution;

use super::report::{EstimatedCamera, FeatureResidual, Plane, Predicted, Residuals};
use super::truth::{Truth, distortion_of, tilt_of};

pub(crate) fn handeye_mode(eye_in_hand: bool) -> Value {
    json!(if eye_in_hand {
        "EyeInHand"
    } else {
        "EyeToHand"
    })
}

/// A config: `T::default()` with JSON `patch` merged in (the configs are `non_exhaustive`). A
/// key of the patch that the config does not have is an error, not ignored.
pub(super) fn config<T: Serialize + DeserializeOwned + Default>(patch: &Value) -> Result<T> {
    let mut v = serde_json::to_value(T::default())?;
    merge(&mut v, patch);
    strict(v, "calibration config")
}

/// Whether calibration-rs reads all of `patch` as `problem`'s config: what a stage's options
/// become ([`config`] with the workflow's type).
///
/// # Errors
///
/// A key the workflow's config does not read, or a value of the wrong type.
pub(crate) fn check_config(problem: etendue_scene::Problem, patch: &Value) -> Result<()> {
    use etendue_scene::Problem as P;
    use vision_calibration_pipeline::{
        laserline_device as ld, planar_intrinsics as pi, rig_extrinsics as re, rig_handeye as rh,
        rig_handeye_laserline as rl, single_cam_handeye as sh,
    };
    match problem {
        P::PlanarIntrinsics => config::<pi::PlanarIntrinsicsConfig>(patch).map(drop),
        P::RigExtrinsics => config::<re::RigExtrinsicsConfig>(patch).map(drop),
        P::SingleCamHandeye => config::<sh::SingleCamHandeyeConfig>(patch).map(drop),
        P::RigHandeye => config::<rh::RigHandeyeConfig>(patch).map(drop),
        P::LaserlineDevice => config::<ld::LaserlineDeviceConfig>(patch).map(drop),
        P::RigHandeyeLaserline => config::<rl::RigHandeyeLaserlineConfig>(patch).map(drop),
    }
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

/// An estimated camera from an export's camera.
pub(crate) fn camera_of(
    p: &CameraParams,
    id: &str,
    cam_se3_ref: Isometry3<f64>,
) -> Result<EstimatedCamera> {
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
pub(super) fn params_of(cam: &EstimatedCamera, like: &CameraParams) -> CameraParams {
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

pub(super) fn plane_of(p: &LaserPlane) -> Plane {
    Plane {
        normal: p.normal().into_inner().into(),
        distance: p.distance(),
    }
}

/// The residual figures of an export's diagnostics, for `cameras` cameras; `mean_px` is the
/// export's own mean reprojection error.
pub(crate) fn residuals(
    diagnostics: &ExportDiagnostics,
    mean_px: f64,
    cameras: usize,
) -> Residuals {
    let mut errors = Vec::new();
    let mut sq = 0.0;
    let mut per_camera = vec![(0.0, 0usize); cameras];
    let mut features = Vec::new();
    for r in &diagnostics.per_feature_residuals.target {
        // A point whose projection failed has no residual. The error is calibration-rs's own.
        let (o, Some(p), Some(e)) = (r.observed_px, r.projected_px, r.error_px) else {
            continue;
        };
        let (du, dv) = (p[0] - o[0], p[1] - o[1]);
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

/// An export as JSON (what a run keeps), with calibration-rs's quality verdict for cameras of
/// `sizes`.
pub(crate) fn exported<E>(
    export: &E,
    sizes: &[[u32; 2]],
) -> Result<(
    Value,
    Option<vision_calibration_pipeline::analysis::QualityReport>,
)>
where
    E: Serialize + vision_calibration_pipeline::common::CalibratedCameras,
{
    let quality = vision_calibration_pipeline::analysis::assess(
        export,
        sizes,
        &vision_calibration_pipeline::analysis::QualityThresholds::default(),
    )
    .ok();
    Ok((serde_json::to_value(export)?, quality))
}

/// The image sizes of dataset cameras `cams`.
pub(super) fn sizes(truth: &Truth, cams: &[usize]) -> Vec<[u32; 2]> {
    cams.iter()
        .map(|&i| truth.gt.cameras[i].resolution)
        .collect()
}

/// calibration-rs's covariance, as σ; `None` when it has none.
pub(crate) fn predicted(uncertainty: &Uncertainty) -> Option<Predicted> {
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
    /// The laser plane of each camera in its frame, where known.
    pub planes: Option<Vec<Option<Plane>>>,
    /// A `rig_handeye` stage's export (JSON), which the joint laser solve starts from.
    pub rig_handeye: Option<Value>,
}

/// A plane as calibration-rs's `LaserPlane`: its normal of unit length.
fn plane_json(p: &Plane) -> Value {
    let [x, y, z] = p.normal;
    let s = (x * x + y * y + z * z).sqrt();
    json!({"normal": [x / s, y / s, z / s], "distance": p.distance / s})
}

impl SeedValues {
    /// The joint laser solve's seed (`JointManualInit`), if the stage has one: a rig hand-eye
    /// result to start from, and the planes of `cameras` (each must have one).
    ///
    /// # Errors
    ///
    /// When planes are seeded but a camera has none.
    pub(crate) fn joint_init(&self, cameras: &[usize]) -> Result<Option<Value>> {
        if self.rig_handeye.is_none() && self.planes.is_none() {
            return Ok(None);
        }
        let planes = match &self.planes {
            Some(p) => Some(
                cameras
                    .iter()
                    .map(|&i| {
                        p[i].as_ref()
                            .map(plane_json)
                            .with_context(|| format!("camera {i} has no laser plane to seed"))
                    })
                    .collect::<Result<Vec<_>>>()?,
            ),
            None => None,
        };
        Ok(Some(
            json!({"rig_handeye": self.rig_handeye, "planes_cam": planes}),
        ))
    }

    pub(super) fn none(&self) -> bool {
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
    pub(super) fn camera_init(&self, i: usize) -> Value {
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
        if let Some(p) = self.planes.as_ref().and_then(|p| p[i].as_ref()) {
            // The laserline device's plane (`DeviceManualInit::plane`).
            seed["plane"] = plane_json(p);
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

/// A single camera's hand-eye and target pose, in the frames of `truth`'s mode.
pub(super) fn single_handeye(
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
pub(super) fn rig_handeye(
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
