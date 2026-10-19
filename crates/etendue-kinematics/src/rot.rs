//! Rotations from angles, and angles from rotations, with the `libm` crate's transcendentals, so
//! forward and inverse kinematics give the same bits natively and in wasm (gate G12.1; CLAUDE.md
//! gotcha 6). nalgebra's own constructors call the platform's `sin`, `cos` and `acos`.

use nalgebra::{Quaternion, Unit, UnitQuaternion, Vector3};

/// The rotation by `angle` about the unit `axis` (as `UnitQuaternion::from_axis_angle`).
#[must_use]
pub(crate) fn axis_angle(axis: &Unit<Vector3<f64>>, angle: f64) -> UnitQuaternion<f64> {
    let (s, c) = (libm::sin(0.5 * angle), libm::cos(0.5 * angle));
    UnitQuaternion::new_unchecked(Quaternion::new(c, axis.x * s, axis.y * s, axis.z * s))
}

/// URDF's `rpy`: roll about x, then pitch about y, then yaw about z, in fixed axes (as
/// `UnitQuaternion::from_euler_angles`).
#[must_use]
pub(crate) fn rpy(roll: f64, pitch: f64, yaw: f64) -> UnitQuaternion<f64> {
    let (sr, cr) = (libm::sin(0.5 * roll), libm::cos(0.5 * roll));
    let (sp, cp) = (libm::sin(0.5 * pitch), libm::cos(0.5 * pitch));
    let (sy, cy) = (libm::sin(0.5 * yaw), libm::cos(0.5 * yaw));
    UnitQuaternion::new_unchecked(Quaternion::new(
        cr * cp * cy + sr * sp * sy,
        sr * cp * cy - cr * sp * sy,
        cr * sp * cy + sr * cp * sy,
        cr * cp * sy - sr * sp * cy,
    ))
}

/// The rotation vector (axis times angle, the angle in `[0, π]`) of `q`.
#[must_use]
pub(crate) fn scaled_axis(q: &UnitQuaternion<f64>) -> Vector3<f64> {
    let (w, v) = if q.w < 0.0 {
        (-q.w, -q.imag())
    } else {
        (q.w, q.imag())
    };
    let n = v.norm();
    if n == 0.0 {
        return Vector3::zeros();
    }
    v * (2.0 * libm::atan2(n, w) / n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn they_agree_with_nalgebra() {
        let axis = Unit::new_normalize(Vector3::new(0.3, -0.5, 0.8));
        for angle in [-3.0, -0.7, 0.0, 1e-9, 0.4, 2.9] {
            let a = axis_angle(&axis, angle);
            assert!(a.angle_to(&UnitQuaternion::from_axis_angle(&axis, angle)) < 1e-15);
            let r = scaled_axis(&a);
            assert!(
                (r - UnitQuaternion::from_axis_angle(&axis, angle).scaled_axis()).norm() < 1e-14
            );
        }
        for (r, p, y) in [(0.1, -0.4, 2.0), (3.0, 1.5, -2.5), (0.0, 0.0, 0.0)] {
            assert!(rpy(r, p, y).angle_to(&UnitQuaternion::from_euler_angles(r, p, y)) < 1e-15);
        }
    }
}
