//! Progress and cancellation of the long pipeline steps (render, detect), so
//! the CLI prints them and the studio streams them to its UI.

use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

/// A pipeline step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// Analytic ground truth (`etendue gt`).
    GroundTruth,
    /// Blender renders the canonical images.
    Render,
    /// EXRs are resampled through the remap LUT into PNGs.
    Resample,
    /// Corner detection (`etendue detect`).
    Detect,
}

/// One progress event.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Progress {
    /// `done` of `total` items of `stage` are finished.
    Step {
        /// The step.
        stage: Stage,
        /// Items finished.
        done: usize,
        /// Items in the step.
        total: usize,
    },
    /// A line for a log: a summary, a warning, or Blender's own output.
    Log {
        /// The line, without its newline.
        line: String,
    },
}

/// The run was cancelled through its [`Control`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("cancelled")
    }
}

impl std::error::Error for Cancelled {}

/// Where a step reports progress and learns it is cancelled.
pub struct Control<'a> {
    report: &'a (dyn Fn(Progress) + Sync),
    cancel: Option<&'a AtomicBool>,
}

impl<'a> Control<'a> {
    /// Report through `report`; cancelled once `cancel` is set.
    pub fn new(report: &'a (dyn Fn(Progress) + Sync), cancel: Option<&'a AtomicBool>) -> Self {
        Self { report, cancel }
    }

    /// Send one event.
    pub fn report(&self, progress: Progress) {
        (self.report)(progress);
    }

    /// Send a log line.
    pub fn log(&self, line: impl Into<String>) {
        self.report(Progress::Log { line: line.into() });
    }

    /// Send a step count.
    pub fn step(&self, stage: Stage, done: usize, total: usize) {
        self.report(Progress::Step { stage, done, total });
    }

    /// Whether the run has been cancelled.
    pub fn cancelled(&self) -> bool {
        self.cancel.is_some_and(|c| c.load(Ordering::Relaxed))
    }

    /// `Err(Cancelled)` once the run has been cancelled.
    ///
    /// # Errors
    ///
    /// [`Cancelled`] if the run has been cancelled.
    pub fn check(&self) -> Result<(), Cancelled> {
        if self.cancelled() {
            Err(Cancelled)
        } else {
            Ok(())
        }
    }
}

/// The CLI's reporter: log lines to stdout, step counts dropped (Blender's
/// own output already shows its progress).
pub fn print(progress: Progress) {
    if let Progress::Log { line } = progress {
        println!("{line}");
    }
}
