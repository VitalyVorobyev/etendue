//! Real datasets (P20): calibration-rs's `dataset.toml` manifests of real images and robot poses.
//!
//! [`calibrate`] runs calibration-rs's dataset runner on one (detection, cached under the output
//! folder, then the manifest's workflow with its default configuration) and reports what a run
//! without truth can: residuals per feature and per view, calibration-rs's σ and quality verdict,
//! and the views the runner left out ([`RealSummary`]). The run is kept like a synthetic one
//! ([`crate::calibrate::write_real_run`]).
//!
//! A dataset is named by a manifest path or an id ([`resolve`]): the git-ignored
//! `local/datasets.toml` maps ids to manifests, so a private dataset's path never leaves the
//! machine.

mod report;
mod resolve;
mod run;

pub use report::{RealSummary, RejectedView, ViewResiduals, report};
pub use resolve::{DatasetSource, LOCAL_DATASETS, PUBLIC, resolve};
pub use run::{DETECTIONS, calibrate};
