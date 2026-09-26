//! Component catalogue: the in-memory registry of [`SensorSpec`], [`LensSpec`],
//! and [`LaserSpec`] entries loaded from JSON bank files.
//!
//! The catalogue is intentionally a simple wrapper over three `Vec`s — the MVP
//! needs load + list, nothing more. A future picker UI will add indexing,
//! filtering, and fuzzy search on top of this structure without a schema
//! change.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::schema::{LaserSpec, LensSpec, SensorSpec};

/// A collection of sensor, lens, and laser component specs loaded from JSON.
///
/// The top-level JSON format is a single object with three arrays:
///
/// ```json
/// {
///   "sensors": [ ... ],
///   "lenses":  [ ... ],
///   "lasers":  [ ... ]
/// }
/// ```
///
/// Each array element is a serialized [`SensorSpec`], [`LensSpec`], or
/// [`LaserSpec`] respectively.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Catalog {
    /// Image sensor specifications.
    #[serde(default)]
    pub sensors: Vec<SensorSpec>,
    /// Lens specifications.
    #[serde(default)]
    pub lenses: Vec<LensSpec>,
    /// Line-laser specifications.
    #[serde(default)]
    pub lasers: Vec<LaserSpec>,
}

impl Catalog {
    /// Create an empty catalogue.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Deserialize a [`Catalog`] from a JSON string.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::Io`] if the string is not valid JSON or does
    /// not match the catalogue schema.
    pub fn load_from_str(s: &str) -> crate::Result<Self> {
        serde_json::from_str(s)
            .map_err(|e| crate::Error::Io(format!("cannot parse catalog JSON: {e}")))
    }

    /// Deserialize a [`Catalog`] from a JSON file at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::Io`] if the file cannot be read or its contents
    /// do not match the catalogue schema.
    pub fn load_from_path(path: impl AsRef<Path>) -> crate::Result<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|e| {
            crate::Error::Io(format!("cannot read catalog file {}: {e}", path.display()))
        })?;
        serde_json::from_str(&text).map_err(|e| {
            crate::Error::Io(format!(
                "cannot parse catalog JSON from {}: {e}",
                path.display()
            ))
        })
    }

    /// Total number of components across all three families.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sensors.len() + self.lenses.len() + self.lasers.len()
    }

    /// True if the catalogue has no components in any family.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Embed the seed bank files at compile time so the test runs without
    // filesystem access (and catches schema drift the moment a seed file is
    // edited).
    const SEED_SENSORS: &str = include_str!("../../assets/bank/sensors.json");
    const SEED_LENSES: &str = include_str!("../../assets/bank/lenses.json");
    const SEED_LASERS: &str = include_str!("../../assets/bank/lasers.json");

    #[test]
    fn seed_sensors_deserialize() {
        let catalog: Catalog =
            Catalog::load_from_str(SEED_SENSORS).expect("seed sensors.json must deserialize");
        assert!(
            !catalog.sensors.is_empty(),
            "seed sensors catalog must have at least one entry"
        );
    }

    #[test]
    fn seed_lenses_deserialize() {
        let catalog: Catalog =
            Catalog::load_from_str(SEED_LENSES).expect("seed lenses.json must deserialize");
        assert!(
            !catalog.lenses.is_empty(),
            "seed lenses catalog must have at least one entry"
        );
    }

    #[test]
    fn seed_lasers_deserialize() {
        let catalog: Catalog =
            Catalog::load_from_str(SEED_LASERS).expect("seed lasers.json must deserialize");
        assert!(
            !catalog.lasers.is_empty(),
            "seed lasers catalog must have at least one entry"
        );
    }

    #[test]
    fn empty_catalog_is_empty() {
        let c = Catalog::empty();
        assert!(c.is_empty());
        assert_eq!(c.len(), 0);
    }

    #[test]
    fn catalog_len_sums_all_families() {
        let json = r#"{"sensors": [{"name":"S","pixel_pitch_m":5e-6,"resolution":[1920,1080]}],
                       "lenses": [],
                       "lasers": []}"#;
        let c = Catalog::load_from_str(json).unwrap();
        assert_eq!(c.len(), 1);
    }
}
