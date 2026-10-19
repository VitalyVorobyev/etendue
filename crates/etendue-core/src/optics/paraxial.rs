//! Paraxial ray tracing through a [`PhysicalCamera`], in its mechanical frame.
//!
//! This is the reference that the camera's closed forms are tested against, and the source of
//! the ray fans the studio draws. It uses nothing but Gaussian conjugates
//! ([`PhysicalCamera::image_of`]) and line–plane intersections:
//! - an object-space ray through a point of the entrance pupil leaves the lens through that
//!   point's image on the exit pupil;
//! - every ray from an object point passes through the point's image.
//!
//! The exit pupil and its magnification are therefore found by imaging the entrance pupil, not
//! taken from the camera's closed forms.

use nalgebra::{Point2, Point3, Vector3};

use crate::optics::camera::PhysicalCamera;
use crate::{Error, Result};

/// One ray from an object point to the sensor, through a point of the pupils.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TracedRay {
    /// The object point.
    pub object: Point3<f64>,
    /// Where the ray crosses the entrance pupil plane.
    pub entrance: Point3<f64>,
    /// Where it crosses the exit pupil plane: the image of `entrance`.
    pub exit: Point3<f64>,
    /// The object point's image, which every ray from it passes through.
    pub image: Point3<f64>,
    /// Where the ray meets the sensor.
    pub on_sensor: Point3<f64>,
}

fn on_sensor(
    camera: &PhysicalCamera,
    from: &Point3<f64>,
    through: &Point3<f64>,
) -> Result<Point3<f64>> {
    let n = &camera.sensor_plane().normal;
    let dir: Vector3<f64> = through - from;
    let denom = n.dot(&dir);
    if denom.abs() < 1e-300 {
        return Err(Error::Numerical("a ray parallel to the sensor".into()));
    }
    Ok(from + dir * (-n.dot(&from.coords) / denom))
}

/// The ray from `object` through the point `pupil` of the entrance pupil plane (given as an
/// offset from the pupil's centre, in the plane).
///
/// # Errors
///
/// For an object at or inside the front focal plane, or a ray parallel to the sensor.
pub fn ray_through(
    camera: &PhysicalCamera,
    object: &Point3<f64>,
    pupil: [f64; 2],
) -> Result<TracedRay> {
    let z_e = camera.cardinal_points().entrance_pupil;
    let entrance = Point3::new(pupil[0], pupil[1], z_e);
    let exit = camera.image_of(&entrance)?;
    let image = camera.image_of(object)?;
    if (image.z - exit.z).abs() < 1e-300 {
        return Err(Error::Numerical(
            "the object images onto the exit pupil".into(),
        ));
    }
    Ok(TracedRay {
        object: *object,
        entrance,
        exit,
        image,
        on_sensor: on_sensor(camera, &exit, &image)?,
    })
}

/// The chief ray of `object`: through the centre of the pupils.
///
/// # Errors
///
/// As [`ray_through`].
pub fn chief_ray(camera: &PhysicalCamera, object: &Point3<f64>) -> Result<TracedRay> {
    ray_through(camera, object, [0.0, 0.0])
}

/// `count` rays from `object` through the rim of the entrance pupil, evenly spaced in angle:
/// where they meet the sensor outlines the blur.
///
/// # Errors
///
/// As [`ray_through`].
pub fn rim_rays(
    camera: &PhysicalCamera,
    object: &Point3<f64>,
    count: usize,
) -> Result<Vec<TracedRay>> {
    let r = camera.entrance_pupil_diameter_m() / 2.0;
    (0..count)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / count as f64;
            ray_through(camera, object, [r * libm::cos(a), r * libm::sin(a)])
        })
        .collect()
}

/// The pixel of a point on the sensor plane. The image is inverted on the sensor and read out
/// upright: pixel `u` grows against the sensor's physical `x`.
#[must_use]
pub fn sensor_pixel(camera: &PhysicalCamera, point: &Point3<f64>) -> Point2<f64> {
    let s = camera.sensor_plane();
    let [cx, cy] = camera.principal_point();
    let pitch = camera.pixel_pitch_m();
    Point2::new(
        cx - point.coords.dot(&s.x_axis) / pitch,
        cy - point.coords.dot(&s.y_axis) / pitch,
    )
}
