//! Gate G3.1 (docs/pivot/PLAN.md P3-1): on full images of every supported
//! camera model, including Scheimpflug tilts up to 6°, the target model's
//! `project(unproject(u))` returns `u` to ≤ 1e-6 px at random pixels; and the
//! remap LUT built on it sends every target pixel to a canonical coordinate
//! that maps back to that pixel (to the LUT's float32 resolution).
//!
//! Undistortion uses calibration-rs's default iteration caps; since 0.8.2
//! (calibration-rs#120, Newton) every gate camera converges at the corners.
//! etendue does not override `iters` (ADR 0004).
//!
//! Full report: `cargo run --release -p etendue-synth --example g3_1_remap`.

use etendue_synth::gate::{RESOLUTION, Rng, cameras};
use etendue_synth::{CanonicalCamera, CanonicalSpec, PixelCentre, remap_lut};
use nalgebra::Point2;
use vision_calibration_core::{CameraModel, CameraParams};

const GATE_PX: f64 = 1e-6;
const SAMPLES: usize = 10_000;

fn round_trip(model: &CameraModel, u: Point2<f64>) -> f64 {
    let ray = model.backproject_pixel(&u).point;
    let back = model
        .project_point_c(&ray)
        .expect("a back-projected ray images");
    (back - u).norm()
}

fn max_round_trip(params: &CameraParams) -> f64 {
    let model = params.build().unwrap();
    let [w, h] = RESOLUTION.map(f64::from);
    let edge = PixelCentre::Integer.edge();
    let mut rng = Rng::new(0x6031);
    let corners = [
        (0.0, 0.0),
        (w, 0.0),
        (0.0, h),
        (w, h),
        (w / 2.0, 0.0),
        (0.0, h / 2.0),
    ];
    corners
        .into_iter()
        .map(|(u, v)| Point2::new(edge + u, edge + v))
        .chain(
            (0..SAMPLES).map(|_| Point2::new(edge + rng.next_f64() * w, edge + rng.next_f64() * h)),
        )
        .map(|u| round_trip(&model, u))
        .fold(0.0, f64::max)
}

#[test]
fn g3_1_project_unproject_round_trip() {
    let mut failures = Vec::new();
    for (name, params) in cameras() {
        let worst = max_round_trip(&params);
        println!("{name:<24} {worst:.3e} px");
        if worst > GATE_PX {
            failures.push(format!("{name}: {worst:.3e} px"));
        }
    }
    assert!(
        failures.is_empty(),
        "G3.1 exceeded {GATE_PX} px: {failures:?}"
    );
}

#[test]
fn g3_1_lut_maps_back_to_the_target_pixel() {
    // s = 4: canonical coordinate → canonical ray → target pixel returns the
    // pixel to the float32 resolution of the LUT entry.
    for (name, params) in cameras() {
        let cam = CanonicalCamera::cover(
            &params,
            RESOLUTION,
            &CanonicalSpec {
                supersample: 4.0,
                ..CanonicalSpec::default()
            },
            PixelCentre::Integer,
        )
        .unwrap();
        let target = params.build().unwrap();
        let render = cam.model().unwrap();
        let lut = remap_lut(&params, RESOLUTION, &cam).unwrap();
        let [cw, ch] = cam.resolution.map(f64::from);
        // f32 spacing at the largest canonical coordinate, in target pixels.
        let tolerance = 2.0 * f64::from(f32::EPSILON) * cw.max(ch) / 4.0 + GATE_PX;
        let mut worst = 0.0_f64;
        for j in (0..RESOLUTION[1]).step_by(8) {
            for i in (0..RESOLUTION[0]).step_by(8) {
                let [u, v] = lut.get(i, j).unwrap();
                let (u, v) = (f64::from(u), f64::from(v));
                assert!(
                    u >= -0.5 && u <= cw - 0.5 && v >= -0.5 && v <= ch - 0.5,
                    "{name}: ({u}, {v}) outside the canonical image"
                );
                let ray = render.backproject_pixel(&Point2::new(u, v)).point;
                let back = target.project_point_c(&ray).unwrap();
                worst = worst.max((back - Point2::new(f64::from(i), f64::from(j))).norm());
            }
        }
        println!("{name:<24} LUT round trip {worst:.3e} px (f32 tolerance {tolerance:.1e})");
        assert!(
            worst <= tolerance,
            "{name}: LUT round trip {worst:e} px > {tolerance:e}"
        );
    }
}
