//! Laser-stripe ground truth (P5-3, [ADR 0006](https://github.com/VitalyVorobyev/etendue/blob/main/docs/adrs/0006-ground-truth.md)).
//!
//! Where a line laser's fan meets the board, it paints a straight stripe (on
//! a PuzzlePole, an ellipse). The stripe is computed geometrically by
//! `etendue-core` ([`etendue_core::laser::stripe_on_target`],
//! [`etendue_core::laser::stripe_on_cylinder`]): the fan plane intersected
//! with the target surface, clipped to the fan's angle and reach and to the
//! printed extent. Its points are expressed in the target frame, as
//! [`BoardPoint`]s, so they are projected by the same
//! [`project_points`](crate::gt::project_points) as the board's corners.
//!
//! The true laser plane is given in calibration-rs's optimizer form:
//! `n̂ · p + d = 0`, metres, in a named frame ([`GtPlane`]).

use etendue_core::LaserEntity;
use etendue_core::TargetEntity;
use etendue_core::laser::{
    GaussianBeamWidth, LaserPlane, WidthModel, stripe_on_cylinder, stripe_on_target,
};
use etendue_scene::LaserSpec;
use nalgebra::{Isometry3, Point3, Vector3};
use serde::{Deserialize, Serialize};

use crate::gt::BoardPoint;
use crate::{Error, Result};

/// How a [`GtPlane`] is parameterised.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaneConvention {
    /// `n̂ · p + d = 0`, with `n̂` a unit normal and `d` in metres: the
    /// calibration-rs optimizer's `LaserPlane`. The sign of `(n̂, d)` is
    /// arbitrary; `n̂` is the laser-local +X axis.
    UnitNormalOffsetM,
}

/// A laser plane in a named frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GtPlane {
    /// The frame `p` is expressed in (a camera or rig id).
    pub frame: String,
    /// The parameterisation.
    pub convention: PlaneConvention,
    /// Unit normal `n̂` in `frame`.
    pub normal: [f64; 3],
    /// Offset `d` in metres.
    pub distance: f64,
}

/// The fan plane of a laser in `frame`, given `frame_se3_laser`. The fan is
/// the laser-local `x = 0` plane, so `n̂` is the laser's +X axis and the plane
/// passes through the laser origin.
#[must_use]
pub fn plane_in(frame: &str, frame_se3_laser: &Isometry3<f64>) -> GtPlane {
    let n = frame_se3_laser.rotation * Vector3::x();
    let o = frame_se3_laser.translation.vector;
    GtPlane {
        frame: frame.to_owned(),
        convention: PlaneConvention::UnitNormalOffsetM,
        normal: [n.x, n.y, n.z],
        distance: -n.dot(&o),
    }
}

/// The printed surface a stripe lies on, in the target frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StripeSurface {
    /// A planar print of this extent (metres, centred, in the `z = 0`
    /// plane, facing +Z).
    Plane([f64; 2]),
    /// A PuzzlePole: a cylinder of `radius` (metres) about +Z, printed over
    /// `z ∈ [0, length]`.
    Cylinder {
        /// Radius, metres.
        radius: f64,
        /// Printed length along the axis, metres.
        length: f64,
    },
}

impl StripeSurface {
    /// The printed surface of a scene target, if it has one.
    #[must_use]
    pub fn of(geometry: &etendue_scene::TargetGeometry) -> Option<Self> {
        if let etendue_scene::TargetGeometry::Printed { target } = geometry
            && let Some((radius, length)) = target.cylinder()
        {
            return Some(Self::Cylinder { radius, length });
        }
        geometry.extent_m().map(Self::Plane)
    }
}

/// The stripe `laser` paints on a target's printed `surface`, as
/// target-frame points about `spacing_m` apart (on a plane both ends
/// included).
///
/// Empty when the laser misses the target, when its plane is parallel to a
/// planar target, or when the laser lights none of the printed side (the +Z
/// face of a plane, the outside of a cylinder).
///
/// # Errors
///
/// [`Error::InvalidInput`] if the laser, the extent or the spacing is out of
/// range.
pub fn stripe(
    laser: &LaserSpec,
    world_se3_laser: &Isometry3<f64>,
    world_se3_target: &Isometry3<f64>,
    surface: StripeSurface,
    spacing_m: f64,
) -> Result<Vec<BoardPoint>> {
    if !(spacing_m.is_finite() && spacing_m > 0.0) {
        return Err(Error::InvalidInput(format!(
            "laser stripe spacing must be positive, got {spacing_m}"
        )));
    }
    let core = |e: etendue_core::Error| Error::InvalidInput(format!("laser `{}`: {e}", laser.id));
    let entity = entity(laser, world_se3_laser)?;
    let fan = LaserPlane::from_entity(&entity);
    let target_se3_world = world_se3_target.inverse();
    let extent = match surface {
        StripeSurface::Plane(extent) => extent,
        StripeSurface::Cylinder { radius, length } => {
            let runs = stripe_on_cylinder(&fan, world_se3_target, radius, [0.0, length], spacing_m);
            if runs.iter().map(Vec::len).sum::<usize>() > MAX_SAMPLES {
                return Err(Error::InvalidInput(format!(
                    "laser `{}`: the stripe at {spacing_m} m spacing is more than {MAX_SAMPLES} points",
                    laser.id
                )));
            }
            return Ok(runs
                .into_iter()
                .flatten()
                .map(|w| {
                    let p = target_se3_world * w;
                    let theta = p.y.atan2(p.x).rem_euclid(std::f64::consts::TAU);
                    let normal = Vector3::new(p.x, p.y, 0.0).normalize();
                    BoardPoint {
                        // The unrolled (z, arc) position, arc from the seam.
                        position_m: [p.z, theta * radius],
                        xyz_m: Some([p.x, p.y, p.z]),
                        normal: Some([normal.x, normal.y, normal.z]),
                        grid: None,
                        id: None,
                    }
                })
                .collect());
        }
    };
    let target = TargetEntity::new(*world_se3_target, extent[0], extent[1]).map_err(core)?;
    if (target_se3_world * Point3::from(world_se3_laser.translation.vector)).z <= 0.0 {
        return Ok(Vec::new());
    }
    let width = GaussianBeamWidth::from_laser(&entity).map_err(core)?;
    // The clipped segment once; its points are evenly spaced along it.
    let Some(ends) = stripe_on_target(&fan, &target, &width, 2) else {
        return Ok(Vec::new());
    };
    let n = (ends.length_m() / spacing_m).ceil();
    if n > MAX_SAMPLES as f64 {
        return Err(Error::InvalidInput(format!(
            "laser `{}`: a {:.3} m stripe at {spacing_m} m spacing is more than {MAX_SAMPLES} points",
            laser.id,
            ends.length_m()
        )));
    }
    let n = n as usize + 1;
    let [a, b] = [ends.samples()[0].point, ends.samples()[1].point];
    let samples = (0..n)
        .map(|i| {
            let t = if n > 1 {
                i as f64 / (n - 1) as f64
            } else {
                0.0
            };
            let p: Point3<f64> = target_se3_world * (a + (b - a) * t);
            BoardPoint::planar([p.x, p.y], None, None)
        })
        .collect();
    Ok(samples)
}

/// The beam's `1/e²` diameter at each stripe point ([`GaussianBeamWidth`],
/// the waist at the laser's origin), metres. `points` are in the target
/// frame, as [`stripe`] gives them.
///
/// # Errors
///
/// [`Error::InvalidInput`] if the laser is out of range.
pub fn beam_widths(
    laser: &LaserSpec,
    world_se3_laser: &Isometry3<f64>,
    world_se3_target: &Isometry3<f64>,
    points: &[BoardPoint],
) -> Result<Vec<f64>> {
    let core = |e: etendue_core::Error| Error::InvalidInput(format!("laser `{}`: {e}", laser.id));
    let entity = entity(laser, world_se3_laser)?;
    let width = GaussianBeamWidth::from_laser(&entity).map_err(core)?;
    let laser_se3_target = world_se3_laser.inverse() * world_se3_target;
    Ok(points
        .iter()
        .map(|p| width.width_at((laser_se3_target * p.xyz()).coords.norm()))
        .collect())
}

/// The core laser entity of `laser` at `world_se3_laser`.
fn entity(laser: &LaserSpec, world_se3_laser: &Isometry3<f64>) -> Result<LaserEntity> {
    LaserEntity::new(
        *world_se3_laser,
        laser.fan_half_angle,
        laser.fan_length,
        laser.wavelength_nm,
        laser.beam_waist_m,
    )
    .map_err(|e| Error::InvalidInput(format!("laser `{}`: {e}", laser.id)))
}

/// The most points a stripe may have (a bound on `spacing_m`).
const MAX_SAMPLES: usize = 100_000;

#[cfg(test)]
mod tests {
    use super::*;
    use etendue_scene::FrameRef;
    use nalgebra::{Translation3, UnitQuaternion};

    fn laser(fan_half_angle: f64) -> LaserSpec {
        LaserSpec {
            id: "laser".into(),
            parent: FrameRef::world(),
            parent_se3_self: Isometry3::identity(),
            fan_half_angle,
            fan_length: 2.0,
            wavelength_nm: 660.0,
            beam_waist_m: 5e-4,
            power_w: None,
            camera: None,
        }
    }

    /// A laser 0.5 m above a board lying face-up at the origin, pointing
    /// down (+Z of the laser = −Z of the world), rotated so its fan is not
    /// aligned with the board axes, and tilted a little.
    fn geometry() -> (Isometry3<f64>, Isometry3<f64>) {
        let down = UnitQuaternion::from_axis_angle(&Vector3::x_axis(), std::f64::consts::PI);
        let turn = UnitQuaternion::from_axis_angle(&Vector3::z_axis(), 0.3)
            * UnitQuaternion::from_axis_angle(&Vector3::y_axis(), 0.05);
        let world_se3_laser =
            Isometry3::from_parts(Translation3::new(0.01, -0.02, 0.5), turn * down);
        let world_se3_target = Isometry3::from_parts(
            Translation3::new(0.0, 0.0, 0.0),
            UnitQuaternion::from_axis_angle(&Vector3::x_axis(), 0.1),
        );
        (world_se3_laser, world_se3_target)
    }

    #[test]
    fn a_too_fine_spacing_is_an_error() {
        let (world_se3_laser, world_se3_target) = geometry();
        let fine = stripe(
            &laser(0.6),
            &world_se3_laser,
            &world_se3_target,
            StripeSurface::Plane([0.2, 0.14]),
            1e-12,
        );
        assert!(matches!(fine, Err(Error::InvalidInput(_))), "{fine:?}");
    }

    #[test]
    fn stripe_points_lie_on_the_laser_plane_and_the_board() {
        let (world_se3_laser, world_se3_target) = geometry();
        let extent = [0.2, 0.14];
        let points = stripe(
            &laser(0.6),
            &world_se3_laser,
            &world_se3_target,
            StripeSurface::Plane(extent),
            1e-3,
        )
        .unwrap();
        assert!(points.len() > 100, "{} points", points.len());
        // The plane in the target frame, and in the world.
        let target_plane = plane_in("board", &(world_se3_target.inverse() * world_se3_laser));
        let world_plane = plane_in("world", &world_se3_laser);
        for bp in &points {
            let [x, y] = bp.position_m;
            assert!(x.abs() <= extent[0] / 2.0 + 1e-12 && y.abs() <= extent[1] / 2.0 + 1e-12);
            let p = Vector3::new(x, y, 0.0);
            let n = Vector3::from(target_plane.normal);
            assert!((n.dot(&p) + target_plane.distance).abs() < 1e-12);
            let w = world_se3_target * Point3::from(p);
            let n = Vector3::from(world_plane.normal);
            assert!((n.dot(&w.coords) + world_plane.distance).abs() < 1e-12);
        }
        // Evenly spaced, no gap wider than the requested spacing.
        for pair in points.windows(2) {
            let d = (pair[1].position_m[0] - pair[0].position_m[0])
                .hypot(pair[1].position_m[1] - pair[0].position_m[1]);
            assert!(d <= 1e-3 + 1e-12);
        }
    }

    #[test]
    fn the_fan_and_the_board_clip_the_stripe() {
        let (world_se3_laser, world_se3_target) = geometry();
        let wide = stripe(
            &laser(0.6),
            &world_se3_laser,
            &world_se3_target,
            StripeSurface::Plane([0.2, 0.14]),
            1e-3,
        )
        .unwrap();
        // A narrow fan reaches less of the board.
        let narrow = stripe(
            &laser(0.05),
            &world_se3_laser,
            &world_se3_target,
            StripeSurface::Plane([0.2, 0.14]),
            1e-3,
        )
        .unwrap();
        let length = |p: &[BoardPoint]| {
            let (a, b) = (p[0].position_m, p[p.len() - 1].position_m);
            (b[0] - a[0]).hypot(b[1] - a[1])
        };
        assert!(length(&narrow) < 0.06 && length(&narrow) > 0.04);
        assert!(length(&wide) > 0.14);
        // A board off to the side of the fan: no stripe.
        let aside = Isometry3::translation(1.0, 0.0, 0.0) * world_se3_target;
        assert!(
            stripe(
                &laser(0.6),
                &world_se3_laser,
                &aside,
                StripeSurface::Plane([0.2, 0.14]),
                1e-3
            )
            .unwrap()
            .is_empty()
        );
        // The laser behind the board lights nothing.
        let flipped = world_se3_target
            * Isometry3::from_parts(
                Translation3::identity(),
                UnitQuaternion::from_axis_angle(&Vector3::x_axis(), std::f64::consts::PI),
            );
        assert!(
            stripe(
                &laser(0.6),
                &world_se3_laser,
                &flipped,
                StripeSurface::Plane([0.2, 0.14]),
                1e-3
            )
            .unwrap()
            .is_empty()
        );
    }

    #[test]
    fn planes_move_with_their_frame() {
        let (world_se3_laser, _) = geometry();
        let world_se3_cam = Isometry3::from_parts(
            Translation3::new(0.1, 0.2, 0.3),
            UnitQuaternion::from_euler_angles(0.2, -0.1, 0.4),
        );
        let cam = plane_in("cam", &(world_se3_cam.inverse() * world_se3_laser));
        // The laser origin and a point along its central ray are on the plane
        // in the camera frame.
        for local in [Point3::origin(), Point3::new(0.0, 0.3, 0.7)] {
            let p = world_se3_cam.inverse() * (world_se3_laser * local);
            let n = Vector3::from(cam.normal);
            assert!((n.dot(&p.coords) + cam.distance).abs() < 1e-12);
        }
        assert!((Vector3::from(cam.normal).norm() - 1.0).abs() < 1e-15);
    }
}
