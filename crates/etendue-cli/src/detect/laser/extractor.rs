//! etendue's stripe detector as calibration-rs's laser-line extractor (P20d): what the dataset
//! runner calls on every laser image of a laser manifest (`etendue real calibrate`).
//!
//! The runner reads, crops to the camera's ROI and caches; the extractor only finds the stripe.
//! A real laser image has no board image beside it, so the stripe is taken as it is: the
//! [`extract_along`] steps without the board's subtraction and division, then only the centres
//! on long continuous chains ([`chained`]): beyond the stripe's ends a column's brightest pixel
//! is the board's dim light. The manifest's scan axis fixes the stripe's direction (`cols`: one
//! centre per column). The manifest's `sigma` and
//! thresholds are the reference extractor's edge detector's (vision-metrology's), and this one has
//! none: it fits a Gaussian across the stripe instead.

use etendue_synth::images::Gray8;
use vision_calibration_dataset::{LaserExtractionSpec, LaserScanAxis};
use vision_calibration_detect::GrayImageView;
use vision_calibration_pipeline::dataset_runner::LaserPixelExtractor;

use super::{chained, extract_along};

/// The extractor's name, part of the runner's cache key (`laser:<name>`): a change to what it
/// finds bumps the version, so cached pixels are not served for it.
pub const STRIPE_EXTRACTOR: &str = "etendue-stripe-v1";

/// etendue's stripe detector behind calibration-rs's [`LaserPixelExtractor`].
#[derive(Clone, Copy, Debug, Default)]
pub struct StripeExtractor;

impl LaserPixelExtractor for StripeExtractor {
    fn name(&self) -> &str {
        STRIPE_EXTRACTOR
    }

    fn extract(
        &self,
        image: GrayImageView<'_>,
        spec: &LaserExtractionSpec,
    ) -> Result<Vec<[f64; 2]>, Box<dyn std::error::Error + Send + Sync>> {
        let gray = Gray8 {
            width: image.width(),
            height: image.height(),
            data: image.pixels().to_vec(),
        };
        let along_x = matches!(spec.scan_axis, LaserScanAxis::Cols);
        let centres = extract_along(&gray, None, Some(along_x));
        Ok(chained(&centres, along_x)
            .into_iter()
            .map(|c| c.pixel)
            .collect())
    }
}
