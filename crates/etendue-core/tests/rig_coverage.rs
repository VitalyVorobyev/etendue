//! Rig coverage (P11): gate G11.2 and the head-unit counts.

use etendue_core::rig::{Criteria, Grid, coverage};
use etendue_scene::RigLayout;
use serde_json::json;

fn pinhole(fx: f64, w: u32, h: u32) -> serde_json::Value {
    json!({
        "params": {
            "projection": {"type": "pinhole"}, "distortion": {"type": "none"},
            "sensor": {"type": "identity"},
            "intrinsics": {"type": "fx_fy_cx_cy_skew", "fx": fx, "fy": fx,
                           "cx": f64::from(w) / 2.0, "cy": f64::from(h) / 2.0, "skew": 0.0}
        },
        "resolution": [w, h]
    })
}

/// G11.2: a parallel pinhole stereo pair (baseline B, fields a·z by c·z) sees, between depths
/// z0 and z1, the volume ∫ (a·z − B)·c·z dz.
#[test]
fn g11_2_parallel_stereo_overlap_is_the_frusta_intersection() {
    let layout: RigLayout = serde_json::from_value(json!({
        "count": 2, "radius_m": 0.05, "working_distance_m": 0.5, "tilt_rad": 0.0,
        "image_x": "common", "camera": pinhole(1000.0, 1000, 800)
    }))
    .unwrap();
    let (a, c, b) = (1.0, 0.8, 0.1);
    let (z0, z1) = (0.2_f64, 0.6_f64);
    let antiderivative = |z: f64| c * (a * z.powi(3) / 3.0 - b * z * z / 2.0);
    let exact = antiderivative(z1) - antiderivative(z0);
    let grid = Grid {
        min: [-0.3, -0.24, z0],
        max: [0.3, 0.24, z1],
        n: [128, 128, 128],
    };
    let cov = coverage(&layout, &grid, &Criteria::default()).unwrap();
    let measured = cov.volume_m3(2, 0);
    let error = (measured - exact).abs() / exact;
    println!(
        "G11.2: overlap {measured:.6} m³ against {exact:.6} m³ ({:.3} %)",
        100.0 * error
    );
    assert!(error < 0.02, "{error}");
    assert_eq!((cov.cameras, cov.pairs), (2, 0));
}

/// A ring of three heads, each with its laser on its camera's body aimed at the working point:
/// the working point is seen by every camera and measured by every pair.
#[test]
fn head_units_measure_at_their_working_point() {
    let mut camera = pinhole(1.0, 1936, 1216);
    camera["optics"] = json!({
        "sensor": {"pixel_pitch_m": 5.86e-6},
        "lens": {"focal_length_m": 0.025, "f_number": 8.0},
        "focus_distance_m": 0.35
    });
    // The laser 100 mm off the camera axis toward −y, aimed at the axis at 350 mm, its line along
    // camera x.
    let central = nalgebra::Vector3::new(0.0, 0.1, 0.35).normalize();
    let line = nalgebra::Vector3::x();
    let q = nalgebra::UnitQuaternion::from_rotation_matrix(
        &nalgebra::Rotation3::from_basis_unchecked(&[line.cross(&central), line, central]),
    );
    let pose = nalgebra::Isometry3::from_parts(nalgebra::Translation3::new(0.0, -0.1, 0.0), q);
    let layout: RigLayout = serde_json::from_value(json!({
        "count": 3, "radius_m": 0.08, "working_distance_m": 0.35,
        "camera": camera,
        "lasers": {"camera_se3_laser": pose, "fan_half_angle": 0.3, "fan_length": 1.0,
                   "wavelength_nm": 660.0, "beam_waist_m": 5e-4}
    }))
    .unwrap();
    let cov = coverage(
        &layout,
        &Grid {
            min: [-0.15, -0.15, 0.25],
            max: [0.15, 0.15, 0.45],
            n: [48, 48, 32],
        },
        &Criteria {
            max_blur_px: 3.0,
            ..Criteria::default()
        },
    )
    .unwrap();
    assert_eq!((cov.cameras, cov.pairs), (3, 3));
    let max_views = *cov.views.iter().max().unwrap();
    let max_lit = *cov.lit.iter().max().unwrap();
    assert_eq!(max_views, 3);
    assert!(max_lit >= 1);
    // Every measured voxel is seen by the measuring camera.
    assert!(cov.views.iter().zip(&cov.lit).all(|(v, l)| v >= l));
    assert!(cov.volume_m3(0, 1) > 0.0 && cov.volume_m3(3, 0) > 0.0);
    let count = |f: &dyn Fn(u8, u8) -> bool| {
        cov.views
            .iter()
            .zip(&cov.lit)
            .filter(|&(&v, &l)| f(v, l))
            .count()
    };
    println!(
        "voxels: seen by 1+ {}, by 3 {}, lit 1+ {}, lit 2+ {}, both {}",
        count(&|v, _| v >= 1),
        count(&|v, _| v >= 3),
        count(&|_, l| l >= 1),
        count(&|_, l| l >= 2),
        count(&|v, l| v >= 3 && l >= 1)
    );
    println!(
        "ring of 3 heads: seen by 3 {:.1} cm³, measured {:.2} cm³",
        1e6 * cov.volume_m3(3, 0),
        1e6 * cov.volume_m3(0, 1)
    );
}
