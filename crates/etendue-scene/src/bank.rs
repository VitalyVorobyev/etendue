//! A user bank's manifest, `etendue-bank.json` (ADR 0010).
//!
//! A bank is a folder shaped like a project's library, shared by every project:
//!
//! ```text
//! <bank>/
//!   etendue-bank.json                {"version": 1, "name": "…"}
//!   library/<folder>/<id>.json       (as a project's, crate::library)
//!   robots/<id>/{robot.json, robot.urdf, meshes/}
//! ```
//!
//! It has no cells, datasets or runs. Projects copy what they use from it.

use serde::{Deserialize, Serialize};

use crate::validate::{Issues, ValidationError};

/// The bank format this build reads and writes.
pub const BANK_VERSION: u32 = 1;

/// The manifest's file name.
pub const BANK_FILE: &str = "etendue-bank.json";

/// `etendue-bank.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct BankManifest {
    /// Format version, [`BANK_VERSION`].
    pub version: u32,
    /// What it is, for people.
    pub name: String,
}

impl BankManifest {
    /// A new bank's manifest.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            version: BANK_VERSION,
            name: name.into(),
        }
    }

    /// Structural checks.
    ///
    /// # Errors
    ///
    /// Every problem found.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut issues = Issues::default();
        if self.version != BANK_VERSION {
            issues.push(
                "version",
                format!(
                    "bank version {}; this etendue reads version {BANK_VERSION}",
                    self.version
                ),
            );
        }
        if self.name.trim().is_empty() {
            issues.push("name", "must not be empty");
        }
        issues.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bank_has_a_name_and_a_known_version() {
        BankManifest::new("my bank").validate().unwrap();
        let unnamed = BankManifest::new(" ");
        assert_eq!(unnamed.validate().unwrap_err().issues[0].path, "name");
        let newer = BankManifest {
            version: 9,
            ..BankManifest::new("x")
        };
        assert!(newer.validate().is_err());
        assert!(
            serde_json::from_str::<BankManifest>(r#"{"version":1,"name":"b","cells":[]}"#).is_err()
        );
    }
}
