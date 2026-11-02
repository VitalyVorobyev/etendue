//! Calibration-pose planning (P18): from the sphere generator's feasible candidates
//! ([`crate::poses`]), the poses that minimise the predicted **system σ**.
//!
//! The system σ is how far, in millimetres across the line of sight, the calibrated cell would
//! place a point it knows in the robot's base frame, averaged over the target's points as every
//! feasible candidate sees them. It is the predicted covariance of the calibration propagated to
//! that position:
//! - **The calibration** estimates, per camera, the focal lengths, the principal point, the
//!   Brown-Conrady `k1, k2, p1, p2` and a Scheimpflug tilt; each camera's pose in the rig but the
//!   first's; the hand-eye; and the target's pose in the base. That is calibration-rs's rig
//!   hand-eye with the rig refined (`docs/measurements/g17_sim_lab.md`). The robot poses are taken
//!   as exact.
//! - **The information** of a view is `Jᵀ J / σ²` over its visible points, `J` the derivative of
//!   each pixel with respect to those parameters. `J` is numerical, by central differences of
//!   calibration-rs's own projection (no camera model here), the poses perturbed on the right
//!   (`T · Exp(δ)`, as calibration-rs's covariance is expressed).
//! - **The system σ²** of a set of views is `tr(Σ A) / n`: `Σ` the inverse of their information
//!   (with a negligible prior), `A` the sum, over the `n` (point, camera, candidate) sightings, of
//!   `w · jᵀ j` with `j` a pixel row's derivative with respect to everything but the target, and
//!   `w = (depth / f)²` turning pixels into millimetres.
//!
//! The planner adds, one at a time, the candidate that lowers the system σ most (greedy), up to
//! the budget, and keeps the generator's tour order for the motion. Deterministic.

use etendue_kinematics::RobotModel;
use etendue_scene::{ScenarioSpec, SceneSpec, SphereGenerator, Step};
use nalgebra::{DMatrix, Isometry3, Point3, Quaternion, Translation3, UnitQuaternion, Vector3};
use serde::{Deserialize, Serialize};
use vision_calibration_core::{CameraModel, CameraParams, DistortionParams, SensorParams};

use crate::board::layout;
use crate::gt::{VisibilitySpec, project_points};
use crate::poses::{CandidateStatus, Generated, Posed};
use crate::{Error, PixelCentre, Result};

fn bad(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

/// How to plan.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanSpec {
    /// How many poses to keep.
    pub budget: usize,
    /// The pixel noise σ the prediction assumes (it scales every σ alike).
    #[serde(default = "PlanSpec::default_noise")]
    pub noise_px: f64,
}

impl PlanSpec {
    fn default_noise() -> f64 {
        0.5
    }
}

/// What the planner chose, and how it compares.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Planned {
    /// The chosen candidates' capture ids, in the order the planner picked them.
    pub chosen: Vec<String>,
    /// The system σ (mm) after each pick: `sigma_mm[k]` with `k + 1` poses.
    pub sigma_mm: Vec<f64>,
    /// The system σ (mm) of the generator's grid thinned evenly to the budget.
    pub grid_sigma_mm: f64,
    /// The thinned grid's capture ids.
    pub grid: Vec<String>,
    /// The system σ (mm) of every feasible candidate.
    pub all_sigma_mm: f64,
    /// Feasible candidates.
    pub feasible: usize,
    /// The predicted σ of each parameter with the chosen poses (labels as calibration-rs names
    /// them: `cam0.fx`, `cam1.cam_se3_rig.tx`, `gripper_se3_rig.rx`, `base_se3_target.tz`, …;
    /// px, radians, metres).
    pub labels: Vec<String>,
    pub sigma: Vec<f64>,
    /// The chosen poses as a stop-and-shoot scenario, in the generator's tour order.
    pub scenario: ScenarioSpec,
}

/// A rotation by `phi` (its axis times its angle), from `libm` sines.
fn exp_so3(phi: Vector3<f64>) -> UnitQuaternion<f64> {
    let angle = phi.norm();
    if angle < 1e-300 {
        return UnitQuaternion::identity();
    }
    let s = libm::sin(angle / 2.0) / angle;
    UnitQuaternion::new_unchecked(Quaternion::new(
        libm::cos(angle / 2.0),
        phi.x * s,
        phi.y * s,
        phi.z * s,
    ))
}

/// `t · Exp(δ)`, `δ = [rx, ry, rz, tx, ty, tz]`: the rotation `R Exp(φ)`, the translation
/// `t + R ρ`.
fn perturbed(t: &Isometry3<f64>, k: usize, h: f64) -> Isometry3<f64> {
    let mut d = [0.0; 6];
    d[k] = h;
    let delta = Isometry3::from_parts(
        Translation3::new(d[3], d[4], d[5]),
        exp_so3(Vector3::new(d[0], d[1], d[2])),
    );
    t * delta
}

/// One camera's parameters, with each perturbed model.
struct Cam {
    frame: usize,
    resolution: [u32; 2],
    model: CameraModel,
    fx: f64,
    /// Per parameter: its label suffix and the models at `±h`.
    params: Vec<(&'static str, f64, CameraModel, CameraModel)>,
}

fn camera_params(
    params: &CameraParams,
) -> Result<Vec<(&'static str, f64, CameraModel, CameraModel)>> {
    let mut out = Vec::new();
    let mut push =
        |name: &'static str, h: f64, set: &dyn Fn(&mut CameraParams, f64)| -> Result<()> {
            let (mut plus, mut minus) = (params.clone(), params.clone());
            set(&mut plus, h);
            set(&mut minus, -h);
            out.push((name, h, plus.build()?, minus.build()?));
            Ok(())
        };
    push("fx", 1e-2, &|p, h| p.intrinsics.fx += h)?;
    push("fy", 1e-2, &|p, h| p.intrinsics.fy += h)?;
    push("cx", 1e-2, &|p, h| p.intrinsics.cx += h)?;
    push("cy", 1e-2, &|p, h| p.intrinsics.cy += h)?;
    if matches!(params.distortion, DistortionParams::BrownConrady5 { .. }) {
        for (name, h) in [("k1", 1e-6), ("k2", 1e-6), ("p1", 1e-7), ("p2", 1e-7)] {
            push(name, h, &move |p, h| {
                if let DistortionParams::BrownConrady5 { params: d } = &mut p.distortion {
                    match name {
                        "k1" => d.k1 += h,
                        "k2" => d.k2 += h,
                        "p1" => d.p1 += h,
                        _ => d.p2 += h,
                    }
                }
            })?;
        }
    }
    if matches!(params.sensor, SensorParams::Scheimpflug { .. }) {
        for name in ["tilt_x", "tilt_y"] {
            push(name, 1e-7, &move |p, h| {
                if let SensorParams::Scheimpflug { params: t } = &mut p.sensor {
                    if name == "tilt_x" {
                        t.tilt_x += h;
                    } else {
                        t.tilt_y += h;
                    }
                }
            })?;
        }
    }
    Ok(out)
}

/// The pixel of `p` (target frame) in camera `c` given the chain, or `None` behind it.
fn pixel(
    model: &CameraModel,
    cam_se3_target: &Isometry3<f64>,
    p: &Point3<f64>,
) -> Option<[f64; 2]> {
    let q = cam_se3_target * p;
    model.project_point_c(&q.coords).map(|u| [u.x, u.y])
}

const POSE: [&str; 6] = ["rx", "ry", "rz", "tx", "ty", "tz"];
const H_POSE: f64 = 1e-7;

/// Plan `spec.budget` poses from `generated` (made by [`crate::poses::generate`] with `sphere`
/// for `scene`).
///
/// # Errors
///
/// [`Error::InvalidInput`] for an unknown robot, target or camera, a target without a printed
/// layout, a budget of zero or more than the feasible candidates, or a calibration that the
/// candidates cannot determine.
pub fn plan(
    scene: &SceneSpec,
    robots: &[RobotModel],
    generated: &Generated,
    sphere: &SphereGenerator,
    spec: &PlanSpec,
) -> Result<Planned> {
    if !(spec.noise_px.is_finite() && spec.noise_px > 0.0) {
        return Err(bad("noise_px must be positive"));
    }
    let posed = Posed::new(scene, robots)?;
    let r = scene
        .robots
        .iter()
        .position(|x| x.id == sphere.robot)
        .ok_or_else(|| bad(format!("no robot `{}`", sphere.robot)))?;
    let model = robots
        .get(r)
        .ok_or_else(|| bad("a robot model per scene robot"))?;
    let names = model.links();
    let base_frame = posed.index(&format!("{}/{}", sphere.robot, names[model.base_link()]))?;
    let tcp_frame = posed.index(&format!("{}/{}", sphere.robot, names[model.tcp_link()]))?;
    let target = scene
        .targets
        .iter()
        .find(|t| t.id == sphere.target)
        .ok_or_else(|| bad(format!("no target `{}`", sphere.target)))?;
    let target_frame = posed.index(&target.id)?;
    let board = layout(&target.geometry)?
        .ok_or_else(|| bad(format!("target `{}` has no printed layout", target.id)))?;
    let points: Vec<Point3<f64>> = board.points.iter().map(|b| b.xyz()).collect();
    let ids: Vec<String> = match &sphere.require.cameras {
        Some(c) => c.clone(),
        None => scene.cameras.iter().map(|c| c.id.clone()).collect(),
    };
    let cams = ids
        .iter()
        .map(|id| {
            let c = scene
                .cameras
                .iter()
                .find(|c| &c.id == id)
                .ok_or_else(|| bad(format!("no camera `{id}`")))?;
            Ok(Cam {
                frame: posed.index(id)?,
                resolution: c.resolution,
                model: c.params.build()?,
                fx: c.params.intrinsics.fx,
                params: camera_params(&c.params)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    // The parameter vector: cameras, then the rig (every camera but the first), the hand-eye
    // (its frame: the first camera's), the target.
    let mut labels = Vec::new();
    let mut cam_offset = Vec::new();
    for (c, cam) in cams.iter().enumerate() {
        cam_offset.push(labels.len());
        labels.extend(cam.params.iter().map(|(n, ..)| format!("cam{c}.{n}")));
    }
    let rig_offset = labels.len();
    for c in 1..cams.len() {
        labels.extend(POSE.iter().map(|k| format!("cam{c}.cam_se3_rig.{k}")));
    }
    let handeye_offset = labels.len();
    labels.extend(POSE.iter().map(|k| format!("gripper_se3_rig.{k}")));
    let target_offset = labels.len();
    labels.extend(POSE.iter().map(|k| format!("base_se3_target.{k}")));
    let n = labels.len();

    // Each feasible candidate's information, and the system σ's weights.
    let feasible: Vec<usize> = (0..generated.candidates.len())
        .filter(|&i| {
            let c = &generated.candidates[i];
            c.status == CandidateStatus::Ok && c.capture.is_some() && c.q.is_some()
        })
        .collect();
    if spec.budget == 0 || spec.budget > feasible.len() {
        return Err(bad(format!(
            "budget {} for {} feasible candidates",
            spec.budget,
            feasible.len()
        )));
    }
    let visibility = VisibilitySpec {
        margin_px: sphere.require.margin_px,
    };
    let edge = PixelCentre::Integer.edge();
    let var = spec.noise_px * spec.noise_px;
    let mut infos: Vec<DMatrix<f64>> = Vec::with_capacity(feasible.len());
    let mut a = DMatrix::<f64>::zeros(n, n);
    let mut sightings = 0usize;
    for &i in &feasible {
        let candidate = &generated.candidates[i];
        let f = posed.at(r, candidate.q.as_deref().expect("feasible"));
        let world_se3_ref = f[cams[0].frame];
        let handeye = f[tcp_frame].inverse() * world_se3_ref;
        let base_se3_tcp = f[base_frame].inverse() * f[tcp_frame];
        let base_se3_target = f[base_frame].inverse() * f[target_frame];
        let mut info = DMatrix::<f64>::zeros(n, n);
        for (c, cam) in cams.iter().enumerate() {
            let cam_se3_ref = f[cam.frame].inverse() * world_se3_ref;
            let chain = |cam_se3_ref: &Isometry3<f64>, he: &Isometry3<f64>, t: &Isometry3<f64>| {
                cam_se3_ref * he.inverse() * base_se3_tcp.inverse() * t
            };
            let nominal = chain(&cam_se3_ref, &handeye, &base_se3_target);
            let seen = project_points(
                &cam.model,
                cam.resolution,
                &f[cam.frame],
                &f[target_frame],
                &board.points,
                &visibility,
                edge,
            );
            for (gt, p) in seen.iter().zip(&points) {
                if !gt.visible() {
                    continue;
                }
                let mut j = vec![[0.0f64; 2]; n];
                let diff =
                    |plus: Option<[f64; 2]>, minus: Option<[f64; 2]>, h: f64| match (plus, minus) {
                        (Some(a), Some(b)) => {
                            [(a[0] - b[0]) / (2.0 * h), (a[1] - b[1]) / (2.0 * h)]
                        }
                        _ => [0.0, 0.0],
                    };
                for (k, (_, h, plus, minus)) in cam.params.iter().enumerate() {
                    j[cam_offset[c] + k] =
                        diff(pixel(plus, &nominal, p), pixel(minus, &nominal, p), *h);
                }
                for k in 0..6 {
                    if c > 0 {
                        let at = |s: f64| {
                            pixel(
                                &cam.model,
                                &chain(&perturbed(&cam_se3_ref, k, s), &handeye, &base_se3_target),
                                p,
                            )
                        };
                        j[rig_offset + 6 * (c - 1) + k] = diff(at(H_POSE), at(-H_POSE), H_POSE);
                    }
                    let at = |s: f64| {
                        pixel(
                            &cam.model,
                            &chain(&cam_se3_ref, &perturbed(&handeye, k, s), &base_se3_target),
                            p,
                        )
                    };
                    j[handeye_offset + k] = diff(at(H_POSE), at(-H_POSE), H_POSE);
                    let at = |s: f64| {
                        pixel(
                            &cam.model,
                            &chain(&cam_se3_ref, &handeye, &perturbed(&base_se3_target, k, s)),
                            p,
                        )
                    };
                    j[target_offset + k] = diff(at(H_POSE), at(-H_POSE), H_POSE);
                }
                let depth = (nominal * p).z;
                let w = (depth / cam.fx) * (depth / cam.fx) * 1e6;
                for axis in 0..2 {
                    let row: Vec<f64> = j.iter().map(|d| d[axis]).collect();
                    for (x, &jx) in row.iter().enumerate() {
                        if jx == 0.0 {
                            continue;
                        }
                        for (y, &jy) in row.iter().enumerate() {
                            info[(x, y)] += jx * jy / var;
                            if x < target_offset && y < target_offset {
                                a[(x, y)] += w * jx * jy;
                            }
                        }
                    }
                }
                sightings += 2;
            }
        }
        infos.push(info);
    }
    if sightings == 0 {
        return Err(bad("no candidate sees the target"));
    }

    // A negligible prior (a millionth of an average view), so any set of views inverts.
    let mut prior = DMatrix::<f64>::zeros(n, n);
    for x in 0..n {
        let mean: f64 = infos.iter().map(|m| m[(x, x)]).sum::<f64>() / infos.len() as f64;
        prior[(x, x)] = 1e-6 * mean.max(f64::MIN_POSITIVE);
    }
    // Per image axis: `a` sums both axes of every sighting.
    let rows = sightings as f64;
    let sigma_of = |info: &DMatrix<f64>| -> Option<(f64, DMatrix<f64>)> {
        let cov = info.clone().cholesky()?.inverse();
        let mut t = 0.0;
        for x in 0..target_offset {
            for y in 0..target_offset {
                t += cov[(x, y)] * a[(x, y)];
            }
        }
        Some(((t / rows).max(0.0).sqrt(), cov))
    };
    let total = |set: &[usize]| -> DMatrix<f64> {
        let mut m = prior.clone();
        for &k in set {
            m += &infos[k];
        }
        m
    };

    // Greedy: the candidate that lowers the system σ most, one at a time.
    let mut chosen: Vec<usize> = Vec::new();
    let mut sigma_mm = Vec::new();
    let mut info = prior.clone();
    for _ in 0..spec.budget {
        let mut best: Option<(usize, f64)> = None;
        for (k, candidate) in infos.iter().enumerate() {
            if chosen.contains(&k) {
                continue;
            }
            let trial = &info + candidate;
            if let Some((s, _)) = sigma_of(&trial)
                && best.is_none_or(|(_, b)| s < b)
            {
                best = Some((k, s));
            }
        }
        let (k, s) = best.ok_or_else(|| bad("the candidates cannot determine the calibration"))?;
        info += &infos[k];
        chosen.push(k);
        sigma_mm.push(s);
    }
    let (_, cov) =
        sigma_of(&info).ok_or_else(|| bad("the chosen poses cannot determine the calibration"))?;
    let sigma: Vec<f64> = (0..n).map(|x| cov[(x, x)].max(0.0).sqrt()).collect();

    // The generator's grid thinned evenly to the budget, in its tour order.
    let tour: Vec<usize> = generated
        .scenario
        .steps
        .iter()
        .filter_map(|s| match s {
            Step::Capture { id: Some(id) } => feasible
                .iter()
                .position(|&i| generated.candidates[i].capture.as_deref() == Some(id)),
            _ => None,
        })
        .collect();
    let m = tour.len();
    let grid: Vec<usize> = (0..spec.budget)
        .map(|k| tour[k * m / spec.budget])
        .collect();
    let grid_sigma = sigma_of(&total(&grid)).map_or(f64::NAN, |s| s.0);
    let all: Vec<usize> = (0..infos.len()).collect();
    let all_sigma = sigma_of(&total(&all)).map_or(f64::NAN, |s| s.0);

    let capture = |k: usize| {
        generated.candidates[feasible[k]]
            .capture
            .clone()
            .expect("feasible")
    };
    let keep: Vec<String> = chosen.iter().map(|&k| capture(k)).collect();
    let scenario = subset(
        generated,
        &keep,
        format!(
            "{} planned poses of `{}` around `{}` (system σ {:.4} mm; the grid's {:.4} mm)",
            chosen.len(),
            sphere.look,
            sphere.target,
            sigma_mm.last().copied().unwrap_or(f64::NAN),
            grid_sigma
        ),
    );
    Ok(Planned {
        chosen: keep,
        sigma_mm,
        grid_sigma_mm: grid_sigma,
        grid: grid.iter().map(|&k| capture(k)).collect(),
        all_sigma_mm: all_sigma,
        feasible: feasible.len(),
        labels,
        sigma,
        scenario,
    })
}

/// The chosen (or any) captures of `generated` as a stop-and-shoot scenario, in its tour order.
#[must_use]
pub fn subset(generated: &Generated, keep: &[String], description: String) -> ScenarioSpec {
    let mut steps = Vec::new();
    let mut pending: Option<Step> = None;
    for s in &generated.scenario.steps {
        match s {
            Step::PtpPose { .. } => pending = Some(s.clone()),
            Step::Capture { id: Some(id) } => {
                if keep.contains(id) {
                    steps.extend(pending.take());
                    steps.push(s.clone());
                } else {
                    pending = None;
                }
            }
            other => {
                steps.extend(pending.take());
                steps.push(other.clone());
            }
        }
    }
    ScenarioSpec {
        steps,
        description: Some(description),
        ..generated.scenario.clone()
    }
}

/// An estimated cell, as a calibration reports it: each camera's parameters and pose in the rig
/// (`cam_se3_ref`, the first camera's frame), and the hand-eye `gripper_se3_ref`.
#[derive(Clone, Debug)]
pub struct EstimatedCell {
    /// Per camera (in `sphere.require.cameras` order, or the scene's): its parameters and
    /// `cam_se3_ref`.
    pub cameras: Vec<(CameraParams, Isometry3<f64>)>,
    /// `gripper_se3_ref`.
    pub handeye: Isometry3<f64>,
}

/// The system error of `estimate`: the RMS, over every sighting of the target's points by every
/// feasible candidate, of how far (mm, across the line of sight) the estimated cell places a
/// point known in the base frame — the quantity [`plan`] predicts the σ of.
///
/// # Errors
///
/// As [`plan`], and for an estimate with another number of cameras.
pub fn system_error(
    scene: &SceneSpec,
    robots: &[RobotModel],
    generated: &Generated,
    sphere: &SphereGenerator,
    estimate: &EstimatedCell,
) -> Result<f64> {
    let posed = Posed::new(scene, robots)?;
    let r = scene
        .robots
        .iter()
        .position(|x| x.id == sphere.robot)
        .ok_or_else(|| bad(format!("no robot `{}`", sphere.robot)))?;
    let model = robots
        .get(r)
        .ok_or_else(|| bad("a robot model per scene robot"))?;
    let names = model.links();
    let base_frame = posed.index(&format!("{}/{}", sphere.robot, names[model.base_link()]))?;
    let tcp_frame = posed.index(&format!("{}/{}", sphere.robot, names[model.tcp_link()]))?;
    let target = scene
        .targets
        .iter()
        .find(|t| t.id == sphere.target)
        .ok_or_else(|| bad(format!("no target `{}`", sphere.target)))?;
    let target_frame = posed.index(&target.id)?;
    let board = layout(&target.geometry)?
        .ok_or_else(|| bad(format!("target `{}` has no printed layout", target.id)))?;
    let ids: Vec<String> = match &sphere.require.cameras {
        Some(c) => c.clone(),
        None => scene.cameras.iter().map(|c| c.id.clone()).collect(),
    };
    if ids.len() != estimate.cameras.len() {
        return Err(bad("the estimate has another number of cameras"));
    }
    let truth = ids
        .iter()
        .map(|id| {
            let c = scene
                .cameras
                .iter()
                .find(|c| &c.id == id)
                .ok_or_else(|| bad(format!("no camera `{id}`")))?;
            Ok((
                posed.index(id)?,
                c.params.build()?,
                c.params.intrinsics.fx,
                c.resolution,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let estimated = estimate
        .cameras
        .iter()
        .map(|(p, pose)| Ok((p.build()?, *pose)))
        .collect::<Result<Vec<_>>>()?;
    let visibility = VisibilitySpec {
        margin_px: sphere.require.margin_px,
    };
    let edge = PixelCentre::Integer.edge();
    let (mut sum, mut count) = (0.0, 0usize);
    for c in generated
        .candidates
        .iter()
        .filter(|c| c.status == CandidateStatus::Ok && c.capture.is_some())
    {
        let f = posed.at(
            r,
            c.q.as_deref()
                .ok_or_else(|| bad("a feasible candidate has no joints"))?,
        );
        let base_se3_tcp = f[base_frame].inverse() * f[tcp_frame];
        let base_se3_target = f[base_frame].inverse() * f[target_frame];
        for ((frame, true_model, fx, resolution), (model, cam_se3_ref)) in
            truth.iter().zip(&estimated)
        {
            let seen = project_points(
                true_model,
                *resolution,
                &f[*frame],
                &f[target_frame],
                &board.points,
                &visibility,
                edge,
            );
            let true_chain = f[*frame].inverse() * f[target_frame];
            let est_chain =
                cam_se3_ref * estimate.handeye.inverse() * base_se3_tcp.inverse() * base_se3_target;
            for (gt, b) in seen.iter().zip(&board.points) {
                let (true_px, Some(est_px)) = (gt.pixel, pixel(model, &est_chain, &b.xyz())) else {
                    continue;
                };
                let Some(true_px) = true_px.filter(|_| gt.visible()) else {
                    continue;
                };
                let depth = (true_chain * b.xyz()).z;
                let du = (est_px[0] - true_px[0], est_px[1] - true_px[1]);
                let mm = depth / fx * 1e3;
                // Per image axis, as the predicted σ² is a mean over pixel rows.
                sum += (du.0 * mm).powi(2) + (du.1 * mm).powi(2);
                count += 2;
            }
        }
    }
    if count == 0 {
        return Err(bad("no candidate sees the target"));
    }
    Ok((sum / count as f64).sqrt())
}
