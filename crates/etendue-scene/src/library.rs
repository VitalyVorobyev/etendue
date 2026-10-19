//! A project's component library (ADR 0008 §5, ADR 0009): parts and designs, one JSON file
//! each, at `library/<kind>s/<id>.json` in a project folder.
//!
//! ```json
//! { "version": 1, "id": "imx174", "name": "Sony IMX174", "sensor": { "pixel_pitch_m": 5.86e-6, … } }
//! ```
//!
//! An item holds exactly one part, under the key that names its kind. Designs embed their parts by
//! value, so a library file is complete on its own; scenes copy designs by value too, so a scene
//! never depends on a library.

use serde::{Deserialize, Serialize};

use crate::entity::TargetGeometry;
use crate::optics::Focusing;
use crate::rig::{CameraTemplate, RigLayout};
use crate::validate::{Issue, Issues, ValidationError};

/// The library item format this build reads and writes.
pub const LIBRARY_VERSION: u32 = 1;

/// An image sensor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct SensorPart {
    /// Pixel pitch, metres.
    pub pixel_pitch_m: f64,
    /// Active pixels, `[width, height]`.
    pub resolution: [u32; 2],
    /// Full-well capacity, electrons.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub well_depth_e: Option<f64>,
    /// Read noise (RMS), electrons.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_noise_e: Option<f64>,
    /// Peak quantum efficiency, in `[0, 1]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peak_qe: Option<f64>,
}

/// A lens mount.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum LensMount {
    /// C-mount: flange focal distance 17.526 mm.
    CMount,
    /// CS-mount: 12.526 mm.
    CsMount,
    /// Nikon F: 46.5 mm.
    FMount,
    /// M12 (S-mount) board lens.
    M12,
    /// Anything else.
    Other,
}

/// A lens: what a datasheet gives, and the thick-lens data the camera model needs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct LensPart {
    /// Effective focal length, metres.
    pub focal_length_m: f64,
    /// The widest aperture: the smallest f-number.
    pub min_f_number: f64,
    /// Signed distance from H′ to H, metres (`0` when unknown: a thin lens).
    #[serde(default)]
    pub principal_gap_m: f64,
    /// Signed distance from H to the entrance pupil, metres (`0` when unknown).
    #[serde(default)]
    pub entrance_pupil_m: f64,
    /// How the lens focuses.
    #[serde(default)]
    pub focusing: Focusing,
    /// The mount.
    pub mount: LensMount,
    /// Diagonal field of view on the sensor it was specified for, degrees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fov_diagonal_deg: Option<f64>,
    /// Spatial frequency at 50 % MTF, line pairs per millimetre.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtf50_lp_mm: Option<f64>,
}

/// A line laser.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct LaserPart {
    /// Emission wavelength, nanometres.
    pub wavelength_nm: f64,
    /// Fan half-angle, radians.
    pub fan_half_angle: f64,
    /// Beam-waist `1/e²` radius across the line, metres, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub beam_waist_m: Option<f64>,
    /// Optical power, milliwatts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power_mw: Option<f64>,
}

/// A calibration target.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct TargetPart {
    /// What it is, as a scene target holds it.
    pub geometry: TargetGeometry,
}

/// A rig of identical units.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct RigDesign {
    /// The layout, its camera (and lasers) included.
    pub layout: RigLayout,
}

/// One library item. Exactly one of the part fields is set: it names the item's kind.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct LibraryItem {
    /// Format version, [`LIBRARY_VERSION`].
    pub version: u32,
    /// The item's id: its file stem.
    pub id: String,
    /// What it is, for people.
    pub name: String,
    /// More about it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// A sensor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sensor: Option<SensorPart>,
    /// A lens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lens: Option<LensPart>,
    /// A line laser.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub laser: Option<LaserPart>,
    /// A calibration target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<TargetPart>,
    /// A camera design: optics (when physical), the as-built model and the image size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera: Option<CameraTemplate>,
    /// A rig design.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rig: Option<RigDesign>,
}

/// The kinds of library item, and the folder each lives in.
pub const KINDS: [(&str, &str); 6] = [
    ("sensor", "sensors"),
    ("lens", "lenses"),
    ("laser", "lasers"),
    ("target", "targets"),
    ("camera", "cameras"),
    ("rig", "rigs"),
];

impl LibraryItem {
    /// Parse a library file: its `version` is checked before anything else, so a file from a
    /// newer etendue is refused as such rather than for a field this build does not know.
    ///
    /// # Errors
    ///
    /// A [`ValidationError`] for a file that is not JSON, has another version, or does not parse.
    pub fn from_json(text: &str) -> Result<Self, ValidationError> {
        let one = |path: &str, message: String| ValidationError {
            issues: vec![Issue {
                path: path.to_owned(),
                message,
            }],
        };
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|e| one("", format!("not JSON: {e}")))?;
        match value.get("version").and_then(serde_json::Value::as_u64) {
            Some(v) if v == u64::from(LIBRARY_VERSION) => {}
            Some(v) => {
                return Err(one(
                    "version",
                    format!(
                        "library item version {v}; this etendue reads version {LIBRARY_VERSION}"
                    ),
                ));
            }
            None => return Err(one("version", "missing or not a number".into())),
        }
        serde_json::from_value(value).map_err(|e| one("", e.to_string()))
    }

    /// The item's kind: the one part it holds, or `None` when it holds none or several.
    #[must_use]
    pub fn kind(&self) -> Option<&'static str> {
        let set = [
            self.sensor.is_some(),
            self.lens.is_some(),
            self.laser.is_some(),
            self.target.is_some(),
            self.camera.is_some(),
            self.rig.is_some(),
        ];
        (set.iter().filter(|s| **s).count() == 1)
            .then(|| KINDS[set.iter().position(|s| *s).unwrap_or(0)].0)
    }

    /// Structural checks.
    ///
    /// # Errors
    ///
    /// Every problem found.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut issues = Issues::default();
        if self.version != LIBRARY_VERSION {
            issues.push(
                "version",
                format!("unsupported library item version {}", self.version),
            );
        }
        if !crate::frame::is_valid_id(&self.id) {
            issues.push("id", format!("`{}` is not a valid id", self.id));
        }
        if self.name.trim().is_empty() {
            issues.push("name", "must not be empty");
        }
        if self.kind().is_none() {
            issues.push(
                "",
                "must hold exactly one of sensor, lens, laser, target, camera, rig",
            );
        }
        if let Some(s) = &self.sensor {
            issues.check_positive("sensor.pixel_pitch_m", s.pixel_pitch_m);
            if s.resolution[0] == 0 || s.resolution[1] == 0 {
                issues.push("sensor.resolution", "must be non-zero");
            }
            if let Some(qe) = s.peak_qe
                && !(0.0..=1.0).contains(&qe)
            {
                issues.push("sensor.peak_qe", "must be in [0, 1]");
            }
        }
        if let Some(l) = &self.lens {
            issues.check_positive("lens.focal_length_m", l.focal_length_m);
            issues.check_positive("lens.min_f_number", l.min_f_number);
            if l.entrance_pupil_m >= l.focal_length_m {
                issues.push(
                    "lens.entrance_pupil_m",
                    "must lie inside the front focal length",
                );
            }
        }
        if let Some(l) = &self.laser {
            issues.check_positive("laser.wavelength_nm", l.wavelength_nm);
            if !(l.fan_half_angle > 0.0 && l.fan_half_angle < std::f64::consts::FRAC_PI_2) {
                issues.push("laser.fan_half_angle", "must be in (0, π/2)");
            }
            if let Some(w) = l.beam_waist_m {
                issues.check_positive("laser.beam_waist_m", w);
            }
        }
        if let Some(c) = &self.camera {
            if c.resolution[0] == 0 || c.resolution[1] == 0 {
                issues.push("camera.resolution", "must be non-zero");
            }
            if let Err(e) = c.params.build() {
                issues.push("camera.params", format!("camera model does not build: {e}"));
            }
            if let Some(o) = &c.optics {
                o.check(&mut issues, "camera.optics");
            }
        }
        if let Some(r) = &self.rig {
            r.layout.check(&mut issues, "rig.layout");
        }
        issues.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sensor() -> &'static str {
        r#"{ "version": 1, "id": "imx174", "name": "Sony IMX174", "sensor": { "pixel_pitch_m": 5.86e-6, "resolution": [1936, 1216] } }"#
    }

    #[test]
    fn an_item_holds_exactly_one_part() {
        let item = LibraryItem::from_json(sensor()).unwrap();
        assert_eq!(item.kind(), Some("sensor"));
        item.validate().unwrap();
        let mut two = item.clone();
        two.laser = Some(LaserPart {
            wavelength_nm: 660.0,
            fan_half_angle: 0.3,
            beam_waist_m: None,
            power_mw: None,
        });
        assert_eq!(two.kind(), None);
        assert!(two.validate().is_err());
    }

    #[test]
    fn unknown_fields_are_refused_and_the_version_comes_first() {
        let extra = sensor().replace(r#""name""#, r#""colour": "red", "name""#);
        assert!(LibraryItem::from_json(&extra).is_err());
        let newer = extra.replace(r#""version": 1"#, r#""version": 7"#);
        let err = LibraryItem::from_json(&newer).unwrap_err();
        assert_eq!(err.issues[0].path, "version");
        assert!(err.issues[0].message.contains("version 7"), "{err}");
    }
}
