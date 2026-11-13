//! Native calibration of a dataset's observations, compared with its ground truth (P13).
//!
//! This is `tools/closed-loop/closed_loop.py` in Rust, on the same calibration-rs workflows
//! (`vision-calibration-pipeline`) with the same configuration and the same comparison rows:
//! - **single-camera hand-eye**: one camera on the robot (or `camera` picks one of a rig);
//! - **rig hand-eye**: several cameras on one rig;
//! - **rig hand-eye laserline**: a rig with lasers (seeded from nominal values when a camera is
//!   Scheimpflug, as the script does);
//! - **laserline device**: one camera carrying its laser (a triangulation head), with the
//!   hand-eye solved on the same views by the single-camera workflow.
//!
//! Eye-to-hand datasets are calibrated too (the script handles eye-in-hand only).
//!
//! The observations are [`Observations`]: the ground truth's own with controlled noise
//! (`etendue_synth::observe`), or a detector's ([`from_features`]). The rows are gated by G5.1
//! (and G5.3 with lasers) only on exact analytic observations, as the script gates them.

mod compare;
mod observations;
mod report;
mod rig;
mod runs;
mod stats;
mod truth;
mod upstream;
mod workflows;

pub(crate) use compare::{with_planes, with_triangulation};
pub use observations::from_features;
pub(crate) use observations::{check_observations, robot_poses};
pub use report::{
    Estimate, EstimatedCamera, FeatureResidual, Offsets, Plane, Predicted, Report, Residuals, Row,
    StageSummary, Workflow,
};
pub(crate) use rig::{rig, rig_extrinsics, rig_laser};
pub use runs::{
    KeptRuns, RUN_CONFIG, RUN_REPORT, RUN_RESULT, RUN_VERSION, Run, RunConfig, RunOptions,
    RunProcedure, StoredRun, TWIN_GT, UnreadableRun, read_detected, read_runs, run_dir, run_trials,
    write_real_run, write_run, write_twin_run,
};
pub use stats::{ParamStats, RowStats, param_stats, row_stats};
pub(crate) use truth::{Truth, distortion_of, nominal_values, tilt_of};
pub(crate) use upstream::{
    SeedValues, StageRun, camera_of, check_config, exported, handeye_mode, predicted, residuals,
};
pub(crate) use workflows::{head, intrinsics, single};

use anyhow::Result;
use etendue_synth::dataset::GroundTruth;
use etendue_synth::observe::Observations;

/// Report version.
pub const REPORT_VERSION: u32 = 3;

/// G5.1's limits (noise-free analytic input), as `closed_loop.py` has them.
pub mod gate {
    pub const MEAN_REPROJ_PX: f64 = 0.05;
    pub const FOCAL_REL: f64 = 1e-4;
    pub const PRINCIPAL_PX: f64 = 0.05;
    pub const HANDEYE_ROT_DEG: f64 = 0.01;
    pub const HANDEYE_TRANS_MM: f64 = 0.05;
    /// G5.3.
    pub const PLANE_NORMAL_DEG: f64 = 0.01;
    pub const PLANE_DISTANCE_MM: f64 = 0.05;
    /// G5.3c: the largest error of a stripe point triangulated in the robot's frame.
    pub const TRIANGULATION_MM: f64 = 0.05;
}

/// What to calibrate.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Calibrate this camera of a rig alone (single-camera hand-eye).
    pub camera: Option<String>,
    /// The observations are a detector's ([`from_features`]), so no gate applies.
    pub detected: bool,
    /// The procedure to run (its id and stages); `None` for `auto`'s choice by the dataset's shape.
    pub procedure: Option<(String, etendue_scene::ProcedurePart)>,
}

/// Calibrate `obs` of `gt`'s dataset with the procedure the dataset's shape calls for (`auto`,
/// [`crate::procedure::auto`]) and compare with the truth. Rows are gated by G5.1 (G5.3) on the
/// exact analytic observations only.
///
/// # Errors
///
/// When the dataset has no robot, the observations do not match the truth's captures, or a
/// calibration-rs workflow fails.
pub fn calibrate(gt: &GroundTruth, obs: &Observations, options: &Options) -> Result<Report> {
    let procedure = match &options.procedure {
        Some(p) => p.clone(),
        None => crate::procedure::auto(gt, options)?,
    };
    crate::procedure::run(gt, obs, options, &procedure.0, &procedure.1)
}
