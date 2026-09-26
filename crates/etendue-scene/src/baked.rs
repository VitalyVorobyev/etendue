//! Baked scenarios: the kinematics output every other consumer reads
//! ([ADR 0003](https://github.com/VitalyVorobyev/etendue/blob/main/docs/adrs/0003-baking.md)).
//!
//! A [`BakedScenario`] lists every frame of the scene once (topological
//! order, `"world"` first) and, per sample, `world_se3_frame` index-aligned
//! with that list. Consumers apply these transforms; they never compute a
//! pose from joint values.

use nalgebra::Isometry3;
use serde::{Deserialize, Serialize};

use crate::validate::{Issues, ValidationError};

/// A scenario sampled at a fixed period.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct BakedScenario {
    /// Format version; `1`.
    pub version: u32,
    /// Sample period in seconds.
    pub dt: f64,
    /// Frame names, topological order, `"world"` first. `world_se3_frame`
    /// in every sample is index-aligned with this list.
    pub frames: Vec<String>,
    /// Robots whose joint positions each sample records, in scene order.
    pub robots: Vec<BakedRobot>,
    /// Samples at `t = k · dt`.
    pub samples: Vec<BakedSample>,
}

/// A robot's joint naming, for [`BakedSample::joint_positions`].
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct BakedRobot {
    /// Robot id.
    pub id: String,
    /// Joint names in `q` order.
    pub joint_names: Vec<String>,
}

/// One sample.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct BakedSample {
    /// Time in seconds.
    pub t: f64,
    /// Pose of every frame in the world, aligned with
    /// [`BakedScenario::frames`].
    #[cfg_attr(
        feature = "schemars",
        schemars(with = "Vec<vision_calibration_core::Iso3Schema>")
    )]
    pub world_se3_frame: Vec<Isometry3<f64>>,
    /// Joint positions per robot, aligned with [`BakedScenario::robots`].
    /// For display only.
    pub joint_positions: Vec<Vec<f64>>,
    /// Set on the stationary sample at which images are captured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture: Option<CaptureEvent>,
}

/// A capture marker.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CaptureEvent {
    /// Capture id (unique within the scenario).
    pub id: String,
}

impl BakedScenario {
    /// Consistency checks: version, `dt`, array alignment, increasing times,
    /// unique capture ids.
    ///
    /// # Errors
    ///
    /// Every problem found.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut issues = Issues::default();
        if self.version != 1 {
            issues.push(
                "version",
                format!("unsupported baked version {}", self.version),
            );
        }
        issues.check_positive("dt", self.dt);
        if self.frames.first().map(String::as_str) != Some(crate::WORLD) {
            issues.push("frames", "the first frame must be `world`");
        }
        let mut captures: Vec<&str> = Vec::new();
        let mut last_t = f64::NEG_INFINITY;
        for (k, s) in self.samples.iter().enumerate() {
            let path = format!("samples[{k}]");
            if s.world_se3_frame.len() != self.frames.len() {
                issues.push(
                    format!("{path}.world_se3_frame"),
                    format!(
                        "{} poses for {} frames",
                        s.world_se3_frame.len(),
                        self.frames.len()
                    ),
                );
            }
            if s.joint_positions.len() != self.robots.len() {
                issues.push(
                    format!("{path}.joint_positions"),
                    format!(
                        "{} joint vectors for {} robots",
                        s.joint_positions.len(),
                        self.robots.len()
                    ),
                );
            } else {
                for (r, (q, robot)) in s.joint_positions.iter().zip(&self.robots).enumerate() {
                    if q.len() != robot.joint_names.len() {
                        issues.push(
                            format!("{path}.joint_positions[{r}]"),
                            format!("{} values for {} joints", q.len(), robot.joint_names.len()),
                        );
                    }
                }
            }
            if !(s.t.is_finite() && s.t > last_t) {
                issues.push(
                    format!("{path}.t"),
                    "times must be finite and strictly increasing",
                );
            }
            last_t = s.t;
            if let Some(c) = &s.capture {
                if captures.contains(&c.id.as_str()) {
                    issues.push(
                        format!("{path}.capture.id"),
                        format!("duplicate capture id `{}`", c.id),
                    );
                }
                captures.push(&c.id);
            }
        }
        issues.finish()
    }

    /// Indices of the capture samples, in time order.
    pub fn capture_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.samples
            .iter()
            .enumerate()
            .filter(|(_, s)| s.capture.is_some())
            .map(|(k, _)| k)
    }
}
