//! A physical camera: a sensor behind an ideal thick lens with pupils, placed by its body.
//!
//! This is the camera model of ADR 0008. It owns every conversion between the camera's
//! **mechanical frame** M (where a scene places the camera) and the optical points that the lens
//! and its focus setting put on M's axis. Callers give points in M and get pixels, rays and blur
//! back; nothing outside this module applies an optical offset.
//!
//! # Frames
//!
//! M has its origin at the image-plane datum S (where the lens axis meets the nominal image
//! plane) and the calibration-rs/OpenCV axes: `+z` along the lens axis toward the object, `+x`
//! along the sensor rows, `+y` down. The optical points sit on M's `z` axis:
//!
//! | Point | `z` in M |
//! |---|---|
//! | S, the sensor pivot | `0` |
//! | H′, the rear principal point | `v0` |
//! | H, the front principal point | `v0 + g` |
//! | E, the entrance pupil | `v0 + g + e` |
//! | E′, the exit pupil | `v0 − s′E`, `s′E = f·e / (e − f)` |
//! | F′, F, the focal points | `v0 − f`, `v0 + g + f` |
//!
//! `v0` is the image distance for the focus distance `D`, which is measured from S, as lens
//! focus scales are. With `T = D − g`, the object distance `d` (from H) and `v0` (from H′) solve
//! `d + v0 = T`, `1/d + 1/v0 = 1/f`:
//!
//! ```text
//! v0 = (T − √(T² − 4fT)) / 2,     d = T − v0         (T ≥ 4f)
//! ```
//!
//! The lens focuses as a unit: refocusing moves H, H′, E and E′ together, and the focal length
//! in pixels changes with them (focus breathing).
//!
//! # Projection
//!
//! The chief ray of every object point passes through the entrance pupil E, so the pinhole's
//! centre of projection is E. The projection frame P has its origin at E and M's axes, up to a
//! small roll (below). In image space the chief rays leave E′ with their slopes divided by the
//! pupil magnification `m_p = f / (f − e)` and meet the sensor `L′ = z_E′` behind E′. For an
//! untilted sensor that is the pinhole
//!
//! ```text
//! fx = fy = L′ / (m_p · pitch) = m0 · (d − e) / pitch,     m0 = v0 / d.
//! ```
//!
//! A tilted sensor makes the exact map from P's normalised coordinates to pixels a homography.
//! It is decomposed into calibration-rs's `K · H(τ′)` (OpenCV's tilted-sensor model):
//! - the model tilt τ′ comes from the homography's last row;
//! - `K = H_phys · H(τ′)⁻¹`.
//!
//! With `m_p = 1` the model tilt is the physical one, and K is the plain pinhole. With
//! `m_p ≠ 1`:
//! - for a tilt about one axis, `tan τ′ = tan τ / m_p`, and that axis's focal length scales by
//!   `cos τ′ / cos τ`;
//! - for a tilt about both axes, `K` would carry a lower-left term that calibration-rs's
//!   upper-triangular K cannot hold. A roll ψ of P about the axis removes it, leaving a small
//!   skew; ψ is found by bisection.
//!
//! The decomposition is exact. The tests compare it with an independent paraxial trace
//! ([`crate::optics::paraxial`]).
//!
//! # Blur
//!
//! The geometric circle of confusion is the exit pupil (diameter `m_p · f / N`) projected from
//! the image point onto the sensor. It uses the sensor's axial position at the image's lateral
//! position, as [`crate::optics::coc`] does; with `e = 0` and `g = 0` the two agree.
//!
//! - **Exact:** without tilt; and with tilt, its zero locus (the plane of best focus, the
//!   conjugate of the sensor plane) and the blur's centre (the chief ray).
//! - **First order with tilt, off that plane:** the oblique chief ray meets the tilted sensor
//!   beside the paraxial image's position. Over a ±0.2 rad field the diameter is within
//!   `0.6 · tan θ` (relative) of the traced ellipse's mean diameter, θ the sensor's tilt from the
//!   axis: about 5 % at 5°.

use nalgebra::{
    Isometry3, Matrix3, Point2, Point3, Rotation3, Translation3, UnitQuaternion, Vector3,
};
use serde::{Deserialize, Serialize};
use vision_calibration_core::{
    CameraModel, CameraParams, FxFyCxCySkew, IntrinsicsParams, ScheimpflugParams, SensorParams,
};

use crate::{Error, Result};

/// How a lens focuses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Focusing {
    /// The whole lens moves along its axis; its focal length does not change.
    #[default]
    Unit,
}

/// An ideal thick lens with its entrance pupil.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lens {
    /// Effective focal length `f` (m).
    pub focal_length_m: f64,
    /// f-number `N = f / D_E`, with `D_E` the entrance pupil diameter.
    pub f_number: f64,
    /// Signed `g = z_H − z_H′` (m): positive when H lies on the object side of H′.
    pub principal_gap_m: f64,
    /// Signed `e = z_E − z_H` (m): the entrance pupil's position relative to H.
    #[serde(default)]
    pub entrance_pupil_m: f64,
    /// How the lens focuses.
    #[serde(default)]
    pub focusing: Focusing,
}

/// The optical points of a camera, as `z` coordinates in its mechanical frame (m).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CardinalPoints {
    /// S, the sensor pivot and image-plane datum: always `0`.
    pub sensor: f64,
    /// H′, the rear principal point.
    pub rear_principal: f64,
    /// H, the front principal point.
    pub front_principal: f64,
    /// E, the entrance pupil.
    pub entrance_pupil: f64,
    /// E′, the exit pupil.
    pub exit_pupil: f64,
    /// F′, the rear focal point.
    pub rear_focal: f64,
    /// F, the front focal point.
    pub front_focal: f64,
}

/// The sensor plane in the mechanical frame: through the origin S.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SensorPlane {
    /// Unit normal, toward the lens.
    pub normal: Vector3<f64>,
    /// The direction of increasing physical `x` on the sensor (unit).
    pub x_axis: Vector3<f64>,
    /// The direction of increasing physical `y` on the sensor (unit).
    pub y_axis: Vector3<f64>,
}

/// Depth-of-field limits along the axis, as `z` in the mechanical frame (m).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DepthOfField {
    /// The nearest point whose blur is within the limit.
    pub near: f64,
    /// The farthest one; `None` when everything beyond `near` is within the limit.
    pub far: Option<f64>,
    /// The focus distance from S at which `far` goes to infinity (the hyperfocal distance).
    pub hyperfocal: f64,
}

/// A camera body with a sensor and a thick lens, focused at a distance. See the module docs.
#[derive(Clone, Debug)]
pub struct PhysicalCamera {
    lens: Lens,
    pixel_pitch_m: f64,
    focus_distance_m: f64,
    tilt: [f64; 2],
    v0: f64,
    d: f64,
    pupil_magnification: f64,
    points: CardinalPoints,
    sensor: SensorPlane,
    principal_point: [f64; 2],
    mech_se3_projection: Isometry3<f64>,
    pinhole: CameraParams,
    model: CameraModel,
}

fn invalid(reason: impl Into<String>) -> Error {
    Error::InvalidInput {
        reason: reason.into(),
    }
}

fn finite(name: &str, x: f64) -> Result<f64> {
    if x.is_finite() {
        Ok(x)
    } else {
        Err(invalid(format!("{name} must be finite, got {x}")))
    }
}

/// The physical sensor's orientation for tilt `(τx, τy)`: rows are its `x` axis, `y` axis and
/// normal. This is the rotation calibration-rs's (OpenCV's) tilted-sensor model uses, so that a
/// lens with unit pupil magnification has a model tilt equal to the physical one.
fn sensor_rotation(tilt_x: f64, tilt_y: f64) -> Matrix3<f64> {
    let rx = Rotation3::from_axis_angle(&Vector3::x_axis(), -tilt_x);
    let ry = Rotation3::from_axis_angle(&Vector3::y_axis(), -tilt_y);
    *(ry * rx).matrix()
}

/// The model tilt τ′ whose tilted-sensor homography has `row` as its last row (up to scale), and
/// `K = h · H(τ′)⁻¹` normalised to `K[2,2] = 1`.
fn decompose(h: &Matrix3<f64>) -> Result<([f64; 2], Matrix3<f64>)> {
    let row = h.row(2).transpose();
    let n = row.norm();
    let r = if row.z < 0.0 { -row / n } else { row / n };
    let tilt_y = r.x.clamp(-1.0, 1.0).asin();
    let tilt_x = (-r.y).atan2(r.z);
    let sensor = ScheimpflugParams { tilt_x, tilt_y }.compile();
    let k = h * sensor.h_inv;
    if k[(2, 2)].abs() < f64::EPSILON || !k.iter().all(|x| x.is_finite()) {
        return Err(Error::Numerical(format!(
            "degenerate sensor homography {h}"
        )));
    }
    Ok(([tilt_x, tilt_y], k / k[(2, 2)]))
}

fn roll(angle: f64) -> Matrix3<f64> {
    *Rotation3::from_axis_angle(&Vector3::z_axis(), angle).matrix()
}

impl PhysicalCamera {
    /// A camera with `lens` on a sensor of pitch `pixel_pitch_m`, focused at `focus_distance_m`
    /// from the image-plane datum, with the sensor tilted by `tilt = [τx, τy]` (radians) about
    /// the datum point. `as_built` gives what the optics do not determine: the principal point
    /// and the distortion. Its focal lengths, skew and sensor model are replaced by the derived
    /// ones.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] for a non-positive focal length, f-number or pitch, a non-finite
    /// value, an entrance pupil at or beyond the front focal point (`e ≥ f`), a focus closer than
    /// the lens can reach (`D − g < 4f`), or an `as_built` that is not a pinhole with
    /// `fx_fy_cx_cy_skew` intrinsics. [`Error::Numerical`] if the tilted sensor cannot be
    /// expressed in calibration-rs's model.
    pub fn new(
        lens: Lens,
        pixel_pitch_m: f64,
        focus_distance_m: f64,
        tilt: [f64; 2],
        as_built: &CameraParams,
    ) -> Result<Self> {
        let f = finite("focal length", lens.focal_length_m)?;
        let g = finite("principal gap", lens.principal_gap_m)?;
        let e = finite("entrance pupil position", lens.entrance_pupil_m)?;
        let big_d = finite("focus distance", focus_distance_m)?;
        finite("f-number", lens.f_number)?;
        finite("pixel pitch", pixel_pitch_m)?;
        let [tilt_x, tilt_y] = [finite("tilt x", tilt[0])?, finite("tilt y", tilt[1])?];
        if f <= 0.0 || lens.f_number <= 0.0 || pixel_pitch_m <= 0.0 {
            return Err(invalid(format!(
                "focal length, f-number and pixel pitch must be positive, got {f}, {}, {pixel_pitch_m}",
                lens.f_number
            )));
        }
        if e >= f {
            return Err(invalid(format!(
                "the entrance pupil ({e} m from H) must lie inside the front focal length ({f} m)"
            )));
        }
        let t = big_d - g;
        if t.partial_cmp(&(4.0 * f)) != Some(std::cmp::Ordering::Greater) {
            return Err(invalid(format!(
                "a focus distance of {big_d} m is closer than this lens can focus: \
                 D − g must exceed 4f = {} m",
                4.0 * f
            )));
        }
        let IntrinsicsParams::FxFyCxCySkew { params: built } = &as_built.intrinsics;
        let (cx, cy) = (built.cx, built.cy);

        // The conjugates at the focus setting, and where the lens's points are in M.
        let v0 = (t - (t * t - 4.0 * f * t).sqrt()) / 2.0;
        let d = t - v0;
        let pupil_magnification = f / (f - e);
        let exit_from_rear = f * e / (e - f);
        let points = CardinalPoints {
            sensor: 0.0,
            rear_principal: v0,
            front_principal: v0 + g,
            entrance_pupil: v0 + g + e,
            exit_pupil: v0 - exit_from_rear,
            rear_focal: v0 - f,
            front_focal: v0 + g + f,
        };
        let rot = sensor_rotation(tilt_x, tilt_y);
        let sensor = SensorPlane {
            x_axis: rot.row(0).transpose(),
            y_axis: rot.row(1).transpose(),
            normal: rot.row(2).transpose(),
        };

        // The exact map from M's normalised coordinates at E, q = (x/z, y/z, 1), to pixels: the
        // chief ray leaves E′ with slopes q/m_p and meets the sensor (the image is inverted, and
        // read out upright).
        let scale = Matrix3::from_diagonal(&Vector3::new(
            1.0 / pupil_magnification,
            1.0 / pupil_magnification,
            1.0,
        ));
        let z_exit = points.exit_pupil;
        let dn = scale * sensor.normal;
        let n_e = z_exit * sensor.normal.z;
        let row = |c: f64, axis: &Vector3<f64>| -> Vector3<f64> {
            c * dn - (z_exit * axis.z * dn - n_e * (scale * axis)) / pixel_pitch_m
        };
        let h_phys = Matrix3::from_rows(&[
            row(cx, &sensor.x_axis).transpose(),
            row(cy, &sensor.y_axis).transpose(),
            dn.transpose(),
        ]);

        // Decompose into calibration-rs's K · H(τ′); roll P until K is upper triangular.
        let lower = |psi: f64| -> Result<f64> { Ok(decompose(&(h_phys * roll(psi)))?.1[(1, 0)]) };
        let scale_k = h_phys[(0, 0)].abs().max(1.0);
        let mut psi = 0.0;
        if lower(0.0)?.abs() > 1e-14 * scale_k {
            let (mut a, mut b) = (-0.5_f64, 0.5_f64);
            let (mut fa, fb) = (lower(a)?, lower(b)?);
            if fa * fb > 0.0 {
                return Err(Error::Numerical(format!(
                    "no projection-frame roll makes the tilted sensor ({tilt_x}, {tilt_y}) \
                     expressible in calibration-rs's model"
                )));
            }
            for _ in 0..200 {
                let m = 0.5 * (a + b);
                let fm = lower(m)?;
                if fm == 0.0 || (b - a) < 1e-17 {
                    a = m;
                    b = m;
                    break;
                }
                if fa * fm <= 0.0 {
                    b = m;
                } else {
                    a = m;
                    fa = fm;
                }
            }
            psi = 0.5 * (a + b);
        }
        let ([model_tilt_x, model_tilt_y], k) = decompose(&(h_phys * roll(psi)))?;
        if !(k[(0, 0)] > 0.0 && k[(1, 1)] > 0.0) {
            return Err(Error::Numerical(format!(
                "derived focal lengths are not positive: {k}"
            )));
        }
        let mut pinhole = as_built.clone();
        pinhole.intrinsics = IntrinsicsParams::FxFyCxCySkew {
            params: FxFyCxCySkew {
                fx: k[(0, 0)],
                fy: k[(1, 1)],
                cx,
                cy,
                skew: if psi == 0.0 { 0.0 } else { k[(0, 1)] },
            },
        };
        pinhole.sensor = if tilt_x == 0.0 && tilt_y == 0.0 {
            SensorParams::Identity
        } else {
            SensorParams::Scheimpflug {
                params: ScheimpflugParams {
                    tilt_x: model_tilt_x,
                    tilt_y: model_tilt_y,
                },
            }
        };
        let model = pinhole
            .build()
            .map_err(|err| invalid(format!("the derived pinhole does not build: {err}")))?;
        let mech_se3_projection = Isometry3::from_parts(
            Translation3::new(0.0, 0.0, points.entrance_pupil),
            UnitQuaternion::from_axis_angle(&Vector3::z_axis(), psi),
        );
        Ok(Self {
            lens,
            pixel_pitch_m,
            focus_distance_m: big_d,
            tilt: [tilt_x, tilt_y],
            v0,
            d,
            pupil_magnification,
            points,
            sensor,
            principal_point: [cx, cy],
            mech_se3_projection,
            pinhole,
            model,
        })
    }

    /// The lens.
    #[must_use]
    pub fn lens(&self) -> &Lens {
        &self.lens
    }

    /// The pixel pitch (m).
    #[must_use]
    pub fn pixel_pitch_m(&self) -> f64 {
        self.pixel_pitch_m
    }

    /// The focus distance from the image-plane datum (m).
    #[must_use]
    pub fn focus_distance_m(&self) -> f64 {
        self.focus_distance_m
    }

    /// The physical sensor tilt `[τx, τy]` (radians).
    #[must_use]
    pub fn tilt(&self) -> [f64; 2] {
        self.tilt
    }

    /// The image distance at the focus setting: H′ to the sensor (m).
    #[must_use]
    pub fn image_distance_m(&self) -> f64 {
        self.v0
    }

    /// The object distance at the focus setting: H to the plane in focus (m).
    #[must_use]
    pub fn object_distance_m(&self) -> f64 {
        self.d
    }

    /// The lateral magnification at the focus setting, `v0 / d` (positive; the image is
    /// inverted).
    #[must_use]
    pub fn magnification(&self) -> f64 {
        self.v0 / self.d
    }

    /// The pupil magnification `m_p = D_exit / D_entrance = f / (f − e)`.
    #[must_use]
    pub fn pupil_magnification(&self) -> f64 {
        self.pupil_magnification
    }

    /// The entrance pupil diameter `f / N` (m).
    #[must_use]
    pub fn entrance_pupil_diameter_m(&self) -> f64 {
        self.lens.focal_length_m / self.lens.f_number
    }

    /// The exit pupil diameter `m_p · f / N` (m).
    #[must_use]
    pub fn exit_pupil_diameter_m(&self) -> f64 {
        self.pupil_magnification * self.entrance_pupil_diameter_m()
    }

    /// The working (effective) f-number at the focus setting, `N · (1 + m / m_p)`.
    #[must_use]
    pub fn working_f_number(&self) -> f64 {
        self.lens.f_number * (1.0 + self.magnification() / self.pupil_magnification)
    }

    /// The optical points on the axis, in the mechanical frame.
    #[must_use]
    pub fn cardinal_points(&self) -> &CardinalPoints {
        &self.points
    }

    /// The sensor plane in the mechanical frame.
    #[must_use]
    pub fn sensor_plane(&self) -> &SensorPlane {
        &self.sensor
    }

    /// The principal point `[cx, cy]` (px): where the lens axis meets the sensor.
    #[must_use]
    pub fn principal_point(&self) -> [f64; 2] {
        self.principal_point
    }

    /// The projection frame P (origin at the entrance pupil) in the mechanical frame.
    #[must_use]
    pub fn mech_se3_projection(&self) -> &Isometry3<f64> {
        &self.mech_se3_projection
    }

    /// The equivalent calibration-rs pinhole about P: what a calibration of this camera recovers.
    #[must_use]
    pub fn pinhole(&self) -> &CameraParams {
        &self.pinhole
    }

    /// The pixel of a point given in the mechanical frame; `None` behind the entrance pupil.
    #[must_use]
    pub fn project(&self, point: &Point3<f64>) -> Option<Point2<f64>> {
        self.model
            .project_point(&self.mech_se3_projection.inverse_transform_point(point))
    }

    /// The ray of a pixel in the mechanical frame: its origin (the entrance pupil) and a
    /// direction with unit `z` in P.
    #[must_use]
    pub fn backproject(&self, pixel: &Point2<f64>) -> (Point3<f64>, Vector3<f64>) {
        let ray = self.model.backproject_pixel(pixel);
        (
            self.mech_se3_projection.translation.vector.into(),
            self.mech_se3_projection.rotation * ray.point,
        )
    }

    /// The paraxial (Gaussian) image of a point given in the mechanical frame.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] for a point in the front focal plane (its image is at infinity).
    pub fn image_of(&self, point: &Point3<f64>) -> Result<Point3<f64>> {
        let f = self.lens.focal_length_m;
        let d = point.z - self.points.front_principal;
        if (d - f).abs() < 1e-15 {
            return Err(invalid(
                "a point in the front focal plane images at infinity",
            ));
        }
        let s = f * d / (d - f);
        // `−s / d`, written so that a point on H (d = 0) has its unit magnification.
        let m = f / (f - d);
        Ok(Point3::new(
            m * point.x,
            m * point.y,
            self.points.rear_principal - s,
        ))
    }

    /// The object whose paraxial image is `image` (both in the mechanical frame).
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] for an image point in the rear focal plane.
    pub fn object_of(&self, image: &Point3<f64>) -> Result<Point3<f64>> {
        let f = self.lens.focal_length_m;
        let s = self.points.rear_principal - image.z;
        if (s - f).abs() < 1e-15 {
            return Err(invalid(
                "an image point in the rear focal plane comes from infinity",
            ));
        }
        let d = f * s / (s - f);
        // `−d / s`, written so that a point on H′ (s = 0) has its unit magnification.
        let m = f / (f - s);
        Ok(Point3::new(
            m * image.x,
            m * image.y,
            self.points.front_principal + d,
        ))
    }

    /// The geometric circle-of-confusion diameter at the sensor (m) of a point in the mechanical
    /// frame: the exit pupil projected from the point's image onto the sensor, at the sensor's
    /// axial position at the image's lateral position.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] for a point at or inside the front focal plane (no real image).
    pub fn coc_m(&self, point: &Point3<f64>) -> Result<f64> {
        let f = self.lens.focal_length_m;
        if (point.z - self.points.front_principal).partial_cmp(&f)
            != Some(std::cmp::Ordering::Greater)
        {
            return Err(invalid(format!(
                "a point at z = {} m forms no real image (front focal plane at {} m)",
                point.z, self.points.front_focal
            )));
        }
        let image = self.image_of(point)?;
        let n = &self.sensor.normal;
        let sensor_z = -(n.x * image.x + n.y * image.y) / n.z;
        Ok(self.exit_pupil_diameter_m() * (image.z - sensor_z).abs()
            / (self.points.exit_pupil - image.z))
    }

    /// [`PhysicalCamera::coc_m`] in pixels.
    ///
    /// # Errors
    ///
    /// As [`PhysicalCamera::coc_m`].
    pub fn coc_px(&self, point: &Point3<f64>) -> Result<f64> {
        Ok(self.coc_m(point)? / self.pixel_pitch_m)
    }

    /// The plane of best focus in the mechanical frame: a point on it (the focus point on the
    /// axis) and its unit normal (toward `+z`). It is the conjugate of the sensor plane, so a
    /// point on it has zero blur.
    ///
    /// # Errors
    ///
    /// Propagates [`PhysicalCamera::object_of`] (a sensor tilted through the rear focal plane).
    pub fn plane_of_best_focus(&self) -> Result<(Point3<f64>, Vector3<f64>)> {
        let step = 1e-3;
        let s = &self.sensor;
        let p0 = self.object_of(&Point3::origin())?;
        let px = self.object_of(&Point3::from(step * s.x_axis))?;
        let py = self.object_of(&Point3::from(step * s.y_axis))?;
        let normal = (px - p0).cross(&(py - p0)).normalize();
        Ok((p0, if normal.z < 0.0 { -normal } else { normal }))
    }

    /// The axial depth of field for a blur limit `coc_limit_m` (m, at the sensor), as `z` in
    /// the mechanical frame, and the hyperfocal focus distance from the datum.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] for a non-positive limit, or one at least the exit pupil (every
    /// depth would pass).
    pub fn depth_of_field(&self, coc_limit_m: f64) -> Result<DepthOfField> {
        let c = coc_limit_m;
        let dx = self.exit_pupil_diameter_m();
        if !(c > 0.0 && c < dx) {
            return Err(invalid(format!(
                "the blur limit must be positive and below the exit pupil diameter {dx} m, got {c}"
            )));
        }
        let f = self.lens.focal_length_m;
        let v0 = self.v0;
        // Distances behind H′: the exit pupil, and the image of each limit.
        let s_exit = self.points.rear_principal - self.points.exit_pupil;
        let object_z = |s: f64| self.points.front_principal + f * s / (s - f);
        let near = object_z((dx * v0 - c * s_exit) / (dx - c));
        let far_image = (dx * v0 + c * s_exit) / (dx + c);
        let far = (far_image > f).then(|| object_z(far_image));
        // The focus at which the far limit's image reaches the rear focal point.
        let v_h = (f * (dx + c) - c * s_exit) / dx;
        let hyperfocal = v_h + self.lens.principal_gap_m + f * v_h / (v_h - f);
        Ok(DepthOfField {
            near,
            far,
            hyperfocal,
        })
    }

    /// The diffraction blur: the Airy disc's diameter to its first zero, `2.44 · λ · N_w` (m).
    #[must_use]
    pub fn airy_diameter_m(&self, wavelength_m: f64) -> f64 {
        2.44 * wavelength_m * self.working_f_number()
    }

    /// The size on an object of one pixel at the image centre, at depth `z` in the mechanical
    /// frame (m per px), from the derived pinhole.
    #[must_use]
    pub fn object_pixel_m(&self, z: f64) -> f64 {
        let IntrinsicsParams::FxFyCxCySkew { params } = &self.pinhole.intrinsics;
        (z - self.points.entrance_pupil) / params.fx
    }
}
