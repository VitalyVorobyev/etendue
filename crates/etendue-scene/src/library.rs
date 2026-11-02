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
//!
//! The same layout serves a user bank (ADR 0010, [`crate::bank`]). An item copied from the bank or
//! the built-ins into a project records where it came from ([`ItemOrigin`]), with its source's
//! content hash (`etendue_core::library::content_hash`), which tells whether the source has
//! changed since.

use serde::{Deserialize, Serialize};

use crate::entity::TargetGeometry;
use crate::optics::Focusing;
use crate::rig::{CameraTemplate, RigLayout};
use crate::validate::{Issue, Issues, ValidationError};

/// A library file's version alone: what to read first (the rest of the file is ignored).
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Versioned {
    /// The file's format version.
    pub version: u32,
}

/// The library item format this build writes. Version 2 (ADR 0010) adds `source`, `origin` and the
/// `mount` kind; version 1 files still read.
pub const LIBRARY_VERSION: u32 = 2;

/// The library item formats this build reads.
pub const LIBRARY_VERSIONS: [u32; 2] = [1, LIBRARY_VERSION];

/// Where an item's data comes from: its datasheet, for people.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ItemSource {
    /// The maker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
    /// The maker's part number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_number: Option<String>,
    /// The datasheet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub datasheet_url: Option<String>,
    /// Anything else: how a value was measured, what was assumed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// A library layer (ADR 0010 §1) an item can be copied from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Layer {
    /// Shipped with etendue, read-only.
    Builtin,
    /// The user's bank.
    Bank,
}

/// Where a project's copy of an item came from (ADR 0010 §3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ItemOrigin {
    /// The layer it was copied from.
    pub layer: Layer,
    /// Its reference there, `<folder>/<id>`.
    pub reference: String,
    /// The source's content hash (`etendue_core::library::content_hash`) when it was copied.
    pub sha256: String,
}

/// A rigid adapter on a robot's tool flange (ADR 0010 §5): what a rig or a target is mounted
/// through, eye-in-hand or eye-to-hand.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct MountPart {
    /// The mounted device's frame in the flange's frame.
    #[cfg_attr(
        feature = "schemars",
        schemars(with = "vision_calibration_core::Iso3Schema")
    )]
    pub flange_se3_device: nalgebra::Isometry3<f64>,
    /// Mass, kilograms, for the payload budget.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mass_kg: Option<f64>,
}

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
    /// Distance from the laser origin to the waist along each ray, metres,
    /// for a laser focused at a fixed distance. Absent, the laser is focusable:
    /// a head design focuses it (the studio, at the head's aim point).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waist_distance_m: Option<f64>,
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

/// Where a head must work: a window in its laser plane, measured from the laser's origin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct HeadWindow {
    /// Nearest range along the laser's central ray, metres.
    pub near_m: f64,
    /// Farthest range, metres.
    pub far_m: f64,
    /// Width along the laser line, centred on the central ray, metres.
    pub width_m: f64,
    /// The largest acceptable blur (geometric and diffraction), pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_blur_px: Option<f64>,
}

/// A laser-triangulation head: a camera design and a line laser fixed to the camera's body.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct HeadDesign {
    /// The camera: physical (its `optics` set), as a head's analyses need.
    pub camera: CameraTemplate,
    /// The laser.
    pub laser: LaserPart,
    /// Fan radius, metres: how far the fan reaches from the laser, the same
    /// along every ray.
    pub fan_length_m: f64,
    /// The laser's pose in the camera's body frame (its fan in the laser's `y–z` plane,
    /// opening about `+z`).
    #[cfg_attr(
        feature = "schemars",
        schemars(with = "vision_calibration_core::Iso3Schema")
    )]
    pub camera_se3_laser: nalgebra::Isometry3<f64>,
    /// Where it must work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<HeadWindow>,
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
    /// Its datasheet (version 2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<ItemSource>,
    /// Where this copy came from, when it was copied into a project (version 2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<ItemOrigin>,
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
    /// A laser-triangulation head.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<HeadDesign>,
    /// A rig design.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rig: Option<RigDesign>,
    /// A flange mount (version 2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mount: Option<MountPart>,
    /// A calibration procedure (version 2, ADR 0011).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub procedure: Option<crate::procedure::ProcedurePart>,
}

/// The kinds of library item, and the folder each lives in.
pub const KINDS: [(&str, &str); 9] = [
    ("sensor", "sensors"),
    ("lens", "lenses"),
    ("laser", "lasers"),
    ("target", "targets"),
    ("camera", "cameras"),
    ("head", "heads"),
    ("rig", "rigs"),
    ("mount", "mounts"),
    ("procedure", "procedures"),
];

impl LibraryItem {
    /// Check a library file's version before parsing the rest of it (read the version alone
    /// with [`Versioned`]), so a file from a newer etendue is refused as such rather than for a
    /// field this build does not know.
    ///
    /// # Errors
    ///
    /// A [`ValidationError`] at `version` for a version not in [`LIBRARY_VERSIONS`].
    pub fn check_version(version: u32) -> Result<(), ValidationError> {
        if LIBRARY_VERSIONS.contains(&version) {
            return Ok(());
        }
        Err(ValidationError {
            issues: vec![Issue {
                path: "version".to_owned(),
                message: format!(
                    "library item version {version}; this etendue reads versions 1 to {LIBRARY_VERSION}"
                ),
            }],
        })
    }

    /// Its folder: where an item of its kind lives (`cameras`), if it has one kind.
    #[must_use]
    pub fn folder(&self) -> Option<&'static str> {
        let kind = self.kind()?;
        KINDS.iter().find(|(k, _)| *k == kind).map(|(_, f)| *f)
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
            self.head.is_some(),
            self.rig.is_some(),
            self.mount.is_some(),
            self.procedure.is_some(),
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
        if !LIBRARY_VERSIONS.contains(&self.version) {
            issues.push(
                "version",
                format!("unsupported library item version {}", self.version),
            );
        }
        if self.version < 2
            && (self.source.is_some()
                || self.origin.is_some()
                || self.mount.is_some()
                || self.procedure.is_some())
        {
            issues.push(
                "version",
                "`source`, `origin`, `mount` and `procedure` need library version 2",
            );
        }
        if let Some(o) = &self.origin {
            if crate::project::library_file(&o.reference, &KINDS.map(|(_, f)| f)).is_none() {
                issues.push(
                    "origin.reference",
                    format!(
                        "`{}` is not a library reference (`<folder>/<id>`)",
                        o.reference
                    ),
                );
            }
            if o.sha256.len() != 64 || !o.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
                issues.push("origin.sha256", "must be 64 hex digits");
            }
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
                "must hold exactly one of sensor, lens, laser, target, camera, head, rig, mount, procedure",
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
            if let Some(d) = l.waist_distance_m {
                issues.check_non_negative("laser.waist_distance_m", d);
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
        if let Some(h) = &self.head {
            if h.camera.optics.is_none() {
                issues.push(
                    "head.camera.optics",
                    "a head's camera must be physical (have optics)",
                );
            }
            if let Some(o) = &h.camera.optics {
                o.check(&mut issues, "head.camera.optics");
            }
            issues.check_positive("head.laser.wavelength_nm", h.laser.wavelength_nm);
            if !(h.laser.fan_half_angle > 0.0
                && h.laser.fan_half_angle < std::f64::consts::FRAC_PI_2)
            {
                issues.push("head.laser.fan_half_angle", "must be in (0, π/2)");
            }
            issues.check_positive("head.fan_length_m", h.fan_length_m);
            issues.check_pose("head.camera_se3_laser", &h.camera_se3_laser);
            if let Some(w) = &h.window {
                issues.check_positive("head.window.width_m", w.width_m);
                if !(w.near_m.is_finite()
                    && w.far_m.is_finite()
                    && 0.0 < w.near_m
                    && w.near_m <= w.far_m)
                {
                    issues.push("head.window", "needs 0 < near_m ≤ far_m");
                }
                if let Some(b) = w.max_blur_px {
                    issues.check_positive("head.window.max_blur_px", b);
                }
            }
        }
        if let Some(r) = &self.rig {
            r.layout.check(&mut issues, "rig.layout");
        }
        if let Some(p) = &self.procedure {
            p.check(&mut issues, "procedure");
        }
        if let Some(m) = &self.mount {
            issues.check_pose("mount.flange_se3_device", &m.flange_se3_device);
            if let Some(kg) = m.mass_kg {
                issues.check_non_negative("mount.mass_kg", kg);
            }
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

    /// What a reader does: the version first, then the item.
    fn read(text: &str) -> Result<LibraryItem, ValidationError> {
        let v: Versioned = serde_json::from_str(text).map_err(|e| ValidationError {
            issues: vec![Issue {
                path: String::new(),
                message: e.to_string(),
            }],
        })?;
        LibraryItem::check_version(v.version)?;
        serde_json::from_str(text).map_err(|e| ValidationError {
            issues: vec![Issue {
                path: String::new(),
                message: e.to_string(),
            }],
        })
    }

    #[test]
    fn an_item_holds_exactly_one_part() {
        let item = read(sensor()).unwrap();
        assert_eq!(item.kind(), Some("sensor"));
        item.validate().unwrap();
        let mut two = item.clone();
        two.laser = Some(LaserPart {
            wavelength_nm: 660.0,
            fan_half_angle: 0.3,
            beam_waist_m: None,
            waist_distance_m: None,
            power_mw: None,
        });
        assert_eq!(two.kind(), None);
        assert!(two.validate().is_err());
    }

    #[test]
    fn version_1_still_reads_and_its_copy_carries_an_origin() {
        let v1 = read(sensor()).unwrap();
        assert_eq!(v1.version, 1);
        let mut copy = v1.clone();
        copy.version = LIBRARY_VERSION;
        copy.origin = Some(ItemOrigin {
            layer: Layer::Bank,
            reference: "sensors/imx174".into(),
            sha256: "0".repeat(64),
        });
        copy.validate().unwrap();
        // Version 2's fields in a version 1 file are refused.
        let mut early = v1.clone();
        early.source = Some(ItemSource {
            vendor: Some("Sony".into()),
            ..ItemSource::default()
        });
        assert!(early.validate().is_err());
        early.version = 2;
        early.validate().unwrap();
    }

    #[test]
    fn a_mount_is_a_pose_and_a_bad_origin_is_refused() {
        let text = r#"{ "version": 2, "id": "flange_plate", "name": "Flange plate",
            "mount": { "flange_se3_device": { "rotation": [0, 0, 0, 1], "translation": [0, 0, 0.04] }, "mass_kg": 0.3 } }"#;
        let item = read(text).unwrap();
        assert_eq!(item.kind(), Some("mount"));
        assert_eq!(item.folder(), Some("mounts"));
        item.validate().unwrap();
        let mut bad = item.clone();
        bad.origin = Some(ItemOrigin {
            layer: Layer::Builtin,
            reference: "../x".into(),
            sha256: "abc".into(),
        });
        let paths: Vec<_> = bad
            .validate()
            .unwrap_err()
            .issues
            .into_iter()
            .map(|i| i.path)
            .collect();
        assert_eq!(paths, ["origin.reference", "origin.sha256"]);
    }

    #[test]
    fn unknown_fields_are_refused_and_the_version_comes_first() {
        let extra = sensor().replace(r#""name""#, r#""colour": "red", "name""#);
        assert!(read(&extra).is_err());
        let newer = extra.replace(r#""version": 1"#, r#""version": 7"#);
        let err = read(&newer).unwrap_err();
        assert_eq!(err.issues[0].path, "version");
        assert!(err.issues[0].message.contains("version 7"), "{err}");
    }
}
