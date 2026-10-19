//! A camera's physical optics (ADR 0008): its sensor, its lens and where it is focused.
//!
//! A camera that carries an [`OpticsSpec`] is placed by its body: its frame is the mechanical
//! frame, with its origin at the image-plane datum (where the lens axis meets the image plane).
//! The optics decide where the lens's principal points and pupils sit on that frame's axis, and
//! from them the camera's focal lengths, skew and model tilt. Those stay in the camera's
//! `params`, as the equivalent calibration-rs pinhole about the entrance pupil, and must match
//! what the optics give: `etendue-core::from_scene` derives them and checks a scene against them.
//! This crate only checks that the numbers are physical.

use serde::{Deserialize, Serialize};

use crate::validate::Issues;

/// How a lens focuses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Focusing {
    /// The whole lens moves along its axis; its focal length does not change.
    #[default]
    Unit,
}

/// The image sensor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct SensorOptics {
    /// What the sensor is, e.g. `Sony IMX174`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Pixel pitch, metres.
    pub pixel_pitch_m: f64,
}

/// An ideal thick lens with its entrance pupil.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct LensOptics {
    /// What the lens is, e.g. `Kowa LM25HC`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Effective focal length, metres.
    pub focal_length_m: f64,
    /// f-number: the focal length over the entrance pupil diameter.
    pub f_number: f64,
    /// Signed distance from the rear principal point H′ to the front one H, metres (positive
    /// when H lies on the object side).
    #[serde(default)]
    pub principal_gap_m: f64,
    /// Signed distance from H to the entrance pupil, metres (positive toward the object).
    #[serde(default)]
    pub entrance_pupil_m: f64,
    /// How the lens focuses.
    #[serde(default)]
    pub focusing: Focusing,
}

/// A camera's physical optics.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpticsSpec {
    /// The sensor.
    pub sensor: SensorOptics,
    /// The lens.
    pub lens: LensOptics,
    /// Distance from the image-plane datum to the plane in focus, metres, as a lens's focus scale
    /// reads it.
    pub focus_distance_m: f64,
    /// The sensor's tilt `[τx, τy]` about the image-plane datum, radians, in calibration-rs's
    /// (OpenCV's) convention.
    #[serde(default, skip_serializing_if = "is_untilted")]
    pub tilt_rad: [f64; 2],
}

fn is_untilted(tilt: &[f64; 2]) -> bool {
    *tilt == [0.0, 0.0]
}

impl OpticsSpec {
    /// Physical-range checks, reported under `path`. Whether the lens can focus there, and
    /// whether the camera's `params` match, is `etendue-core`'s to say.
    pub(crate) fn check(&self, issues: &mut Issues, path: &str) {
        issues.check_positive(
            &format!("{path}.sensor.pixel_pitch_m"),
            self.sensor.pixel_pitch_m,
        );
        issues.check_positive(
            &format!("{path}.lens.focal_length_m"),
            self.lens.focal_length_m,
        );
        issues.check_positive(&format!("{path}.lens.f_number"), self.lens.f_number);
        issues.check_positive(&format!("{path}.focus_distance_m"), self.focus_distance_m);
        for (field, value) in [
            ("lens.principal_gap_m", self.lens.principal_gap_m),
            ("lens.entrance_pupil_m", self.lens.entrance_pupil_m),
            ("tilt_rad[0]", self.tilt_rad[0]),
            ("tilt_rad[1]", self.tilt_rad[1]),
        ] {
            if !value.is_finite() {
                issues.push(
                    format!("{path}.{field}"),
                    format!("must be finite, got {value}"),
                );
            }
        }
        if self.lens.entrance_pupil_m >= self.lens.focal_length_m {
            issues.push(
                format!("{path}.lens.entrance_pupil_m"),
                "must lie inside the front focal length (less than focal_length_m)",
            );
        }
        for (i, tau) in self.tilt_rad.iter().enumerate() {
            if tau.abs() >= std::f64::consts::FRAC_PI_4 {
                issues.push(format!("{path}.tilt_rad[{i}]"), "must be under 45°");
            }
        }
    }
}
