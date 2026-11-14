//! The calibrations a split-half check compares: every view's, and each split's halves'. They do
//! not depend on the planner's noise model, so one set is judged against several
//! ([`super::judge`]).

use anyhow::{Context, Result};
use etendue_synth::dataset::GroundTruth;
use etendue_synth::observe::Observations;

use super::SplitHalfOptions;
use super::planner_procedure;
use crate::calibrate::{Report, RunOptions, run_trials};
use crate::progress::Control;
use crate::real::splits::{Split, splits};

/// Every view's calibration and each split's halves', with the procedure they ran.
#[derive(Clone, Debug)]
pub struct SplitCalibrations {
    /// The calibration of every view.
    pub full: Report,
    /// Each split whose halves both calibrated, with their reports (`a`'s, then `b`'s).
    pub splits: Vec<(Split, Report, Report)>,
    /// Splits a half of which did not calibrate, with why.
    pub failed: Vec<String>,
}

/// Calibrates every view of `obs` (observations of `gt`'s captures), then each split's halves,
/// with `options`' procedure (the planner's model by default).
///
/// # Errors
///
/// When the calibration of every view fails. A split whose half fails is recorded in `failed`.
pub fn calibrate_splits(
    gt: &GroundTruth,
    obs: &Observations,
    options: &SplitHalfOptions,
    ctl: &Control,
) -> Result<SplitCalibrations> {
    let procedure = options
        .procedure
        .clone()
        .or_else(|| planner_procedure(gt.cameras.len()));
    let run_options = RunOptions {
        detected: true,
        trials: 1,
        procedure,
        ..RunOptions::default()
    };
    let calibrate = |keep: &[usize]| -> Result<Report> {
        let (g, o) = subset(gt, obs, keep);
        let mut run = run_trials(&g, Some(o), &run_options, ctl)?;
        Ok(run.reports.remove(0))
    };
    let views = gt.captures.len();
    let all: Vec<usize> = (0..views).collect();
    let full = calibrate(&all).context("calibrating every view")?;
    let mut done = Vec::new();
    let mut failed = Vec::new();
    for split in splits(views, options.random, options.seed) {
        match calibrate(&split.a).and_then(|ra| Ok((ra, calibrate(&split.b)?))) {
            Ok((ra, rb)) => done.push((split, ra, rb)),
            Err(e) => failed.push(format!("{}: {e:#}", split.name)),
        }
    }
    Ok(SplitCalibrations {
        full,
        splits: done,
        failed,
    })
}

/// `gt` and `obs` with the captures `keep` only.
fn subset(gt: &GroundTruth, obs: &Observations, keep: &[usize]) -> (GroundTruth, Observations) {
    let mut g = gt.clone();
    g.captures = keep.iter().map(|&k| gt.captures[k].clone()).collect();
    let mut o = obs.clone();
    o.captures = keep.iter().map(|&k| obs.captures[k].clone()).collect();
    (g, o)
}
