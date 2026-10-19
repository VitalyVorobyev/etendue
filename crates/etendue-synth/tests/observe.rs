//! P13: observations synthesised from ground truth are exact without noise, the same for the same
//! seed, and noisy as asked.

use std::path::Path;

use etendue_kinematics::{RobotModel, bake};
use etendue_scene::{RobotManifest, ScenarioSpec, SceneSpec};
use etendue_synth::PixelCentre;
use etendue_synth::dataset::{EmitOptions, GroundTruth, emit};
use etendue_synth::gt::VisibilitySpec;
use etendue_synth::observe::{ObservationNoise, observe};

fn read(path: &str) -> String {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    std::fs::read_to_string(root.join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The ground truth of `examples/laser_ur5e`: two cameras and a laser on a UR5e.
fn laser_gt() -> GroundTruth {
    let scene: SceneSpec = serde_json::from_str(&read("examples/laser_ur5e/scene.json")).unwrap();
    let scenario: ScenarioSpec =
        serde_json::from_str(&read("examples/laser_ur5e/scenario.json")).unwrap();
    let manifest: RobotManifest =
        serde_json::from_str(&read("assets/robots/ur5e/robot.json")).unwrap();
    let model =
        RobotModel::from_urdf_str(&read("assets/robots/ur5e/robot.urdf"), &manifest).unwrap();
    let baked = bake(&scene, &scenario, &[model]).unwrap();
    let points = etendue_synth::board::layout(&scene.targets[0].geometry)
        .unwrap()
        .unwrap()
        .points;
    let options = EmitOptions {
        visibility: VisibilitySpec { margin_px: 2.0 },
        pixel_centre: PixelCentre::Integer,
        laser_spacing_m: 1e-3,
    };
    emit(&scene, &baked, &[manifest], &points, &options)
        .unwrap()
        .gt
}

#[test]
fn without_noise_the_observations_are_the_truth() {
    let gt = laser_gt();
    let obs = observe(&gt, &ObservationNoise::default());
    assert_eq!(obs.captures.len(), gt.captures.len());
    let mut points = 0;
    let mut stripes = 0;
    for (c, truth) in obs.captures.iter().zip(&gt.captures) {
        assert_eq!(c.base_se3_tcp, truth.base_se3_tcp);
        for (v, tv) in c.views.iter().zip(&truth.views) {
            let seen: Vec<_> = tv.points.iter().filter(|p| p.visible()).collect();
            if seen.len() >= 4 {
                assert_eq!(v.points.len(), seen.len());
                for (o, t) in v.points.iter().zip(seen) {
                    assert_eq!(o.point, t.point);
                    assert_eq!(o.pixel, t.pixel.unwrap());
                }
            }
            points += v.points.len();
            stripes += usize::from(!v.laser.is_empty());
        }
    }
    assert!(
        points > 1000 && stripes > 10,
        "{points} points, {stripes} stripes"
    );
}

#[test]
fn noise_is_seeded_and_has_the_asked_spread() {
    let gt = laser_gt();
    let noise = ObservationNoise {
        pixel_sigma_px: 0.5,
        laser_sigma_px: 0.2,
        robot_rot_sigma_rad: 1e-3,
        robot_trans_sigma_m: 1e-4,
        seed: 7,
        ..ObservationNoise::default()
    };
    let a = observe(&gt, &noise);
    assert_eq!(a, observe(&gt, &noise), "same seed, same observations");
    assert_ne!(a, observe(&gt, &ObservationNoise { seed: 8, ..noise }));
    let exact = observe(&gt, &ObservationNoise::default());
    let (mut sum, mut n) = (0.0, 0usize);
    let (mut rot, mut trans, mut m) = (0.0, 0.0, 0usize);
    for (c, e) in a.captures.iter().zip(&exact.captures) {
        let (p, q) = (c.base_se3_tcp.unwrap(), e.base_se3_tcp.unwrap());
        rot += p.rotation.angle_to(&q.rotation).powi(2);
        trans += (p.translation.vector - q.translation.vector).norm_squared();
        m += 1;
        for (v, ve) in c.views.iter().zip(&e.views) {
            for (o, x) in v.points.iter().zip(&ve.points) {
                sum += (o.pixel[0] - x.pixel[0]).powi(2) + (o.pixel[1] - x.pixel[1]).powi(2);
                n += 2;
            }
        }
    }
    let sigma = (sum / n as f64).sqrt();
    assert!((sigma - 0.5).abs() < 0.02, "pixel σ {sigma} from {n} draws");
    // Three components each: the angle's RMS is σ√3, the translation's too.
    let rot_rms = (rot / m as f64).sqrt() / 3f64.sqrt();
    let trans_rms = (trans / m as f64).sqrt() / 3f64.sqrt();
    assert!((rot_rms / 1e-3 - 1.0).abs() < 0.35, "rotation σ {rot_rms}");
    assert!(
        (trans_rms / 1e-4 - 1.0).abs() < 0.35,
        "translation σ {trans_rms}"
    );
}

#[test]
fn dropouts_and_outliers_take_their_share() {
    let gt = laser_gt();
    let exact = observe(&gt, &ObservationNoise::default());
    let total: usize = exact
        .captures
        .iter()
        .flat_map(|c| &c.views)
        .map(|v| v.points.len())
        .sum();
    let obs = observe(
        &gt,
        &ObservationNoise {
            dropout_fraction: 0.2,
            outlier_fraction: 0.05,
            outlier_sigma_px: 20.0,
            seed: 3,
            ..ObservationNoise::default()
        },
    );
    let points: Vec<_> = obs
        .captures
        .iter()
        .flat_map(|c| &c.views)
        .flat_map(|v| &v.points)
        .collect();
    let kept = points.len() as f64 / total as f64;
    let outliers = points.iter().filter(|p| p.outlier).count() as f64 / points.len() as f64;
    assert!((kept - 0.8).abs() < 0.03, "kept {kept}");
    assert!((outliers - 0.05).abs() < 0.015, "outliers {outliers}");
}
