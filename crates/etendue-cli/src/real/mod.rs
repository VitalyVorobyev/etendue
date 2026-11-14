//! Real datasets (P20): calibration-rs's `dataset.toml` manifests of real images and robot poses.
//!
//! [`calibrate`] runs calibration-rs's dataset runner on one (detection, cached under the output
//! folder, then the manifest's workflow with its default configuration) and reports what a run
//! without truth can: residuals per feature and per view, calibration-rs's σ and quality verdict,
//! and the views the runner left out ([`RealSummary`]). The run is kept like a synthetic one
//! ([`crate::calibrate::write_real_run`]).
//!
//! [`twin`] builds a real run's twin (P20b): the same cell, robot poses and seen points,
//! simulated, calibrated by the same workflow; [`versus`] sets the real run against it.
//!
//! [`split_half_run`] checks the P18 planner on the real poses (P20c): its σ, predicted on the
//! twin, against the real calibration's σ and the scatter between calibrations of halves of the
//! real views.
//!
//! A dataset is named by a manifest path or an id ([`resolve`]): the git-ignored
//! `local/datasets.toml` maps ids to manifests, so a private dataset's path never leaves the
//! machine.

mod report;
mod resolve;
mod run;
mod split_half;
mod splits;
mod twin;
mod versus;

pub use report::{RealSummary, RejectedView, ViewResiduals, report};
pub use resolve::{DatasetSource, LOCAL_DATASETS, PUBLIC, resolve};
pub use run::{DETECTIONS, calibrate};
pub use split_half::{
    GroupRow, P1, ParamRow, SplitHalf, SplitHalfOptions, SplitRow, planner_procedure, split_half,
    split_half_run,
};
pub use splits::{Split, splits};
pub use twin::{Twin, twin};
pub use versus::{CameraVersus, MIN_BIN, ParamVersus, Versus, Whiteness, versus, whiteness};
