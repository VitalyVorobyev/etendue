//! A real dataset's calibration: calibration-rs's dataset runner, its detections cached under the
//! output folder.

use std::path::Path;

use anyhow::{Context, Result};
use vision_calibration_core::RunControl;
use vision_calibration_dataset::DatasetSpec;
use vision_calibration_detect::FsDetectionCache;
use vision_calibration_pipeline::dataset_runner::{Detection, calibrate_dataset};

use super::{DatasetSource, report};
use crate::calibrate::Run;
use crate::detect::laser::StripeExtractor;

/// The detection cache's folder in the output folder: never in the dataset's, which may be
/// read-only or shared.
pub const DETECTIONS: &str = ".detections";

/// Calibrate the real dataset `source` with calibration-rs's dataset runner: the manifest's
/// topology picks the workflow, run with its default configuration. A laser topology's stripes are
/// found by etendue's stripe detector ([`StripeExtractor`], P20d). Detections and stripe pixels are
/// cached in `out`/[`DETECTIONS`], so a second run detects nothing again.
///
/// # Errors
///
/// An unreadable manifest or upstream calibration, or a failed detection, extraction, pairing or
/// solve.
pub fn calibrate(source: &DatasetSource, out: &Path) -> Result<Run> {
    let spec = DatasetSpec::from_path(source.manifest())
        .with_context(|| format!("reading {}", source.manifest))?;
    let base = source.base_dir();
    let cache = FsDetectionCache::new(out.join(DETECTIONS));
    let run = calibrate_dataset(
        &spec,
        &base,
        None,
        Detection::Cached(&cache),
        Some(&StripeExtractor),
        &RunControl::default(),
    )
    .with_context(|| format!("calibrating {}", source.name()))?;
    Ok(Run {
        reports: vec![report(&spec, &base, &run)?],
        row_stats: None,
        param_stats: None,
    })
}
