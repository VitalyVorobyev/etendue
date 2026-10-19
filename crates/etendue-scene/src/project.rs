//! A project folder's manifest, `etendue-project.json` (ADR 0009).
//!
//! ```json
//! { "version": 1, "name": "laser heads", "cells": ["cells/ring3"],
//!   "defaults": { "camera": "cameras/imx174_lm25hc", "target": "targets/chess_9x6_20mm" } }
//! ```
//!
//! Every path is relative to the project folder and uses `/`. A library reference names an item
//! by its folder and id: `cameras/imx174_lm25hc` is `library/cameras/imx174_lm25hc.json`.

use serde::{Deserialize, Serialize};

use crate::library::KINDS;
use crate::validate::{Issues, ValidationError};

/// The manifest format this build reads and writes.
pub const PROJECT_VERSION: u32 = 1;

/// The manifest's file name, at the project folder's root.
pub const PROJECT_FILE: &str = "etendue-project.json";

/// What new things in the studio are made from: library references.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ProjectDefaults {
    /// A camera design (`cameras/<id>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera: Option<String>,
    /// A laser part (`lasers/<id>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub laser: Option<String>,
    /// A target part (`targets/<id>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

/// `etendue-project.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ProjectManifest {
    /// Format version, [`PROJECT_VERSION`].
    pub version: u32,
    /// What the project is.
    pub name: String,
    /// The cell folders, each holding `scene.json` and usually `scenario.json`.
    #[serde(default)]
    pub cells: Vec<String>,
    /// Robots that come with the project: folders holding `robot.json`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub robots: Vec<String>,
    /// What new things are made from.
    #[serde(default)]
    pub defaults: ProjectDefaults,
}

/// The library folder of a kind (`camera` → `cameras`).
fn folder(kind: &'static str) -> &'static str {
    KINDS
        .iter()
        .find(|(k, _)| *k == kind)
        .map_or(kind, |(_, f)| *f)
}

/// Whether `path` is a relative path inside the project: `/`-separated, no `..`, not absolute.
#[must_use]
pub fn is_project_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// The library file a reference names (`cameras/x` → `library/cameras/x.json`), when it names
/// one of `kinds` (the folder names) with a valid id.
#[must_use]
pub fn library_file(reference: &str, kinds: &[&str]) -> Option<String> {
    let (kind, id) = reference.split_once('/')?;
    (kinds.contains(&kind) && crate::frame::is_valid_id(id))
        .then(|| format!("library/{kind}/{id}.json"))
}

impl ProjectManifest {
    /// Structural checks: version, paths, references. Whether the files exist is the reader's to
    /// say.
    ///
    /// # Errors
    ///
    /// Every problem found.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut issues = Issues::default();
        if self.version != PROJECT_VERSION {
            issues.push(
                "version",
                format!(
                    "project version {}; this etendue reads version {PROJECT_VERSION}",
                    self.version
                ),
            );
        }
        if self.name.trim().is_empty() {
            issues.push("name", "must not be empty");
        }
        for (list, paths) in [("cells", &self.cells), ("robots", &self.robots)] {
            for (i, p) in paths.iter().enumerate() {
                if !is_project_path(p) {
                    issues.push(
                        format!("{list}[{i}]"),
                        format!("`{p}` is not a relative path inside the project"),
                    );
                }
            }
        }
        for (field, value, kind) in [
            ("camera", &self.defaults.camera, "camera"),
            ("laser", &self.defaults.laser, "laser"),
            ("target", &self.defaults.target, "target"),
        ] {
            if let Some(r) = value
                && library_file(r, &[folder(kind)]).is_none()
            {
                issues.push(
                    format!("defaults.{field}"),
                    format!(
                        "`{r}` is not a reference to a {kind} (`{}/<id>`)",
                        folder(kind)
                    ),
                );
            }
        }
        issues.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_stay_inside_the_project() {
        for ok in ["cells/ring3", "eye_in_hand_ur5e", "a/b/c"] {
            assert!(is_project_path(ok), "{ok}");
        }
        for bad in [
            "", "/abs", "../up", "a/../b", "a//b", "./a", "c:\\x", "a\\b", "C:/x",
        ] {
            assert!(!is_project_path(bad), "{bad}");
        }
    }

    #[test]
    fn references_name_a_folder_and_an_id() {
        assert_eq!(
            library_file("cameras/imx174_lm25hc", &["cameras"]).as_deref(),
            Some("library/cameras/imx174_lm25hc.json")
        );
        assert_eq!(library_file("lasers/x", &["cameras"]), None);
        assert_eq!(library_file("cameras/../x", &["cameras"]), None);
        assert_eq!(library_file("cameras", &["cameras"]), None);
    }

    #[test]
    fn a_manifest_reports_every_problem() {
        let m = ProjectManifest {
            version: 2,
            name: " ".into(),
            cells: vec!["../elsewhere".into()],
            robots: vec![],
            defaults: ProjectDefaults {
                camera: Some("lasers/l".into()),
                laser: None,
                target: None,
            },
        };
        let paths: Vec<String> = m
            .validate()
            .unwrap_err()
            .issues
            .into_iter()
            .map(|i| i.path)
            .collect();
        assert_eq!(paths, ["version", "name", "cells[0]", "defaults.camera"]);
    }
}
