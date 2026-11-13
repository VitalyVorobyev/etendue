//! The planner's information model: the parameters' numerical derivatives through
//! calibration-rs's own projection, and the frames a view's chain is made of.

use nalgebra::{Isometry3, Point3, Quaternion, Translation3, UnitQuaternion, Vector3};
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
    pub(super) frame: usize,
    pub(super) resolution: [u32; 2],
    pub(super) model: CameraModel,
    pub(super) fx: f64,
    /// Per parameter: its label suffix and the models at `±h`.
    pub(super) params: Vec<(&'static str, f64, CameraModel, CameraModel)>,
}

pub(super) fn camera_params(
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
