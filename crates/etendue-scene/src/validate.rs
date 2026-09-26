//! Validation results shared by every spec type.
//!
//! Validation collects **all** problems it finds rather than stopping at the
//! first, so a CLI or UI can show a complete list. Each [`Issue`] carries a
//! path into the document (`cameras[2].parent`) and a message.

use std::fmt;

use nalgebra::Isometry3;

/// One problem found while validating a spec document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    /// Location in the document, e.g. `cameras[2].parent_se3_self`.
    pub path: String,
    /// What is wrong, in one sentence.
    pub message: String,
}

impl Issue {
    pub(crate) fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

/// A spec document failed validation. Holds every [`Issue`] found.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub struct ValidationError {
    /// All problems found, in document order.
    pub issues: Vec<Issue>,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} validation issue(s)", self.issues.len())?;
        for issue in &self.issues {
            write!(f, "\n  - {issue}")?;
        }
        Ok(())
    }
}

/// Accumulates issues; converts to `Result` at the end.
#[derive(Default)]
pub(crate) struct Issues(Vec<Issue>);

impl Issues {
    pub(crate) fn push(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.0.push(Issue::new(path, message));
    }

    pub(crate) fn finish(self) -> Result<(), ValidationError> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(ValidationError { issues: self.0 })
        }
    }

    pub(crate) fn into_error(self) -> ValidationError {
        ValidationError { issues: self.0 }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Check a pose: finite components and a unit quaternion.
    ///
    /// `nalgebra` deserialises `UnitQuaternion` without renormalising, so a
    /// hand-typed `[0, 0, 0.7071, 0.7071]` would silently become a scaled —
    /// non-rigid — transform. Ground truth cannot tolerate that, so the norm
    /// must be 1 to within [`UNIT_QUATERNION_TOLERANCE`].
    pub(crate) fn check_pose(&mut self, path: &str, pose: &Isometry3<f64>) {
        let q = pose.rotation.quaternion().coords;
        let t = pose.translation.vector;
        if !(q.iter().all(|c| c.is_finite()) && t.iter().all(|c| c.is_finite())) {
            self.push(path, "pose has a non-finite component");
            return;
        }
        let norm = q.norm();
        if (norm - 1.0).abs() > UNIT_QUATERNION_TOLERANCE {
            self.push(
                path,
                format!(
                    "rotation quaternion must be unit length (|q| = {norm:.12}); \
                     write it to full double precision"
                ),
            );
        }
    }

    pub(crate) fn check_positive(&mut self, path: &str, value: f64) {
        if !(value.is_finite() && value > 0.0) {
            self.push(path, format!("must be finite and > 0, got {value}"));
        }
    }
}

/// Maximum allowed deviation of a pose quaternion's norm from 1.
pub const UNIT_QUATERNION_TOLERANCE: f64 = 1e-9;
