//! P20d: etendue's stripe detector as calibration-rs's laser-line extractor, on images drawn from
//! an in-tree scene's analytic stripes. A real laser image is the stripe on the board's dim light
//! with no board image beside it, so each image here is that: the stripe of `laser_ur5e`'s ground
//! truth over a dim checkerboard with a little noise. The extractor must find the stripe and
//! nothing of the board.

use std::path::{Path, PathBuf};

use etendue_cli::detect::laser::{STRIPE_EXTRACTOR, StripeExtractor, analytic_stripe, check};
use etendue_cli::{bake_file, gt, load};
use etendue_kinematics::BakeOptions;
use etendue_synth::dataset::GroundTruth;
use vision_calibration_dataset::{LaserExtractionSpec, LaserScanAxis};
use vision_calibration_detect::GrayImageView;
use vision_calibration_pipeline::dataset_runner::LaserPixelExtractor;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `laser_ur5e`'s ground truth, written under `target/`.
fn truth() -> GroundTruth {
    let dir = root().join("target/laser_extractor_test");
    std::fs::create_dir_all(&dir).unwrap();
    let text = std::fs::read_to_string(root().join("examples/laser_ur5e/scene.json")).unwrap();
    let mut scene: etendue_scene::SceneSpec = serde_json::from_str(&text).unwrap();
    for r in &mut scene.robots {
        r.manifest = root()
            .join("examples/laser_ur5e")
            .join(&r.manifest)
            .to_string_lossy()
            .into_owned();
    }
    let path = dir.join("scene.json");
    std::fs::write(&path, serde_json::to_string(&scene).unwrap()).unwrap();
    let loaded = load(&path).unwrap();
    let scenario = root().join("examples/laser_ur5e/scenario.json");
    let baked = bake_file(&loaded, &scenario, BakeOptions::default()).unwrap();
    gt::write(&loaded, &baked, &dir.join("gt")).unwrap();
    serde_json::from_str(&std::fs::read_to_string(dir.join("gt/gt.json")).unwrap()).unwrap()
}

/// A deterministic noise value in `[-2, 2]` DN for pixel `k`.
fn noise(k: usize) -> f64 {
    let h = (k as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 40;
    (h % 5) as f64 - 2.0
}

/// A `width × height` image: a dim checkerboard (3 and 14 DN, 40 px squares), noise, and a
/// Gaussian stripe (σ 1.2 px, 150 DN) along the polylines `runs`.
fn image(width: usize, height: usize, runs: &[Vec<[f64; 2]>]) -> Vec<u8> {
    let mut stripe = vec![0.0f64; width * height];
    let sigma = 1.2;
    for run in runs {
        for seg in run.windows(2) {
            let (a, b) = (seg[0], seg[1]);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let len2 = dx * dx + dy * dy;
            let lo = |u: f64, v: f64| (u.min(v) - 6.0).floor().max(0.0) as usize;
            let hi = |u: f64, v: f64, n: usize| ((u.max(v) + 6.0).ceil() as usize).min(n - 1);
            for y in lo(a[1], b[1])..=hi(a[1], b[1], height) {
                for x in lo(a[0], b[0])..=hi(a[0], b[0], width) {
                    let (px, py) = (x as f64 - a[0], y as f64 - a[1]);
                    let t = if len2 > 0.0 {
                        ((px * dx + py * dy) / len2).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let d2 = (px - t * dx).powi(2) + (py - t * dy).powi(2);
                    let v = 150.0 * (-d2 / (2.0 * sigma * sigma)).exp();
                    let k = y * width + x;
                    stripe[k] = stripe[k].max(v);
                }
            }
        }
    }
    (0..width * height)
        .map(|k| {
            let (x, y) = (k % width, k / width);
            let board = if ((x / 40) + (y / 40)) % 2 == 0 {
                14.0
            } else {
                3.0
            };
            (board + noise(k) + stripe[k]).round().clamp(0.0, 255.0) as u8
        })
        .collect()
}

#[test]
fn the_extractor_finds_the_stripe_and_nothing_of_the_board() {
    let gt = truth();
    assert_eq!(StripeExtractor.name(), STRIPE_EXTRACTOR);
    let mut views = 0;
    for capture in &gt.captures {
        for view in &capture.views {
            let runs = analytic_stripe(view);
            let along: f64 = runs
                .iter()
                .flat_map(|r| r.windows(2))
                .map(|s| (s[1][0] - s[0][0]).abs())
                .sum();
            let across: f64 = runs
                .iter()
                .flat_map(|r| r.windows(2))
                .map(|s| (s[1][1] - s[0][1]).abs())
                .sum();
            if along + across < 100.0 {
                continue; // too little stripe in view
            }
            let camera = gt.cameras.iter().find(|c| c.id == view.camera).unwrap();
            let [w, h] = camera.resolution;
            let pixels = image(w as usize, h as usize, &runs);
            let spec = LaserExtractionSpec {
                scan_axis: if along >= across {
                    LaserScanAxis::Cols
                } else {
                    LaserScanAxis::Rows
                },
                ..LaserExtractionSpec::default()
            };
            let found = StripeExtractor
                .extract(GrayImageView::new(w, h, &pixels).unwrap(), &spec)
                .unwrap();
            let c = check(&found, &runs);
            // One centre per scan line along the stripe, on it: the board's squares bias the
            // centres a little where their edges cross the stripe, and at most 1 % of them (where
            // the stripe ends or breaks) are off it.
            let lines = along.max(across);
            assert!(
                c.outliers * 100 <= c.points && c.rms_px < 0.2,
                "{} {}: {c:?}",
                capture.id,
                view.camera
            );
            assert!(
                c.points as f64 > 0.9 * lines,
                "{} {}: {} of {lines}",
                capture.id,
                view.camera,
                c.points
            );
            views += 1;
        }
    }
    assert!(views >= 3, "only {views} views with a stripe");
}

#[test]
fn a_board_without_a_stripe_gives_no_pixels() {
    let (w, h) = (320, 240);
    let pixels = image(w, h, &[]);
    let found = StripeExtractor
        .extract(
            GrayImageView::new(w as u32, h as u32, &pixels).unwrap(),
            &LaserExtractionSpec::default(),
        )
        .unwrap();
    assert!(found.is_empty(), "{} pixels on a bare board", found.len());
}
