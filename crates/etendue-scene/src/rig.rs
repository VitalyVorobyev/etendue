//! Symmetric rig layouts (P7-3): mono, stereo, and rings of cameras around
//! the rig's +Z axis, each optionally paired with its own line laser.
//!
//! A [`RigLayout`] is kept on its [`RigSpec`](crate::RigSpec) so the layout
//! can be edited again; the cameras and lasers it expands to
//! ([`RigLayout::expand`]) are ordinary scene entities and stay the source
//! of truth for everything else.
//!
//! # Geometry (rig frame)
//!
//! Camera `i` of `count` sits at azimuth `φᵢ = phase + 2πi/count`, at
//! `radius` from the rig axis in the `z = 0` plane. Its optical axis (+Z,
//! OpenCV) is tilted by `tilt` from the rig's +Z towards the axis, in the
//! radial plane through the camera; by default `tilt = atan(radius /
//! working_distance)`, so every axis meets the rig axis at the working point
//! `(0, 0, working_distance)`. The image +X is the tangential direction
//! (`image_x = tangential`) or the outward radial one (`radial`).
//!
//! Laser `i` sits at azimuth `φᵢ + azimuth` and `radius` from the axis, aims
//! its central ray (+Z) at the working point, and spreads its fan (the local
//! `x = 0` plane) along camera `i`'s tangential direction: on a target at
//! the working distance its line runs along the camera's image +X (with
//! `image_x = tangential`), and the triangulation angle is the angle between
//! the camera's axis and the fan.

use nalgebra::{Isometry3, Point3, Rotation3, Translation3, UnitQuaternion, Vector3};
use serde::{Deserialize, Serialize};
use vision_calibration_core::CameraParams;

use crate::frame::FrameRef;
use crate::optics::OpticsSpec;
use crate::validate::Issues;
use crate::{CameraSpec, LaserSpec};

/// How a rig's cameras (and lasers) are laid out.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct RigLayout {
    /// Number of cameras: 1 (mono), 2 (stereo), 3, 6, …
    pub count: u32,
    /// Distance of each camera from the rig axis, metres (`0` only for one
    /// camera).
    pub radius_m: f64,
    /// Azimuth of camera 0 about the rig axis, radians.
    #[serde(default)]
    pub phase_rad: f64,
    /// Where the camera axes meet the rig axis (with the default tilt), and
    /// where the lasers aim: `(0, 0, working_distance_m)` in the rig frame.
    pub working_distance_m: f64,
    /// Angle between each camera's axis and the rig axis, radians; default
    /// `atan(radius / working_distance)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tilt_rad: Option<f64>,
    /// Which way each camera's image +X points.
    #[serde(default)]
    pub image_x: ImageX,
    /// The camera every position gets.
    pub camera: CameraTemplate,
    /// One laser per camera, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lasers: Option<LaserLayout>,
}

/// The direction of a ring camera's image +X.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ImageX {
    /// Along the ring (counter-clockwise seen from +Z).
    #[default]
    Tangential,
    /// Away from the rig axis.
    Radial,
    /// The rig's +X, as each camera sees it: classical stereo. For two cameras on the rig's x
    /// axis the image rows line up (the image +Y axes are the same); with no tilt (a parallel
    /// rig) the image +X axes are the same too.
    Common,
}

/// The camera model of every position of a layout.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CameraTemplate {
    /// Projection, distortion, sensor and intrinsics.
    pub params: CameraParams,
    /// Image size in pixels, `[width, height]`.
    pub resolution: [u32; 2],
    /// The cameras' physical optics, if they have them (ADR 0008).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optics: Option<OpticsSpec>,
}

/// One line laser per camera.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct LaserLayout {
    /// Distance of each laser from the rig axis, metres (unused with `camera_se3_laser`).
    #[serde(default)]
    pub radius_m: f64,
    /// Azimuth of laser `i` relative to camera `i`, radians (unused with `camera_se3_laser`).
    #[serde(default)]
    pub azimuth_rad: f64,
    /// Each laser fixed to its camera's body at this pose, as a head carries it; otherwise it
    /// stands on the ring, aimed at the working point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(
        feature = "schemars",
        schemars(with = "Option<vision_calibration_core::Iso3Schema>")
    )]
    pub camera_se3_laser: Option<Isometry3<f64>>,
    /// Fan half-angle, radians, in `(0, π/2)`.
    pub fan_half_angle: f64,
    /// Fan reach along its central ray, metres.
    pub fan_length: f64,
    /// Wavelength, nanometres.
    pub wavelength_nm: f64,
    /// Beam-waist `1/e²` radius, metres.
    pub beam_waist_m: f64,
    /// Distance from the laser origin to the waist along each ray, metres:
    /// where a focusable laser is focused. Absent, the waist is at the laser.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waist_distance_m: Option<f64>,
    /// Optical power, watts (`LaserSpec::power_w`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power_w: Option<f64>,
}

/// Rotation whose columns are the given frame axes (all unit, right-handed).
fn frame(x: Vector3<f64>, y: Vector3<f64>, z: Vector3<f64>) -> UnitQuaternion<f64> {
    UnitQuaternion::from_rotation_matrix(&Rotation3::from_basis_unchecked(&[x, y, z]))
}

impl RigLayout {
    /// The camera axes' angle to the rig axis.
    #[must_use]
    pub fn tilt(&self) -> f64 {
        self.tilt_rad
            .unwrap_or_else(|| self.radius_m.atan2(self.working_distance_m))
    }

    /// Structural checks.
    ///
    /// # Errors
    ///
    /// Every problem found.
    pub fn validate(&self) -> Result<(), crate::ValidationError> {
        let mut issues = Issues::default();
        self.check(&mut issues, "layout");
        issues.finish()
    }

    /// Structural checks, reported under `path`.
    pub(crate) fn check(&self, issues: &mut Issues, path: &str) {
        if self.count == 0 {
            issues.push(format!("{path}.count"), "must be at least 1");
        }
        if !(self.radius_m.is_finite() && self.radius_m >= 0.0) {
            issues.push(format!("{path}.radius_m"), "must be finite and ≥ 0");
        } else if self.count > 1 && self.radius_m == 0.0 {
            issues.push(
                format!("{path}.radius_m"),
                "must be > 0 for more than one camera",
            );
        }
        if !self.phase_rad.is_finite() {
            issues.push(format!("{path}.phase_rad"), "must be finite");
        }
        issues.check_positive(
            &format!("{path}.working_distance_m"),
            self.working_distance_m,
        );
        if let Some(t) = self.tilt_rad
            && !(t.is_finite() && (0.0..std::f64::consts::FRAC_PI_2).contains(&t))
        {
            issues.push(format!("{path}.tilt_rad"), "must be in [0, π/2)");
        }
        if let Some(optics) = &self.camera.optics {
            optics.check(issues, &format!("{path}.camera.optics"));
        }
        if let Some(l) = &self.lasers {
            if let Some(pose) = &l.camera_se3_laser {
                issues.check_pose(&format!("{path}.lasers.camera_se3_laser"), pose);
            }
            if !(l.radius_m.is_finite() && l.radius_m >= 0.0) {
                issues.push(format!("{path}.lasers.radius_m"), "must be finite and ≥ 0");
            }
            if !l.azimuth_rad.is_finite() {
                issues.push(format!("{path}.lasers.azimuth_rad"), "must be finite");
            }
        }
    }

    /// The cameras (`<rig>_cam<i>`) and lasers (`<rig>_laser<i>`, each
    /// linked to its camera) of this layout, attached to `rig`.
    #[must_use]
    pub fn expand(&self, rig: &str) -> (Vec<CameraSpec>, Vec<LaserSpec>) {
        let n = self.count.max(1);
        let tilt = self.tilt();
        let target = Point3::new(0.0, 0.0, self.working_distance_m);
        let mut cameras = Vec::with_capacity(n as usize);
        let mut lasers = Vec::new();
        for i in 0..n {
            let phi = self.phase_rad + std::f64::consts::TAU * f64::from(i) / f64::from(n);
            let radial = Vector3::new(phi.cos(), phi.sin(), 0.0);
            let tangential = Vector3::new(-phi.sin(), phi.cos(), 0.0);
            let position = radial * self.radius_m;
            // Tilted from +Z towards the axis, in the radial plane.
            let z = Vector3::z() * tilt.cos() - radial * tilt.sin();
            // The radial direction made perpendicular to the optical axis.
            let outward = (radial * tilt.cos() + Vector3::z() * tilt.sin()).normalize();
            let along = match self.image_x {
                ImageX::Tangential => tangential,
                ImageX::Radial => outward,
                // The rig's +X projected onto the image plane; about the axis (a camera looking
                // along the rig's x) the tangential direction stands in.
                ImageX::Common => {
                    let x = Vector3::x() - z * z.x;
                    if x.norm() < 1e-9 {
                        tangential
                    } else {
                        x.normalize()
                    }
                }
            };
            let (x, y) = (along, z.cross(&along));
            let camera = format!("{rig}_cam{i}");
            cameras.push(CameraSpec {
                id: camera.clone(),
                parent: FrameRef::from(rig),
                parent_se3_self: Isometry3::from_parts(
                    Translation3::from(position),
                    frame(x, y, z),
                ),
                params: self.camera.params.clone(),
                resolution: self.camera.resolution,
                optics: self.camera.optics.clone(),
            });
            if let Some(l) = &self.lasers
                && let Some(camera_se3_laser) = &l.camera_se3_laser
            {
                lasers.push(LaserSpec {
                    id: format!("{rig}_laser{i}"),
                    parent: FrameRef::from(camera.as_str()),
                    parent_se3_self: *camera_se3_laser,
                    fan_half_angle: l.fan_half_angle,
                    fan_length: l.fan_length,
                    wavelength_nm: l.wavelength_nm,
                    beam_waist_m: l.beam_waist_m,
                    waist_distance_m: l.waist_distance_m,
                    power_w: l.power_w,
                    camera: Some(camera),
                });
            } else if let Some(l) = &self.lasers {
                let psi = phi + l.azimuth_rad;
                let at = Vector3::new(psi.cos(), psi.sin(), 0.0) * l.radius_m;
                let ray = (target.coords - at).normalize();
                // The fan spreads along the camera's tangential direction.
                let along = (tangential - ray * ray.dot(&tangential)).normalize();
                lasers.push(LaserSpec {
                    id: format!("{rig}_laser{i}"),
                    parent: FrameRef::from(rig),
                    parent_se3_self: Isometry3::from_parts(
                        Translation3::from(at),
                        frame(along.cross(&ray), along, ray),
                    ),
                    fan_half_angle: l.fan_half_angle,
                    fan_length: l.fan_length,
                    wavelength_nm: l.wavelength_nm,
                    beam_waist_m: l.beam_waist_m,
                    waist_distance_m: l.waist_distance_m,
                    power_w: l.power_w,
                    camera: Some(camera),
                });
            }
        }
        (cameras, lasers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(count: u32, lasers: bool) -> RigLayout {
        serde_json::from_value(serde_json::json!({
            "count": count,
            "radius_m": if count == 1 { 0.0 } else { 0.08 },
            "working_distance_m": 0.4,
            "camera": {
                "params": {
                    "projection": {"type": "pinhole"}, "distortion": {"type": "none"},
                    "sensor": {"type": "identity"},
                    "intrinsics": {"type": "fx_fy_cx_cy_skew", "fx": 2000.0, "fy": 2000.0,
                                   "cx": 640.0, "cy": 512.0, "skew": 0.0}
                },
                "resolution": [1280, 1024]
            },
            "lasers": if lasers { serde_json::json!({
                "radius_m": 0.03, "azimuth_rad": 0.4, "fan_half_angle": 0.35, "fan_length": 1.0,
                "wavelength_nm": 660.0, "beam_waist_m": 0.0005
            }) } else { serde_json::Value::Null },
        }))
        .unwrap()
    }

    #[test]
    fn ring_cameras_converge_on_the_working_point() {
        for count in [1, 2, 3, 6] {
            let l = layout(count, false);
            let (cameras, lasers) = l.expand("rig");
            assert_eq!(cameras.len(), count as usize);
            assert!(lasers.is_empty());
            let target = Point3::new(0.0, 0.0, 0.4);
            for c in &cameras {
                // The working point is on the optical axis.
                let p = c.parent_se3_self.inverse() * target;
                assert!(p.x.abs() < 1e-12 && p.y.abs() < 1e-12 && p.z > 0.0, "{p}");
                let axis = c.parent_se3_self.rotation * Vector3::z();
                assert!((axis.angle(&Vector3::z()) - l.tilt()).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn ring_cameras_are_rotations_of_each_other() {
        let (cameras, _) = layout(6, false).expand("rig");
        let step = Isometry3::rotation(Vector3::z() * std::f64::consts::TAU / 6.0);
        for pair in cameras.windows(2) {
            let expected = step * pair[0].parent_se3_self;
            let got = pair[1].parent_se3_self;
            assert!((expected.translation.vector - got.translation.vector).norm() < 1e-12);
            assert!(expected.rotation.angle_to(&got.rotation) < 1e-12);
        }
        assert_eq!(cameras[5].id, "rig_cam5");
    }

    #[test]
    fn lasers_aim_at_the_working_point_across_their_camera() {
        let (cameras, lasers) = layout(3, true).expand("head");
        assert_eq!(lasers.len(), 3);
        for (c, l) in cameras.iter().zip(&lasers) {
            assert_eq!(l.camera.as_deref(), Some(c.id.as_str()));
            let target = l.parent_se3_self.inverse() * Point3::new(0.0, 0.0, 0.4);
            assert!(target.x.abs() < 1e-12 && target.y.abs() < 1e-12 && target.z > 0.0);
            // The fan (local x = 0) contains the camera's image +X direction.
            let normal = l.parent_se3_self.rotation * Vector3::x();
            let image_x = c.parent_se3_self.rotation * Vector3::x();
            assert!(
                normal.dot(&image_x).abs() < 1e-12,
                "{}",
                normal.dot(&image_x)
            );
            // The camera is off the laser plane: it can triangulate.
            let camera_from_plane = normal.dot(
                &(c.parent_se3_self.translation.vector - l.parent_se3_self.translation.vector),
            );
            assert!(camera_from_plane.abs() > 0.01);
        }
    }

    #[test]
    fn invalid_layouts_are_reported() {
        let mut l = layout(3, false);
        l.radius_m = 0.0;
        l.tilt_rad = Some(2.0);
        let mut issues = Issues::default();
        l.check(&mut issues, "rigs[0].layout");
        let paths: Vec<String> = issues
            .finish()
            .unwrap_err()
            .issues
            .into_iter()
            .map(|i| i.path)
            .collect();
        assert_eq!(
            paths,
            ["rigs[0].layout.radius_m", "rigs[0].layout.tilt_rad"]
        );
    }
}

#[cfg(test)]
mod stereo_tests {
    use super::*;

    fn stereo(tilt: Option<f64>, lasers: Option<serde_json::Value>) -> RigLayout {
        let mut v = serde_json::json!({
            "count": 2,
            "radius_m": 0.06,
            "working_distance_m": 0.5,
            "image_x": "common",
            "camera": {
                "params": {
                    "projection": {"type": "pinhole"}, "distortion": {"type": "none"},
                    "sensor": {"type": "identity"},
                    "intrinsics": {"type": "fx_fy_cx_cy_skew", "fx": 2000.0, "fy": 2000.0,
                                   "cx": 640.0, "cy": 512.0, "skew": 0.0}
                },
                "resolution": [1280, 1024]
            }
        });
        if let Some(t) = tilt {
            v["tilt_rad"] = serde_json::json!(t);
        }
        if let Some(l) = lasers {
            v["lasers"] = l;
        }
        serde_json::from_value(v).unwrap()
    }

    fn axes(c: &CameraSpec) -> (Vector3<f64>, Vector3<f64>) {
        let r = c.parent_se3_self.rotation;
        (r * Vector3::x(), r * Vector3::y())
    }

    #[test]
    fn two_cameras_with_common_image_x_are_a_stereo_pair() {
        // G11.1: converged (toe-in) and parallel.
        for tilt in [None, Some(0.0)] {
            let (cams, _) = stereo(tilt, None).expand("rig");
            let baseline = cams[0].parent_se3_self.translation.vector
                - cams[1].parent_se3_self.translation.vector;
            assert!((baseline.norm() - 0.12).abs() < 1e-12);
            assert!(
                baseline.y.abs() < 1e-12 && baseline.z.abs() < 1e-12,
                "the baseline is the rig's x"
            );
            let ((x0, y0), (x1, y1)) = (axes(&cams[0]), axes(&cams[1]));
            // Rows line up: the image +Y axes are the same.
            assert!((y0 - y1).norm() < 1e-12, "{y0} vs {y1}");
            // Image +X lies in the baseline plane (the rig's x–z plane), pointing along +X.
            assert!(x0.y.abs() < 1e-12 && x1.y.abs() < 1e-12 && x0.x > 0.0 && x1.x > 0.0);
            if tilt == Some(0.0) {
                assert!(
                    (x0 - x1).norm() < 1e-12,
                    "a parallel rig's image +X axes are the same"
                );
            }
        }
    }

    #[test]
    fn a_head_unit_carries_its_laser_on_each_camera() {
        let pose = Isometry3::from_parts(
            Translation3::new(0.0, -0.1, 0.05),
            UnitQuaternion::from_euler_angles(0.3, 0.0, 0.0),
        );
        let layout = stereo(
            None,
            Some(serde_json::json!({
                "camera_se3_laser": pose,
                "fan_half_angle": 0.3, "fan_length": 1.0, "wavelength_nm": 660.0, "beam_waist_m": 5e-4
            })),
        );
        layout.validate().unwrap();
        let (cams, lasers) = layout.expand("rig");
        for (c, l) in cams.iter().zip(&lasers) {
            assert_eq!(l.parent.as_str(), c.id);
            assert_eq!(l.camera.as_deref(), Some(c.id.as_str()));
            assert_eq!(l.parent_se3_self, pose);
        }
    }
}
