//! Gate G3.1 report: round trips per camera model, the undistortion
//! iterations needed at the image corners, canonical cameras, and LUT cost.
//!
//! ```bash
//! cargo run --release -p etendue-synth --example g3_1_remap
//! ```

use std::time::Instant;

use etendue_synth::gate::{RESOLUTION, Rng, cameras, with_iters};
use etendue_synth::{CanonicalCamera, CanonicalSpec, PixelCentre, remap_lut};
use nalgebra::Point2;
use vision_calibration_core::{CameraModel, CameraParams, DistortionParams};

const GATE_PX: f64 = 1e-6;

fn err(model: &CameraModel, u: Point2<f64>) -> f64 {
    let ray = model.backproject_pixel(&u).point;
    model
        .project_point_c(&ray)
        .map_or(f64::INFINITY, |b| (b - u).norm())
}

fn corners() -> Vec<Point2<f64>> {
    let [w, h] = RESOLUTION.map(f64::from);
    let e = PixelCentre::Integer.edge();
    vec![
        Point2::new(e, e),
        Point2::new(e + w, e),
        Point2::new(e, e + h),
        Point2::new(e + w, e + h),
    ]
}

fn random_max(model: &CameraModel) -> f64 {
    let [w, h] = RESOLUTION.map(f64::from);
    let e = PixelCentre::Integer.edge();
    let mut rng = Rng::new(0x6031);
    let mut worst = corners()
        .into_iter()
        .map(|u| err(model, u))
        .fold(0.0, f64::max);
    for _ in 0..10_000 {
        worst = worst.max(err(
            model,
            Point2::new(e + rng.next_f64() * w, e + rng.next_f64() * h),
        ));
    }
    worst
}

fn default_iters(p: &CameraParams) -> Option<u32> {
    match &p.distortion {
        DistortionParams::BrownConrady5 { params } => Some(params.iters),
        DistortionParams::Rational { params } => Some(params.iters),
        DistortionParams::ThinPrism { params } => Some(params.iters),
        _ => None,
    }
}

fn main() {
    println!(
        "G3.1 remap report ({}×{} px)\n",
        RESOLUTION[0], RESOLUTION[1]
    );
    println!(
        "{:<24} {:>6} {:>12} {:>12} {:>9}  {:>28}",
        "camera", "iters", "max err px", "corner px", "G3.1", "corner iters for ≤1e-6 px"
    );
    for (name, params) in cameras() {
        let model = params.build().unwrap();
        let random = random_max(&model);
        let corner = corners()
            .into_iter()
            .map(|u| err(&model, u))
            .fold(0.0, f64::max);
        let needed = default_iters(&params).map(|_| {
            (1..=400u32)
                .find(|&n| {
                    let m = with_iters(&params, n).unwrap().build().unwrap();
                    corners().into_iter().all(|u| err(&m, u) <= GATE_PX)
                })
                .map_or_else(
                    || {
                        let m = with_iters(&params, 400).unwrap().build().unwrap();
                        let e = corners()
                            .into_iter()
                            .map(|u| err(&m, u))
                            .fold(0.0, f64::max);
                        format!("> 400 (at 400: {e:.1e} px)")
                    },
                    |n| n.to_string(),
                )
        });
        println!(
            "{name:<24} {:>6} {random:>12.3e} {corner:>12.3e} {:>9}  {:>28}",
            default_iters(&params).map_or("-".into(), |n| n.to_string()),
            if random <= GATE_PX { "PASS" } else { "FAIL" },
            needed.unwrap_or_else(|| "closed form".into()),
        );
    }

    println!("\nCanonical cameras (margin 2 %, Integer pixel centres) and LUT cost:");
    println!(
        "{:<24} {:>5} {:>13} {:>9} {:>10}",
        "camera", "s", "canonical", "hfov °", "LUT ms"
    );
    for (name, params) in cameras() {
        for s in [1.0, 4.0] {
            let cam = CanonicalCamera::cover(
                &params,
                RESOLUTION,
                &CanonicalSpec {
                    supersample: s,
                    ..CanonicalSpec::default()
                },
                PixelCentre::Integer,
            )
            .unwrap();
            let t = Instant::now();
            let lut = remap_lut(&params, RESOLUTION, &cam).unwrap();
            let ms = t.elapsed().as_secs_f64() * 1e3;
            assert_eq!(lut.data.len(), 2 * 2048 * 1536);
            println!(
                "{name:<24} {s:>5} {:>13} {:>9.2} {ms:>10.0}",
                format!("{}×{}", cam.resolution[0], cam.resolution[1]),
                cam.horizontal_fov().to_degrees()
            );
        }
    }
}
