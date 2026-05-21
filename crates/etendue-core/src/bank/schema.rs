//! Component-bank schema for the etendue component catalogue.
//!
//! Defines the serializable specification types for the three component
//! families the M3 bank holds: image sensors, lenses, and line lasers. The
//! serde conventions mirror `vision-calibration-core`:
//!
//! - tagged enums with `#[serde(tag = "type", rename_all = "snake_case")]`
//! - `#[serde(flatten)]` for nested parameter structs
//! - `#[serde(alias = "...")]` for backward-compatible field names
//!
//! All physical quantities use SI units unless documented otherwise.
//! `wavelength_nm` stores nanometres (laser-industry convention).
//! Focal length in [`LensSpec`] is metres; pixel pitch in [`SensorSpec`] is
//! metres (physical pixel size on the die).
//!
//! # Component-picker UI
//!
//! Component-picker UI is post-MVP. This module is schema + catalogue + seed
//! files only; the UI comes later (see development plan, post-MVP section).

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// SensorSpec
// ---------------------------------------------------------------------------

/// Specification for an image sensor (camera chip).
///
/// Pixel pitch and resolution are the primary sizing parameters; the optional
/// electro-optical fields are present for future noise / SNR analysis (M6+).
///
/// # Note on values
///
/// Seed-file values are illustrative and should be cross-checked against
/// datasheets before use in a real design.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SensorSpec {
    /// Human-readable sensor name (e.g. `"Sony IMX174"`).
    pub name: String,
    /// Pixel pitch (centre-to-centre distance), in metres (e.g. `5.86e-6`
    /// for a 5.86 µm pixel). Physical pixel size on the die.
    pub pixel_pitch_m: f64,
    /// Resolution as `(width, height)` in pixels (e.g. `(1936, 1216)`).
    pub resolution: (u32, u32),
    /// Full-well capacity in electrons (optional — noise model).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub well_depth_e: Option<f64>,
    /// Read-noise RMS in electrons (optional — noise model).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_noise_e: Option<f64>,
    /// Peak quantum efficiency, dimensionless in `[0, 1]` (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_qe: Option<f64>,
}

// ---------------------------------------------------------------------------
// LensSpec
// ---------------------------------------------------------------------------

/// Lens mount / interface enum.
///
/// Extended as new mount standards are added; the `#[non_exhaustive]` guard
/// future-proofs deserialization of files created by newer versions.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum LensMount {
    /// C-mount (1-inch thread, 17.526 mm back focal distance).
    CMount,
    /// CS-mount (1-inch thread, 12.526 mm back focal distance).
    CsMount,
    /// F-mount (Nikon bayonet, 46.5 mm back focal distance).
    FMount,
    /// M12 (S-mount) board lens.
    M12,
    /// Any other / unspecified mount.
    Other {
        /// Free-text description.
        description: String,
    },
}

/// Placeholder MTF data record — enough structure for a future MTF-aware picker
/// without forcing it now. Left as a single-field struct so M4+ can extend it
/// without a breaking schema change.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MtfPlaceholder {
    /// Spatial frequency at which MTF = 50%, in line-pairs per mm.
    #[serde(alias = "mtf50_lp_per_mm")]
    pub mtf50_lp_mm: Option<f64>,
}

/// Specification for a machine-vision lens.
///
/// Focal length and f-number are the primary optical parameters. MTF is
/// captured as a placeholder for now; a full curve (contrast vs. lp/mm) is
/// M6+.
///
/// # Note on values
///
/// Seed-file values are illustrative and should be cross-checked against
/// datasheets before use in a real design.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LensSpec {
    /// Human-readable lens name (e.g. `"Kowa LM25HC"`).
    pub name: String,
    /// Nominal focal length in metres (e.g. `0.025` for a 25 mm lens).
    pub focal_length_m: f64,
    /// Maximum aperture (minimum f-number), dimensionless (e.g. `1.4`).
    pub max_aperture_f: f64,
    /// Lens mount / interface.
    pub mount: LensMount,
    /// Diagonal angle of view in degrees (optional; derived from focal length
    /// and sensor size when not specified).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fov_diagonal_deg: Option<f64>,
    /// MTF placeholder (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mtf: Option<MtfPlaceholder>,
}

// ---------------------------------------------------------------------------
// LaserSpec
// ---------------------------------------------------------------------------

/// Parametric line-width model kind.
///
/// The MVP uses `Gaussian` with a beam-waist parameter; the `Measured` variant
/// is a placeholder for a future lookup-table approach. The `#[non_exhaustive]`
/// guard future-proofs deserialization.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum LineWidthModel {
    /// Gaussian beam: width governed by beam-waist `w0` at focus.
    Gaussian {
        /// Beam waist at focus (1/e² intensity radius), in metres.
        #[serde(flatten)]
        params: GaussianBeamParams,
    },
    /// Measured line-width profile — placeholder for a lookup-table approach.
    Measured {
        /// Source note (e.g. datasheet figure reference).
        note: String,
    },
}

/// Parameters for a Gaussian beam-waist line-width model.
///
/// Only `w0` is needed for M5's beam-waist model; additional fields (Rayleigh
/// range, divergence half-angle) are derivable from `w0` and the wavelength
/// but can be stored for convenience.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GaussianBeamParams {
    /// Beam waist (1/e² intensity radius) at focus, in metres.
    #[serde(alias = "w_0")]
    pub w0: f64,
}

/// Specification for a line-projecting laser.
///
/// Wavelength and fan half-angle are the primary parameters for the MVP scene.
/// Beam-waist (M5) and optical-power fields are present for future physical
/// models.
///
/// # Note on values
///
/// Seed-file values are illustrative and should be cross-checked against
/// datasheets before use in a real design.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LaserSpec {
    /// Human-readable laser name (e.g. `"Coherent StingRay 660 nm"`).
    pub name: String,
    /// Emission wavelength in nanometres (e.g. `660.0`).
    pub wavelength_nm: f64,
    /// Fan half-angle in radians (e.g. `0.262` ≈ 15°).
    #[serde(alias = "fan_half_angle_rad")]
    pub fan_half_angle: f64,
    /// Optical output power in milliwatts (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power_mw: Option<f64>,
    /// Parametric line-width model (optional; required for M5 beam width).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_width: Option<LineWidthModel>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensor_spec_round_trips_json() {
        let spec = SensorSpec {
            name: "Test Sensor".to_string(),
            pixel_pitch_m: 5.86e-6,
            resolution: (1936, 1216),
            well_depth_e: Some(32000.0),
            read_noise_e: Some(7.0),
            peak_qe: Some(0.72),
        };
        let json = serde_json::to_string(&spec).expect("serialize");
        let back: SensorSpec = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.name, spec.name);
        assert!((back.pixel_pitch_m - spec.pixel_pitch_m).abs() < 1e-12);
        assert_eq!(back.resolution, spec.resolution);
    }

    #[test]
    fn lens_spec_round_trips_json() {
        let spec = LensSpec {
            name: "Test Lens".to_string(),
            focal_length_m: 0.025,
            max_aperture_f: 1.4,
            mount: LensMount::CMount,
            fov_diagonal_deg: Some(45.0),
            mtf: None,
        };
        let json = serde_json::to_string(&spec).expect("serialize");
        let back: LensSpec = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.name, spec.name);
        assert!((back.focal_length_m - spec.focal_length_m).abs() < 1e-12);
    }

    #[test]
    fn laser_spec_round_trips_json() {
        let spec = LaserSpec {
            name: "Test Laser".to_string(),
            wavelength_nm: 660.0,
            fan_half_angle: 0.262,
            power_mw: Some(30.0),
            line_width: Some(LineWidthModel::Gaussian {
                params: GaussianBeamParams { w0: 50e-6 },
            }),
        };
        let json = serde_json::to_string(&spec).expect("serialize");
        let back: LaserSpec = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.name, spec.name);
        assert!((back.wavelength_nm - spec.wavelength_nm).abs() < 1e-12);
    }

    #[test]
    fn lens_mount_other_round_trips() {
        let mount = LensMount::Other {
            description: "Custom bayonet".to_string(),
        };
        let json = serde_json::to_string(&mount).expect("serialize");
        let back: LensMount = serde_json::from_str(&json).expect("deserialize");
        assert!(
            matches!(back, LensMount::Other { ref description } if description == "Custom bayonet"),
            "expected LensMount::Other {{ description: \"Custom bayonet\" }}, got {back:?}",
        );
    }
}
