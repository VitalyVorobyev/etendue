//! Calibration poses on spheres around a target (P7-6).
//!
//! An eye-in-hand sensor — a rig, a camera, or any frame the robot's tool
//! carries — is placed on spheres centred on the target: at radius `r`,
//! polar angle `θ` from the sphere's pole and azimuth `φ`, with its +Z at
//! the centre and its +Y along the target's "down" (so the images read
//! upright), then rolled about its axis. Each placement is a candidate. It is
//! kept when:
//!
//! 1. the robot reaches it (inverse kinematics), with every joint at least
//!    `joint_margin_rad` from its limits and the tool Jacobian's smallest
//!    singular value at least `min_sigma`;
//! 2. every required camera sees at least `min_visible_fraction` of the
//!    target's points (the ground truth's visibility tests: in front of the
//!    camera, inside the image with a margin, on a side facing it).
//!
//! The kept poses are ordered to keep the motion short (nearest neighbour,
//! then 2-opt, by the slowest joint's travel time) and become a stop-and-shoot
//! scenario of `ptp_pose` moves, each carrying the joint configuration it was
//! solved to, so baking never picks another IK branch.
//!
//! Deterministic: no random numbers. Inverse kinematics runs through the
//! candidates in a snake order (radius, then θ, then φ back and forth), each
//! solve seeded from the previous solution, so the arm stays on one branch
//! where it can.

use etendue_kinematics::{IkOptions, RobotModel};
use etendue_scene::{
    FrameGraph, GeneratorSpec, ScenarioSpec, SceneSpec, SphereGenerator, Step, TargetGeometry,
};
use nalgebra::{Isometry3, Rotation3, Translation3, UnitQuaternion, Vector3};
use serde::{Deserialize, Serialize};
use vision_calibration_core::CameraModel;

use crate::board::{Surface, layout};
use crate::gt::{BoardPoint, VisibilitySpec, project_points};
use crate::{Error, PixelCentre, Result};

/// Why a candidate was kept or not.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum CandidateStatus {
    /// Reachable, feasible, and seen by every required camera.
    Ok,
    /// No joint configuration reaches it.
    Unreachable,
    /// Reached, but a joint is too close to its limit.
    NearLimit {
        /// The joint.
        joint: String,
        /// Its distance from the limit (rad or m).
        margin: f64,
    },
    /// Reached, but too close to a singularity.
    NearSingular {
        /// The tool Jacobian's smallest singular value.
        sigma: f64,
    },
    /// A required camera does not see enough of the target.
    NotVisible {
        /// The camera.
        camera: String,
        /// Target points it sees.
        visible: usize,
        /// Target points.
        total: usize,
    },
}

/// One candidate sensor placement.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    /// Sphere radius, metres.
    pub radius_m: f64,
    /// Polar angle from the pole, radians.
    pub theta_rad: f64,
    /// Azimuth, radians.
    pub phi_rad: f64,
    /// Roll about the sensor axis, radians.
    pub roll_rad: f64,
    /// The sensor's world pose.
    pub world_se3_sensor: Isometry3<f64>,
    /// The tool pose that places it, in the robot's base frame.
    pub base_se3_tool: Isometry3<f64>,
    /// The joint configuration, when the robot reaches it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub q: Option<Vec<f64>>,
    /// Inverse kinematics found it only from a restart seed: it may be on
    /// another branch than its neighbours.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ik_restart: bool,
    /// Kept or why not.
    pub status: CandidateStatus,
    /// Its capture id, when kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture: Option<String>,
}

/// What the generator produced.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Generated {
    /// Every candidate, in `(r, θ, φ, roll)` order.
    pub candidates: Vec<Candidate>,
    /// The kept candidates as a stop-and-shoot scenario.
    pub scenario: ScenarioSpec,
}

/// How still a frame must stay (metres and radians) to count as carried
/// rigidly, or as fixed, when every joint moves.
const RIGID: f64 = 1e-9;

fn bad(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

/// The frame poses of the scene with robot `r` at `q` (the others at their
/// initial configuration).
struct Posed<'a> {
    scene: &'a SceneSpec,
    robots: &'a [RobotModel],
    graph: FrameGraph,
    rest: Vec<Vec<f64>>,
}

impl Posed<'_> {
    fn at(&self, r: usize, q: &[f64]) -> Vec<Isometry3<f64>> {
        let links: Vec<Vec<Isometry3<f64>>> = self
            .robots
            .iter()
            .enumerate()
            .map(|(i, m)| m.link_poses(if i == r { q } else { &self.rest[i] }))
            .collect();
        self.graph.resolve(|robot, link| links[robot][link])
    }

    fn index(&self, frame: &str) -> Result<usize> {
        self.graph
            .index_of(frame)
            .ok_or_else(|| bad(format!("no frame `{frame}` in the scene")))
    }
}

fn same(a: &Isometry3<f64>, b: &Isometry3<f64>) -> bool {
    (a.translation.vector - b.translation.vector).norm() < RIGID
        && a.rotation.angle_to(&b.rotation) < RIGID
}

/// A rotation whose columns are the frame axes `x, y, z`.
fn frame(x: Vector3<f64>, y: Vector3<f64>, z: Vector3<f64>) -> UnitQuaternion<f64> {
    UnitQuaternion::from_rotation_matrix(&Rotation3::from_basis_unchecked(&[x, y, z]))
}

/// The slowest joint's travel time between two configurations: the cost
/// the poses are ordered by.
fn travel(model: &RobotModel, a: &[f64], b: &[f64]) -> f64 {
    a.iter()
        .zip(b)
        .zip(model.active_joints())
        .map(|((x, y), j)| (x - y).abs() / j.max_velocity)
        .fold(0.0, f64::max)
}

/// Order `poses` (configurations) for a short tour from `start`: nearest
/// neighbour, then 2-opt (at most 50 passes; ties keep the earlier index).
fn order(model: &RobotModel, start: &[f64], poses: &[Vec<f64>]) -> Vec<usize> {
    let n = poses.len();
    let mut tour: Vec<usize> = Vec::with_capacity(n);
    let mut left: Vec<usize> = (0..n).collect();
    let mut here = start.to_vec();
    while !left.is_empty() {
        let (k, _) = left
            .iter()
            .enumerate()
            .map(|(k, &i)| (k, travel(model, &here, &poses[i])))
            .fold(
                (0, f64::INFINITY),
                |best, c| if c.1 < best.1 { c } else { best },
            );
        let i = left.remove(k);
        here.clone_from(&poses[i]);
        tour.push(i);
    }
    // 2-opt on the open path start → tour[0] → … → tour[n−1].
    let at = |tour: &[usize], k: isize| -> &[f64] {
        if k < 0 {
            start
        } else {
            &poses[tour[k as usize]]
        }
    };
    for _ in 0..50 {
        let mut improved = false;
        for i in 0..n {
            for j in i + 1..n {
                // Reverse tour[i..=j]: edges (i−1, i) and (j, j+1) change.
                let before = travel(model, at(&tour, i as isize - 1), at(&tour, i as isize))
                    + if j + 1 < n {
                        travel(model, at(&tour, j as isize), at(&tour, j as isize + 1))
                    } else {
                        0.0
                    };
                let after = travel(model, at(&tour, i as isize - 1), at(&tour, j as isize))
                    + if j + 1 < n {
                        travel(model, at(&tour, i as isize), at(&tour, j as isize + 1))
                    } else {
                        0.0
                    };
                if after < before - 1e-12 {
                    tour[i..=j].reverse();
                    improved = true;
                }
            }
        }
        if !improved {
            break;
        }
    }
    tour
}

/// Generate the candidates and the scenario of `spec` for `scene`, whose
/// robots are `robots` (aligned with `scene.robots`), sampled at `dt`.
///
/// # Errors
///
/// [`Error::InvalidInput`] if the spec names an unknown robot, frame,
/// target or camera; if the sensor is not carried rigidly by the robot's
/// tool; if the target moves with the robot (eye-to-hand is not generated)
/// or has no printed layout; or if a parameter is out of range.
pub fn generate(
    scene: &SceneSpec,
    robots: &[RobotModel],
    spec: &SphereGenerator,
    dt: f64,
) -> Result<Generated> {
    if spec.radii_m.is_empty() || !spec.radii_m.iter().all(|r| r.is_finite() && *r > 0.0) {
        return Err(bad("radii must be positive"));
    }
    if spec.theta.steps == 0 || spec.phi.steps == 0 {
        return Err(bad("θ and φ need at least one step each"));
    }
    let f = spec.require.min_visible_fraction;
    if !(f > 0.0 && f <= 1.0) {
        return Err(bad("min_visible_fraction must be in (0, 1]"));
    }
    if !(spec.speed_scale > 0.0 && spec.speed_scale <= 1.0) {
        return Err(bad("speed_scale must be in (0, 1]"));
    }
    let r = scene
        .robots
        .iter()
        .position(|x| x.id == spec.robot)
        .ok_or_else(|| bad(format!("no robot `{}`", spec.robot)))?;
    let model = robots
        .get(r)
        .ok_or_else(|| bad("a robot model per scene robot"))?;
    let links: Vec<Vec<String>> = robots.iter().map(|m| m.links().to_vec()).collect();
    let graph = FrameGraph::build(scene, &links).map_err(|e| bad(e.to_string()))?;
    let rest: Vec<Vec<f64>> = scene
        .robots
        .iter()
        .zip(robots)
        .map(|(s, m)| s.initial_q.clone().unwrap_or_else(|| vec![0.0; m.dof()]))
        .collect();
    let posed = Posed {
        scene,
        robots,
        graph,
        rest,
    };
    let q_ref = posed.rest[r].clone();
    let names = model.links();
    let base_frame = posed.index(&format!("{}/{}", spec.robot, names[model.base_link()]))?;
    let tcp_frame = posed.index(&format!("{}/{}", spec.robot, names[model.tcp_link()]))?;
    let look_frame = posed.index(&spec.look)?;
    let target = posed
        .scene
        .targets
        .iter()
        .find(|t| t.id == spec.target)
        .ok_or_else(|| bad(format!("no target `{}`", spec.target)))?;
    let target_frame = posed.index(&target.id)?;

    // Rigidity: the sensor rides on the tool; the base and the target do not
    // move with the robot. Checked by moving every joint.
    let q_alt: Vec<f64> = q_ref
        .iter()
        .zip(model.active_joints())
        .map(|(q, j)| (q + 0.3).min(j.upper).max(j.lower))
        .collect();
    let (a, b) = (posed.at(r, &q_ref), posed.at(r, &q_alt));
    let tcp_se3_look = a[tcp_frame].inverse() * a[look_frame];
    if !same(&tcp_se3_look, &(b[tcp_frame].inverse() * b[look_frame])) {
        return Err(bad(format!(
            "`{}` is not carried rigidly by `{}`'s tool: the generator places an eye-in-hand sensor",
            spec.look, spec.robot
        )));
    }
    if !same(&a[base_frame], &b[base_frame]) {
        return Err(bad(format!(
            "`{}`'s base moves with its joints",
            spec.robot
        )));
    }
    if !same(&a[target_frame], &b[target_frame]) {
        return Err(bad(format!(
            "target `{}` moves with `{}`: eye-to-hand poses are not generated",
            target.id, spec.robot
        )));
    }
    let world_se3_base = a[base_frame];
    let world_se3_target = a[target_frame];
    let tcp_se3_sensor = tcp_se3_look * spec.look_se3_sensor.unwrap_or_else(Isometry3::identity);
    let sensor_se3_tcp = tcp_se3_sensor.inverse();

    let board = layout(&target.geometry)?.ok_or_else(|| {
        bad(format!(
            "target `{}` has no printed layout to look at",
            target.id
        ))
    })?;
    let points: Vec<BoardPoint> = board.points;
    let (centre_default, pole_default, down) = match board.surface {
        Surface::Plane => (Vector3::zeros(), Vector3::z(), -Vector3::y()),
        Surface::Cylinder { .. } => {
            let length = match &target.geometry {
                TargetGeometry::Printed { target } => target.cylinder().map_or(0.0, |c| c.1),
                _ => 0.0,
            };
            (
                Vector3::new(0.0, 0.0, length / 2.0),
                Vector3::x(),
                -Vector3::z(),
            )
        }
    };
    let centre = spec.centre_m.map_or(centre_default, Vector3::from);
    let pole = spec
        .pole
        .map_or(pole_default, Vector3::from)
        .try_normalize(1e-12)
        .ok_or_else(|| bad("the pole must be a non-zero vector"))?;
    let to_pole = UnitQuaternion::rotation_between(&Vector3::z(), &pole).unwrap_or_else(|| {
        UnitQuaternion::from_axis_angle(&Vector3::x_axis(), std::f64::consts::PI)
    });

    let cameras: Vec<String> = match &spec.require.cameras {
        Some(c) => c.clone(),
        None => scene.cameras.iter().map(|c| c.id.clone()).collect(),
    };
    let camera_models = cameras
        .iter()
        .map(|id| {
            let c = scene
                .cameras
                .iter()
                .find(|c| &c.id == id)
                .ok_or_else(|| bad(format!("no camera `{id}`")))?;
            let frame = posed.index(id)?;
            let model: CameraModel = c.params.build()?;
            Ok((id.clone(), frame, model, c.resolution))
        })
        .collect::<Result<Vec<_>>>()?;
    let visibility = VisibilitySpec {
        margin_px: spec.require.margin_px,
    };
    let edge = PixelCentre::Integer.edge();

    let rolls = if spec.roll_rad.is_empty() {
        vec![0.0]
    } else {
        spec.roll_rad.clone()
    };
    let thetas = spec.theta.values();
    let phis = spec.phi.values();

    // Candidates in (r, θ, φ, roll) order, and the snake order IK visits them in.
    let mut candidates = Vec::new();
    let mut snake = Vec::new();
    let mut labels = Vec::new();
    for (ri, &radius) in spec.radii_m.iter().enumerate() {
        for (ti, &theta) in thetas.iter().enumerate() {
            for (pi, &phi) in phis.iter().enumerate() {
                for (oi, &roll) in rolls.iter().enumerate() {
                    let d = to_pole
                        * Vector3::new(
                            theta.sin() * phi.cos(),
                            theta.sin() * phi.sin(),
                            theta.cos(),
                        );
                    let eye = centre + d * radius;
                    let z = -d;
                    let y = (down - z * z.dot(&down))
                        .try_normalize(1e-9)
                        .unwrap_or_else(|| {
                            let x = Vector3::x();
                            (x - z * z.dot(&x)).normalize()
                        });
                    let x = y.cross(&z);
                    let target_se3_sensor = Isometry3::from_parts(
                        Translation3::from(eye),
                        frame(x, y, z) * UnitQuaternion::from_axis_angle(&Vector3::z_axis(), roll),
                    );
                    let world_se3_sensor = world_se3_target * target_se3_sensor;
                    candidates.push(Candidate {
                        radius_m: radius,
                        theta_rad: theta,
                        phi_rad: phi,
                        roll_rad: roll,
                        world_se3_sensor,
                        base_se3_tool: world_se3_base.inverse() * world_se3_sensor * sensor_se3_tcp,
                        q: None,
                        ik_restart: false,
                        status: CandidateStatus::Unreachable,
                        capture: None,
                    });
                    labels.push(format!("s{ri}_t{ti}_p{pi}_r{oi}"));
                }
            }
        }
    }
    let (nt, np, no) = (thetas.len(), phis.len(), rolls.len());
    for ri in 0..spec.radii_m.len() {
        for ti in 0..nt {
            for pk in 0..np {
                // φ back and forth, so consecutive candidates stay close.
                let pi = if (ri * nt + ti) % 2 == 0 {
                    pk
                } else {
                    np - 1 - pk
                };
                for oi in 0..no {
                    snake.push(((ri * nt + ti) * np + pi) * no + oi);
                }
            }
        }
    }

    let local = IkOptions {
        restarts: 0,
        ..IkOptions::default()
    };
    let mut seed = q_ref.clone();
    for &i in &snake {
        let c = &mut candidates[i];
        let solved = model
            .ik(&c.base_se3_tool, &seed, &local)
            .or_else(|_| model.ik(&c.base_se3_tool, &q_ref, &local))
            .map(|s| (s.q, false))
            .or_else(|_| {
                model
                    .ik(&c.base_se3_tool, &q_ref, &IkOptions::default())
                    .map(|s| (s.q, true))
            });
        let Ok((q, restart)) = solved else {
            continue;
        };
        seed.clone_from(&q);
        c.ik_restart = restart;
        c.status = feasibility(model, &q, spec);
        if c.status == CandidateStatus::Ok {
            let frames = posed.at(r, &q);
            for (id, frame, camera, resolution) in &camera_models {
                let seen = project_points(
                    camera,
                    *resolution,
                    &frames[*frame],
                    &world_se3_target,
                    &points,
                    &visibility,
                    edge,
                )
                .iter()
                .filter(|p| p.visible())
                .count();
                if (seen as f64) < f * points.len() as f64 {
                    c.status = CandidateStatus::NotVisible {
                        camera: id.clone(),
                        visible: seen,
                        total: points.len(),
                    };
                    break;
                }
            }
        }
        c.q = Some(q);
    }

    let kept: Vec<usize> = (0..candidates.len())
        .filter(|&i| candidates[i].status == CandidateStatus::Ok)
        .collect();
    let configurations: Vec<Vec<f64>> = kept
        .iter()
        .map(|&i| candidates[i].q.clone().expect("kept candidates are solved"))
        .collect();
    let mut steps = Vec::with_capacity(2 * kept.len() + 1);
    for k in order(model, &q_ref, &configurations) {
        let i = kept[k];
        candidates[i].capture = Some(labels[i].clone());
        steps.push(Step::PtpPose {
            robot: spec.robot.clone(),
            base_se3_tool: candidates[i].base_se3_tool,
            speed_scale: spec.speed_scale,
            q: candidates[i].q.clone(),
        });
        steps.push(Step::Capture {
            id: Some(labels[i].clone()),
        });
    }
    if spec.return_home && !kept.is_empty() {
        steps.push(Step::PtpJoints {
            robot: spec.robot.clone(),
            q: q_ref,
            speed_scale: spec.speed_scale,
        });
    }
    let scenario = ScenarioSpec {
        version: 1,
        dt,
        steps,
        description: Some(format!(
            "{} sphere poses of `{}` around `{}` ({} candidates)",
            kept.len(),
            spec.look,
            spec.target,
            candidates.len()
        )),
        generator: Some(GeneratorSpec::Sphere(spec.clone())),
    };
    Ok(Generated {
        candidates,
        scenario,
    })
}

/// Joint margin and singularity checks of a reached configuration.
fn feasibility(model: &RobotModel, q: &[f64], spec: &SphereGenerator) -> CandidateStatus {
    for (v, j) in q.iter().zip(model.active_joints()) {
        let margin = (v - j.lower).min(j.upper - v);
        if margin < spec.joint_margin_rad {
            return CandidateStatus::NearLimit {
                joint: j.name.clone(),
                margin,
            };
        }
    }
    let (_, jacobian) = model.tcp_jacobian(q);
    let sigma = jacobian.singular_values().min();
    if sigma < spec.min_sigma {
        return CandidateStatus::NearSingular { sigma };
    }
    CandidateStatus::Ok
}
