//! The bridge from scene documents to the camera model (ADR 0008).
//!
//! A scene places a camera that has [`OpticsSpec`] by its body: its frame is the mechanical
//! frame M. Everything that projects (ground truth, datasets, render jobs, sensor images) needs
//! the projection frame P at the entrance pupil, and the equivalent pinhole about it.
//! [`resolve`] produces that view of a scene once, so those consumers never meet the optics:
//! - each camera with optics is moved to P and given the derived pinhole;
//! - whatever hangs on it is re-expressed in P, so its world pose does not change.

use std::collections::BTreeMap;

use etendue_scene::{CameraSpec, Issue, OpticsSpec, SceneSpec, ValidationError};
use nalgebra::Isometry3;
use vision_calibration_core::{CameraParams, IntrinsicsParams, SensorParams};

use crate::optics::camera::{Lens, PhysicalCamera};

/// How far (relative) a camera's stored focal lengths and skew may be from the ones its optics
/// give, and (in radians) its stored sensor tilt from the derived one.
pub const DERIVED_TOLERANCE: f64 = 1e-9;

/// The physical camera of `optics`, with the principal point and distortion of `as_built`.
///
/// # Errors
///
/// As [`PhysicalCamera::new`].
pub fn physical_camera(
    optics: &OpticsSpec,
    as_built: &CameraParams,
) -> crate::Result<PhysicalCamera> {
    let lens = &optics.lens;
    PhysicalCamera::new(
        Lens {
            focal_length_m: lens.focal_length_m,
            f_number: lens.f_number,
            principal_gap_m: lens.principal_gap_m,
            entrance_pupil_m: lens.entrance_pupil_m,
            focusing: lens.focusing,
        },
        optics.sensor.pixel_pitch_m,
        optics.focus_distance_m,
        optics.tilt_rad,
        as_built,
    )
}

/// What `optics` derive for a camera built as `as_built`: the equivalent pinhole about P (to
/// store in the camera's `params`) and P in the camera's mechanical frame.
///
/// # Errors
///
/// As [`PhysicalCamera::new`].
pub fn derive(
    optics: &OpticsSpec,
    as_built: &CameraParams,
) -> crate::Result<(CameraParams, Isometry3<f64>)> {
    let camera = physical_camera(optics, as_built)?;
    Ok((camera.pinhole().clone(), *camera.mech_se3_projection()))
}

/// Whether `a` is within `tolerance` of `b` (never for a NaN).
fn close(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}

/// The ways stored `params` differ from the `derived` ones, as `(field, stored, derived)`.
fn mismatches(stored: &CameraParams, derived: &CameraParams) -> Vec<(&'static str, f64, f64)> {
    let mut out = Vec::new();
    let IntrinsicsParams::FxFyCxCySkew { params: s } = &stored.intrinsics;
    let IntrinsicsParams::FxFyCxCySkew { params: d } = &derived.intrinsics;
    for (field, a, b) in [("fx", s.fx, d.fx), ("fy", s.fy, d.fy)] {
        if !close(a, b, DERIVED_TOLERANCE * b.abs()) {
            out.push((field, a, b));
        }
    }
    if !close(s.skew, d.skew, DERIVED_TOLERANCE * d.fx.abs()) {
        out.push(("skew", s.skew, d.skew));
    }
    let tilt = |p: &CameraParams| match &p.sensor {
        SensorParams::Scheimpflug { params } => Some((params.tilt_x, params.tilt_y)),
        SensorParams::Identity => Some((0.0, 0.0)),
        SensorParams::Homography { .. } => None,
    };
    match (tilt(stored), tilt(derived)) {
        (Some(a), Some(b)) => {
            for (field, x, y) in [("tilt_x", a.0, b.0), ("tilt_y", a.1, b.1)] {
                if !close(x, y, DERIVED_TOLERANCE) {
                    out.push((field, x, y));
                }
            }
        }
        _ => out.push(("sensor", f64::NAN, f64::NAN)),
    }
    out
}

/// Check one camera's `params` against its `optics`, pushing issues under `path`; the derived
/// pinhole and P, when the optics are physical.
fn check_camera(
    optics: &OpticsSpec,
    params: &CameraParams,
    path: &str,
    issues: &mut Vec<Issue>,
) -> Option<(CameraParams, Isometry3<f64>)> {
    let derived = match derive(optics, params) {
        Ok(d) => d,
        Err(e) => {
            issues.push(Issue {
                path: format!("{path}.optics"),
                message: e.to_string(),
            });
            return None;
        }
    };
    for (field, stored, want) in mismatches(params, &derived.0) {
        let message = if field == "sensor" {
            "a camera with optics needs an identity or Scheimpflug sensor model".to_owned()
        } else {
            format!("is {stored}, but the optics give {want}: derive the params from the optics")
        };
        let at = match field {
            "tilt_x" | "tilt_y" | "sensor" => format!("{path}.params.sensor"),
            _ => format!("{path}.params.intrinsics.{field}"),
        };
        issues.push(Issue { path: at, message });
    }
    Some(derived)
}

/// The problems with a camera design (or a scene camera) whose `params` must hold what its
/// `optics` derive, reported under `path`: optics that are not physical, or stored focal
/// lengths, skew or tilt beyond [`DERIVED_TOLERANCE`].
#[must_use]
pub fn check_derived(optics: &OpticsSpec, params: &CameraParams, path: &str) -> Vec<Issue> {
    let mut issues = Vec::new();
    check_camera(optics, params, path, &mut issues);
    issues
}

/// A scene in projection frames, with where each moved camera's projection frame sits in its
/// mechanical frame.
#[derive(Clone, Debug)]
pub struct ResolvedScene {
    /// The scene, with every camera that has optics at its projection frame P, its `params` the
    /// derived pinhole and its `optics` removed. Everything attached to such a camera is
    /// re-expressed so that its world pose is unchanged.
    pub scene: SceneSpec,
    /// `M_se3_P` of every camera that had optics, by id.
    pub mech_se3_projection: BTreeMap<String, Isometry3<f64>>,
}

/// The scene as its projecting consumers need it. See [`ResolvedScene`].
///
/// Resolving a resolved scene changes nothing: it has no optics left.
///
/// # Errors
///
/// A [`ValidationError`] listing every camera (and rig template) whose optics are not physical
/// or whose stored `params` differ from what its optics give, beyond [`DERIVED_TOLERANCE`].
pub fn resolve(scene: &SceneSpec) -> Result<ResolvedScene, ValidationError> {
    let mut issues = Vec::new();
    for (i, rig) in scene.rigs.iter().enumerate() {
        if let Some(layout) = &rig.layout
            && let Some(optics) = &layout.camera.optics
        {
            check_camera(
                optics,
                &layout.camera.params,
                &format!("rigs[{i}].layout.camera"),
                &mut issues,
            );
        }
    }
    let mut out = scene.clone();
    let mut offsets = BTreeMap::new();
    for (i, camera) in scene.cameras.iter().enumerate() {
        let CameraSpec {
            optics: Some(optics),
            params,
            ..
        } = camera
        else {
            continue;
        };
        if let Some((pinhole, mech_se3_projection)) =
            check_camera(optics, params, &format!("cameras[{i}]"), &mut issues)
        {
            let c = &mut out.cameras[i];
            c.params = pinhole;
            c.optics = None;
            c.parent_se3_self *= mech_se3_projection;
            offsets.insert(camera.id.clone(), mech_se3_projection);
        }
    }
    if !issues.is_empty() {
        return Err(ValidationError { issues });
    }
    // Re-express what hangs on a moved camera in its projection frame.
    let reexpress = |parent: &str, pose: &mut Isometry3<f64>| {
        if let Some(m_se3_p) = offsets.get(parent) {
            *pose = m_se3_p.inverse() * *pose;
        }
    };
    for f in &mut out.frames {
        reexpress(f.parent.as_str(), &mut f.parent_se3_self);
    }
    for r in &mut out.robots {
        reexpress(r.parent.as_str(), &mut r.parent_se3_self);
    }
    for r in &mut out.rigs {
        reexpress(r.parent.as_str(), &mut r.parent_se3_self);
    }
    for c in &mut out.cameras {
        reexpress(c.parent.as_str(), &mut c.parent_se3_self);
    }
    for l in &mut out.lasers {
        reexpress(l.parent.as_str(), &mut l.parent_se3_self);
    }
    for l in &mut out.lights {
        reexpress(l.parent.as_str(), &mut l.parent_se3_self);
    }
    for t in &mut out.targets {
        reexpress(t.parent.as_str(), &mut t.parent_se3_self);
    }
    for p in &mut out.parts {
        reexpress(p.parent.as_str(), &mut p.parent_se3_self);
    }
    Ok(ResolvedScene {
        scene: out,
        mech_se3_projection: offsets,
    })
}
