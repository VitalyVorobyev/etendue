//! Calibration-pose planning (P18): from the sphere generator's feasible candidates
//! ([`crate::poses`]), the poses that minimise the predicted **system σ**.
//!
//! The system σ is how far, in millimetres across the line of sight, the calibrated cell would
//! place a point it knows in the robot's base frame, averaged over the target's points as every
//! feasible candidate sees them. It is the predicted covariance of the calibration propagated to
//! that position:
//! - **The calibration** estimates, per camera, the focal lengths, the principal point, the
//!   Brown-Conrady `k1, k2, p1, p2` and a Scheimpflug tilt; each camera's pose in the rig but the
//!   first's; the hand-eye; and the target's pose. Eye-in-hand (the rig on the tool) that is
//!   `gripper_se3_rig` and the target in the base; eye-to-hand (the target on the tool, P18b)
//!   `rig_se3_base` and the target on the gripper. That is calibration-rs's rig hand-eye with the
//!   rig refined (`docs/measurements/g17_sim_lab.md`). The robot poses are taken as exact.
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
//!
//! [`predict`] applies the same model to a ground truth's own captures (P20c: the twin of a real
//! calibration, its real poses), with each view's robot pose refined under a prior
//! ([`RobotPrior`]) if the calibration refines them; [`system_difference`] measures what it
//! predicts, between two estimates.

mod information;
mod predict;
mod system_error;

use etendue_kinematics::RobotModel;
use etendue_scene::{ScenarioSpec, SceneSpec, SphereGenerator, Step};
use nalgebra::{DMatrix, Point3};
use serde::{Deserialize, Serialize};

use crate::board::layout;
use crate::gt::{VisibilitySpec, project_points};
use crate::poses::{CandidateStatus, Generated, Mounting, Posed, mounting};
use crate::{Error, PixelCentre, Result};
pub use information::RobotPrior;
use information::{Cam, Information, Layout, View, view_information};
pub use predict::{Prediction, predict};
pub use system_error::{EstimatedCell, system_difference, system_error};

fn bad(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

/// The relative improvement in the system σ that a later candidate needs over an earlier one.
const TIE: f64 = 1e-7;

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
    /// The thinned grid's system σ (mm) with `k + 1` poses, for every `k` below the budget
    /// (`None` where so few poses cannot determine the calibration): the curve beside
    /// `sigma_mm`.
    #[serde(default)]
    pub grid_sigma_curve_mm: Vec<Option<f64>>,
    /// The thinned grid's capture ids.
    pub grid: Vec<String>,
    /// The system σ (mm) of every feasible candidate.
    pub all_sigma_mm: f64,
    /// Feasible candidates.
    pub feasible: usize,
    /// The predicted σ of each parameter with the chosen poses (labels as calibration-rs names
    /// them: `cam0.fx`, `cam1.cam_se3_rig.tx`, `gripper_se3_rig.rx`, `base_se3_target.tz`, …;
    /// eye-to-hand `rig_se3_base` and `gripper_se3_target`; px, radians, metres).
    pub labels: Vec<String>,
    pub sigma: Vec<f64>,
    /// The chosen poses as a stop-and-shoot scenario, in the generator's tour order.
    pub scenario: ScenarioSpec,
}

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
    let mounting = mounting(&posed, r, model, &sphere.look, &target.id)?;
    let board = layout(&target.geometry)?
        .ok_or_else(|| bad(format!("target `{}` has no printed layout", target.id)))?;
    let points: Vec<Point3<f64>> = board.points.iter().map(|b| b.xyz()).collect();
    let ids: Vec<String> = match &sphere.require.cameras {
        Some(c) => c.clone(),
        None => scene.cameras.iter().map(|c| c.id.clone()).collect(),
    };
    let mut frames = Vec::with_capacity(ids.len());
    let mut resolutions = Vec::with_capacity(ids.len());
    let cams = ids
        .iter()
        .map(|id| {
            let c = scene
                .cameras
                .iter()
                .find(|c| &c.id == id)
                .ok_or_else(|| bad(format!("no camera `{id}`")))?;
            frames.push(posed.index(id)?);
            resolutions.push(c.resolution);
            Cam::new(&c.params)
        })
        .collect::<Result<Vec<_>>>()?;
    let params = Layout::new(&cams, mounting);
    let (labels, target_offset, n) = (&params.labels, params.target_offset, params.len());

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
        let world_se3_ref = f[frames[0]];
        let base_se3_tcp = f[base_frame].inverse() * f[tcp_frame];
        // The hand-eye and the target's pose, as the calibration estimates them.
        let (handeye, target) = match mounting {
            Mounting::EyeInHand => (
                f[tcp_frame].inverse() * world_se3_ref,
                f[base_frame].inverse() * f[target_frame],
            ),
            Mounting::EyeToHand => (
                world_se3_ref.inverse() * f[base_frame],
                f[tcp_frame].inverse() * f[target_frame],
            ),
        };
        let cameras = cams
            .iter()
            .zip(frames.iter().zip(&resolutions))
            .map(|(cam, (&frame, &resolution))| {
                let seen = project_points(
                    &cam.model,
                    resolution,
                    &f[frame],
                    &f[target_frame],
                    &board.points,
                    &visibility,
                    edge,
                );
                let visible = seen
                    .iter()
                    .zip(&points)
                    .filter(|(gt, _)| gt.visible())
                    .map(|(_, p)| *p)
                    .collect();
                (f[frame].inverse() * world_se3_ref, visible)
            })
            .collect();
        let view = View {
            cameras,
            handeye,
            target,
            base_se3_tcp,
        };
        let (info, rows) = view_information(&cams, &params, mounting, &view, var, None, &mut a);
        infos.push(info);
        sightings += rows;
    }
    if sightings == 0 {
        return Err(bad("no candidate sees the target"));
    }
    let information = Information::new(infos, a, sightings, target_offset);
    let infos = &information.infos;
    let prior = &information.prior;
    let sigma_of = |info: &DMatrix<f64>| information.sigma_of(info);
    let total = |set: &[usize]| information.total(set);

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
            // Better by more than rounding: a symmetric cell's mirror candidates tie (to ~1e-9
            // here), and the earlier one is kept, so the plan does not hang on the last bits.
            if let Some((s, _)) = sigma_of(&trial)
                && best.is_none_or(|(_, b)| s < b * (1.0 - TIE))
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
    let thinned = |k: usize| -> Vec<usize> { (0..k).map(|j| tour[j * m / k]).collect() };
    let grid = thinned(spec.budget);
    let grid_sigma = sigma_of(&total(&grid)).map_or(f64::NAN, |s| s.0);
    let grid_sigma_curve: Vec<Option<f64>> = (1..=spec.budget)
        .map(|k| sigma_of(&total(&thinned(k))).map(|s| s.0))
        .collect();
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
        grid_sigma_curve_mm: grid_sigma_curve,
        grid: grid.iter().map(|&k| capture(k)).collect(),
        all_sigma_mm: all_sigma,
        feasible: feasible.len(),
        labels: labels.clone(),
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
