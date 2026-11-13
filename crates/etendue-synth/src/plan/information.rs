//! The planner's information model: the parameters' numerical derivatives through
//! calibration-rs's own projection, each view's information, and the frames a view's chain is
//! made of.

use nalgebra::{DMatrix, Isometry3, Point3, Quaternion, Translation3, UnitQuaternion, Vector3};
use serde::{Deserialize, Serialize};
use vision_calibration_core::{CameraModel, CameraParams, DistortionParams, SensorParams};

use crate::Result;
use crate::poses::Mounting;

/// A rotation by `phi` (its axis times its angle), from `libm` sines.
pub(super) fn exp_so3(phi: Vector3<f64>) -> UnitQuaternion<f64> {
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
pub(super) fn perturbed(t: &Isometry3<f64>, k: usize, h: f64) -> Isometry3<f64> {
    let mut d = [0.0; 6];
    d[k] = h;
    let delta = Isometry3::from_parts(
        Translation3::new(d[3], d[4], d[5]),
        exp_so3(Vector3::new(d[0], d[1], d[2])),
    );
    t * delta
}

/// One camera's parameters, with each perturbed model.
pub(super) struct Cam {
    pub(super) model: CameraModel,
    pub(super) fx: f64,
    /// Per parameter: its label suffix and the models at `±h`.
    pub(super) params: Vec<(&'static str, f64, CameraModel, CameraModel)>,
}

impl Cam {
    pub(super) fn new(params: &CameraParams) -> Result<Self> {
        Ok(Self {
            model: params.build()?,
            fx: params.intrinsics.fx,
            params: camera_params(params)?,
        })
    }
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
pub(super) fn pixel(
    model: &CameraModel,
    cam_se3_target: &Isometry3<f64>,
    p: &Point3<f64>,
) -> Option<[f64; 2]> {
    let q = cam_se3_target * p;
    model.project_point_c(&q.coords).map(|u| [u.x, u.y])
}

pub(super) const POSE: [&str; 6] = ["rx", "ry", "rz", "tx", "ty", "tz"];
pub(super) const H_POSE: f64 = 1e-7;

/// The reference camera's pose relative to the robot's base at a view: eye-in-hand through the
/// hand-eye `gripper_se3_ref` and the tool's pose, eye-to-hand the hand-eye `ref_se3_base` itself.
pub(super) fn ref_se3_base(
    mounting: Mounting,
    handeye: &Isometry3<f64>,
    base_se3_tcp: &Isometry3<f64>,
) -> Isometry3<f64> {
    match mounting {
        Mounting::EyeInHand => handeye.inverse() * base_se3_tcp.inverse(),
        Mounting::EyeToHand => *handeye,
    }
}

/// The target's pose in the robot's base at a view, from its estimated pose: `base_se3_target`
/// eye-in-hand, `gripper_se3_target` (carried by the tool) eye-to-hand.
pub(super) fn target_in_base(
    mounting: Mounting,
    target: &Isometry3<f64>,
    base_se3_tcp: &Isometry3<f64>,
) -> Isometry3<f64> {
    match mounting {
        Mounting::EyeInHand => *target,
        Mounting::EyeToHand => base_se3_tcp * target,
    }
}

/// The robot's pose error a calibration weighs its pose corrections with (calibration-rs's
/// `robot_pose_sigma`): each view's robot pose is refined as `base_se3_tcp · Exp(δ)` under a
/// Gaussian prior of these σ on `δ`'s rotation (rad) and translation (m).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RobotPrior {
    pub rot_sigma_rad: f64,
    pub trans_sigma_m: f64,
}

impl RobotPrior {
    /// The prior's information on `δ`, or `None` for exact poses (either σ zero).
    fn information(self) -> Option<[f64; 6]> {
        let (r, t) = (self.rot_sigma_rad, self.trans_sigma_m);
        (r > 0.0 && t > 0.0).then(|| {
            let (r, t) = (1.0 / (r * r), 1.0 / (t * t));
            [r, r, r, t, t, t]
        })
    }
}

/// The calibration's parameters, as the planner models them: per camera its intrinsics,
/// distortion (and tilt); every camera's pose in the rig but the first's; the hand-eye (its
/// frame: the first camera's); the target's pose.
pub(super) struct Layout {
    pub(super) labels: Vec<String>,
    cam_offset: Vec<usize>,
    rig_offset: usize,
    handeye_offset: usize,
    pub(super) target_offset: usize,
}

impl Layout {
    pub(super) fn new(cams: &[Cam], mounting: Mounting) -> Self {
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
        let (handeye_label, target_label) = match mounting {
            Mounting::EyeInHand => ("gripper_se3_rig", "base_se3_target"),
            Mounting::EyeToHand => ("rig_se3_base", "gripper_se3_target"),
        };
        let handeye_offset = labels.len();
        labels.extend(POSE.iter().map(|k| format!("{handeye_label}.{k}")));
        let target_offset = labels.len();
        labels.extend(POSE.iter().map(|k| format!("{target_label}.{k}")));
        Self {
            labels,
            cam_offset,
            rig_offset,
            handeye_offset,
            target_offset,
        }
    }

    pub(super) fn len(&self) -> usize {
        self.labels.len()
    }
}

/// One view of the cell in the planner's frames, as the calibration estimates them.
pub(super) struct View {
    /// Per camera: `cam_se3_ref`, and the target points (target frame) it sees.
    pub(super) cameras: Vec<(Isometry3<f64>, Vec<Point3<f64>>)>,
    /// `gripper_se3_ref` eye-in-hand, `ref_se3_base` eye-to-hand.
    pub(super) handeye: Isometry3<f64>,
    /// `base_se3_target` eye-in-hand, `gripper_se3_target` eye-to-hand.
    pub(super) target: Isometry3<f64>,
    pub(super) base_se3_tcp: Isometry3<f64>,
}

/// A view's information `Jᵀ J / var` over `layout`'s parameters, and its pixel rows (two per
/// sighting). Each sighting's system weights (`w · jᵀ j` over the cell's parameters) are added to
/// `a`. With `robot`, the view's robot pose correction is a parameter too, under its prior, and is
/// marginalised out (the Schur complement), as in a calibration that refines the robot poses.
pub(super) fn view_information(
    cams: &[Cam],
    layout: &Layout,
    mounting: Mounting,
    view: &View,
    var: f64,
    robot: Option<RobotPrior>,
    a: &mut DMatrix<f64>,
) -> (DMatrix<f64>, usize) {
    let n = layout.len();
    let prior = robot.and_then(RobotPrior::information);
    let m = if prior.is_some() { n + 6 } else { n };
    let target_offset = layout.target_offset;
    let (handeye, target, tcp) = (&view.handeye, &view.target, &view.base_se3_tcp);
    let chain = |cam_se3_ref: &Isometry3<f64>,
                 he: &Isometry3<f64>,
                 t: &Isometry3<f64>,
                 tcp: &Isometry3<f64>| {
        cam_se3_ref * ref_se3_base(mounting, he, tcp) * target_in_base(mounting, t, tcp)
    };
    let diff = |plus: Option<[f64; 2]>, minus: Option<[f64; 2]>, h: f64| match (plus, minus) {
        (Some(a), Some(b)) => [(a[0] - b[0]) / (2.0 * h), (a[1] - b[1]) / (2.0 * h)],
        _ => [0.0, 0.0],
    };
    let mut info = DMatrix::<f64>::zeros(m, m);
    let mut rows = 0usize;
    for (c, (cam, (cam_se3_ref, seen))) in cams.iter().zip(&view.cameras).enumerate() {
        let nominal = chain(cam_se3_ref, handeye, target, tcp);
        for p in seen {
            let mut j = vec![[0.0f64; 2]; m];
            for (k, (_, h, plus, minus)) in cam.params.iter().enumerate() {
                j[layout.cam_offset[c] + k] =
                    diff(pixel(plus, &nominal, p), pixel(minus, &nominal, p), *h);
            }
            let at = |cam_se3_ref: &Isometry3<f64>,
                      he: &Isometry3<f64>,
                      t: &Isometry3<f64>,
                      tcp: &Isometry3<f64>| {
                pixel(&cam.model, &chain(cam_se3_ref, he, t, tcp), p)
            };
            let d = |f: &dyn Fn(f64) -> Option<[f64; 2]>| diff(f(H_POSE), f(-H_POSE), H_POSE);
            for k in 0..6 {
                if c > 0 {
                    j[layout.rig_offset + 6 * (c - 1) + k] =
                        d(&|s| at(&perturbed(cam_se3_ref, k, s), handeye, target, tcp));
                }
                j[layout.handeye_offset + k] =
                    d(&|s| at(cam_se3_ref, &perturbed(handeye, k, s), target, tcp));
                j[target_offset + k] =
                    d(&|s| at(cam_se3_ref, handeye, &perturbed(target, k, s), tcp));
                if prior.is_some() {
                    j[n + k] = d(&|s| at(cam_se3_ref, handeye, target, &perturbed(tcp, k, s)));
                }
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
            rows += 2;
        }
    }
    match prior {
        Some(p) => (marginalise(&info, n, &p), rows),
        None => (info, rows),
    }
}

/// The information on the first `n` parameters of `info` with the last six (a robot pose
/// correction, under the prior information `p`) marginalised out: `I₁₁ − I₁₂ (I₂₂ + P)⁻¹ I₂₁`.
/// The products are summed in order, not by a matrix kernel (bit-identical in wasm).
fn marginalise(info: &DMatrix<f64>, n: usize, p: &[f64; 6]) -> DMatrix<f64> {
    let mut i22 = nalgebra::Matrix6::<f64>::zeros();
    for r in 0..6 {
        for c in 0..6 {
            i22[(r, c)] = info[(n + r, n + c)];
        }
        i22[(r, r)] += p[r];
    }
    let inv = i22
        .cholesky()
        .expect("the prior makes it positive definite")
        .inverse();
    // `b = I₁₂ (I₂₂ + P)⁻¹`, then `I₁₁ − b I₂₁`.
    let mut b = DMatrix::<f64>::zeros(n, 6);
    for x in 0..n {
        for c in 0..6 {
            let mut s = 0.0;
            for k in 0..6 {
                s += info[(x, n + k)] * inv[(k, c)];
            }
            b[(x, c)] = s;
        }
    }
    let mut out = DMatrix::<f64>::zeros(n, n);
    for x in 0..n {
        for y in 0..n {
            let mut s = 0.0;
            for k in 0..6 {
                s += b[(x, k)] * info[(n + k, y)];
            }
            out[(x, y)] = info[(x, y)] - s;
        }
    }
    out
}

/// Every view's information, and the system σ's weights over their sightings.
pub(super) struct Information {
    pub(super) infos: Vec<DMatrix<f64>>,
    a: DMatrix<f64>,
    /// Pixel rows: `a` sums both axes of every sighting.
    rows: f64,
    /// A negligible prior (a millionth of an average view), so any set of views inverts.
    pub(super) prior: DMatrix<f64>,
    target_offset: usize,
}

impl Information {
    /// `rows` must be positive (some view sees the target).
    pub(super) fn new(
        infos: Vec<DMatrix<f64>>,
        a: DMatrix<f64>,
        rows: usize,
        target_offset: usize,
    ) -> Self {
        let n = a.nrows();
        let mut prior = DMatrix::<f64>::zeros(n, n);
        for x in 0..n {
            let mean: f64 = infos.iter().map(|m| m[(x, x)]).sum::<f64>() / infos.len() as f64;
            prior[(x, x)] = 1e-6 * mean.max(f64::MIN_POSITIVE);
        }
        Self {
            infos,
            a,
            rows: rows as f64,
            prior,
            target_offset,
        }
    }

    /// The system σ (mm) of the views whose information sums to `info`, and their covariance;
    /// `None` when they cannot determine the calibration.
    pub(super) fn sigma_of(&self, info: &DMatrix<f64>) -> Option<(f64, DMatrix<f64>)> {
        let cov = info.clone().cholesky()?.inverse();
        let mut t = 0.0;
        for x in 0..self.target_offset {
            for y in 0..self.target_offset {
                t += cov[(x, y)] * self.a[(x, y)];
            }
        }
        Some(((t / self.rows).max(0.0).sqrt(), cov))
    }

    /// The information of the views `set` (indices into `infos`), with the prior.
    pub(super) fn total(&self, set: &[usize]) -> DMatrix<f64> {
        let mut m = self.prior.clone();
        for &k in set {
            m += &self.infos[k];
        }
        m
    }
}
