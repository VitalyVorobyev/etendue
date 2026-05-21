//! Typed error enum for `etendue-core`.
//!
//! Mirrors the error convention of `vision-calibration-core` so that callers
//! moving between the calibration kernel and the optical-design kernel see a
//! consistent error vocabulary.

/// Errors returned by public APIs in `etendue-core`.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Input data is invalid (e.g. a non-positive focal length, a degenerate
    /// pose, or a mesh with mismatched index/vertex counts).
    #[error("invalid input: {reason}")]
    InvalidInput {
        /// Human-readable description of what was invalid.
        reason: String,
    },

    /// Not enough data to proceed (e.g. fewer scene entities than an
    /// analysis requires).
    #[error("insufficient data: need {need}, got {got}")]
    InsufficientData {
        /// Minimum count the operation requires.
        need: usize,
        /// Count that was actually supplied.
        got: usize,
    },

    /// A matrix inversion or decomposition produced a degenerate result.
    #[error("singular matrix or degenerate configuration")]
    Singular,

    /// A numerical operation failed unexpectedly.
    #[error("numerical failure: {0}")]
    Numerical(String),

    /// A file I/O or parse error — wraps the underlying error message as a
    /// `String` so callers do not depend on `std::io::Error` or
    /// `serde_json::Error` directly.
    #[error("I/O or parse error: {0}")]
    Io(String),
}

/// Result type used throughout `etendue-core`.
pub type Result<T> = std::result::Result<T, Error>;
