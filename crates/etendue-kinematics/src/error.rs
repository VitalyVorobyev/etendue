//! Error type of `etendue-kinematics`.

use etendue_scene::ValidationError;

/// Errors returned by `etendue-kinematics`.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The URDF could not be parsed.
    #[error("URDF parse error: {0}")]
    Urdf(String),
    /// The URDF and/or manifest do not describe a supported robot.
    #[error("invalid robot model: {0}")]
    InvalidModel(String),
    /// A spec document failed validation.
    #[error(transparent)]
    Validation(#[from] ValidationError),
    /// A joint vector has the wrong length or leaves the joint limits.
    #[error("invalid joint vector: {0}")]
    InvalidJoints(String),
    /// Inverse kinematics did not converge.
    #[error("inverse kinematics failed: {0}")]
    IkFailed(String),
    /// A scenario step cannot be executed.
    #[error("scenario step {step}: {message}")]
    Scenario {
        /// Index of the step in `ScenarioSpec::steps`.
        step: usize,
        /// What went wrong.
        message: String,
    },
}

/// Result alias for `etendue-kinematics`.
pub type Result<T> = std::result::Result<T, Error>;
