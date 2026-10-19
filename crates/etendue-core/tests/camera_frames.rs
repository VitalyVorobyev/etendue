//! Gates G9.1 and G9.4 (ADR 0008 §4), core part: the physical camera's closed forms against an
//! independent paraxial trace, its thin-lens limit, focus breathing in a fixed mechanical frame,
//! and agreement with the legacy defocus model.
//!
//! ```bash
//! cargo test -p etendue-core --test camera_frames -- --nocapture
//! ```

use etendue_core::calibration::{
    CameraParams, DistortionParams, FxFyCxCySkew, IntrinsicsParams, ProjectionParams, SensorParams,
};
use etendue_core::optics::camera::{Focusing, Lens, PhysicalCamera};
use etendue_core::optics::coc::coc_diameter_at_sensor;
use etendue_core::optics::paraxial::{chief_ray, rim_rays, sensor_pixel};
use nalgebra::{Point2, Point3};

const PITCH: f64 = 5.86e-6;

fn as_built() -> CameraParams {
    CameraParams {
        projection: ProjectionParams::Pinhole,
        distortion: DistortionParams::None,
        sensor: SensorParams::Identity,
        intrinsics: IntrinsicsParams::FxFyCxCySkew {
            params: FxFyCxCySkew {
                fx: 1.0,
                fy: 1.0,
                cx: 960.3,
                cy: 600.7,
                skew: 0.0,
            },
        },
    }
}

fn lens(g: f64, e: f64) -> Lens {
    Lens {
        focal_length_m: 0.025,
        f_number: 4.0,
        principal_gap_m: g,
        entrance_pupil_m: e,
        focusing: Focusing::Unit,
    }
}

fn camera(g: f64, e: f64, focus: f64, tilt: [f64; 2]) -> PhysicalCamera {
    PhysicalCamera::new(lens(g, e), PITCH, focus, tilt, &as_built()).expect("a valid camera")
}

/// A deterministic spread of object points in front of the camera, within its field.
fn points(focus: f64) -> Vec<Point3<f64>> {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 11) as f64 / (1u64 << 53) as f64
    };
    (0..200)
        .map(|_| {
            let z = focus * (0.6 + 0.8 * next());
            Point3::new((next() - 0.5) * 0.4 * z, (next() - 0.5) * 0.3 * z, z)
        })
        .collect()
}

/// The lenses and tilts every check runs over: thin, thick, crossed principal planes, a pupil
/// in front of and behind H; no tilt, one axis, both axes.
fn cases() -> Vec<(f64, f64, [f64; 2])> {
    let mut out = Vec::new();
    for (g, e) in [
        (0.0, 0.0),
        (0.008, 0.0),
        (-0.004, 0.0),
        (0.006, 0.005),
        (0.006, -0.012),
    ] {
        for tilt in [
            [0.0, 0.0],
            [0.08, 0.0],
            [0.0, -0.06],
            [0.07, 0.05],
            [-0.15, 0.1],
        ] {
            out.push((g, e, tilt));
        }
    }
    out
}

#[test]
fn projection_matches_the_chief_ray_trace() {
    let mut worst: f64 = 0.0;
    for (g, e, tilt) in cases() {
        let cam = camera(g, e, 0.3, tilt);
        for p in points(0.3) {
            let model = cam.project(&p).expect("in front");
            let traced = sensor_pixel(&cam, &chief_ray(&cam, &p).unwrap().on_sensor);
            let gap = (model - traced).norm();
            assert!(gap.is_finite(), "model {model}, traced {traced}");
            worst = worst.max(gap);
        }
    }
    println!("projection vs chief-ray trace: worst {worst:.3e} px");
    assert!(worst < 1e-9, "worst {worst} px");
}

#[test]
fn backprojection_is_the_chief_ray() {
    for (g, e, tilt) in cases() {
        let cam = camera(g, e, 0.3, tilt);
        for p in points(0.3).into_iter().take(20) {
            let pixel = cam.project(&p).unwrap();
            let (origin, dir) = cam.backproject(&pixel);
            // The point lies on the ray from the entrance pupil.
            let off = (p - origin).cross(&dir).norm() / dir.norm();
            assert!(off < 1e-10, "{off} m off the ray");
            assert!((origin.z - cam.cardinal_points().entrance_pupil).abs() < 1e-15);
        }
    }
}

#[test]
fn the_plane_of_best_focus_is_sharp_and_where_the_trace_converges() {
    for (g, e, tilt) in cases() {
        let cam = camera(g, e, 0.3, tilt);
        let (p0, n) = cam.plane_of_best_focus().unwrap();
        assert!(
            (p0 - Point3::new(0.0, 0.0, 0.3)).norm() < 1e-12,
            "focus point {p0}"
        );
        // Points on the plane: zero blur in the model, and every traced rim ray meets the sensor
        // where the chief ray does.
        let u = n.cross(&nalgebra::Vector3::x()).normalize();
        let v = n.cross(&u);
        for (a, b) in [(0.0, 0.0), (0.05, 0.0), (0.0, -0.04), (-0.06, 0.03)] {
            let p = p0 + a * u + b * v;
            let coc = cam.coc_m(&p).unwrap();
            assert!(coc < 1e-15, "blur {coc} m on the plane of best focus");
            let chief = chief_ray(&cam, &p).unwrap().on_sensor;
            for ray in rim_rays(&cam, &p, 8).unwrap() {
                let off = (ray.on_sensor - chief).norm();
                assert!(off < 1e-14, "rim ray off by {off}");
            }
        }
    }
}

#[test]
fn blur_matches_the_traced_rim_without_tilt() {
    for (g, e, _) in cases() {
        let cam = camera(g, e, 0.3, [0.0, 0.0]);
        for p in points(0.3).into_iter().take(40) {
            let chief = chief_ray(&cam, &p).unwrap().on_sensor;
            let rim = rim_rays(&cam, &p, 16).unwrap();
            // The rim's hits form a circle about the chief ray's hit.
            let radii: Vec<f64> = rim.iter().map(|r| (r.on_sensor - chief).norm()).collect();
            let traced = 2.0 * radii[0];
            // Rounding near focus is relative to the pupil, not to the (small) blur.
            let tol = 1e-12 * cam.exit_pupil_diameter_m();
            for r in &radii {
                assert!(
                    r.is_finite() && (r - radii[0]).abs() <= tol,
                    "not a circle: {r} vs {}",
                    radii[0]
                );
            }
            let model = cam.coc_m(&p).unwrap();
            assert!(
                (model - traced).abs() <= tol,
                "model {model} vs traced {traced}"
            );
        }
    }
}

#[test]
fn tilted_blur_is_first_order_in_the_tilt() {
    // The model reads the sensor's axial position at the image's lateral position; the trace
    // intersects the cone with the tilted plane, an ellipse, where the oblique chief ray meets
    // it. Over a field of ±0.2 rad the model's diameter is within 0.6·tanθ (relative) of the
    // traced mean diameter, θ the sensor normal's angle to the axis (measured: 0.48 at most, the
    // wider cone of a pupil behind H). The plane of best focus and the blur's centre are exact
    // (the tests above).
    let mut worst: f64 = 0.0;
    for (g, e, tilt) in cases() {
        if tilt == [0.0, 0.0] {
            continue;
        }
        let cam = camera(g, e, 0.3, tilt);
        let tan = {
            let n = cam.sensor_plane().normal;
            (n.x * n.x + n.y * n.y).sqrt() / n.z
        };
        for p in points(0.3).into_iter().take(40) {
            let chief = chief_ray(&cam, &p).unwrap().on_sensor;
            let rim = rim_rays(&cam, &p, 64).unwrap();
            let mean = 2.0
                * rim
                    .iter()
                    .map(|r| (r.on_sensor - chief).norm())
                    .sum::<f64>()
                / rim.len() as f64;
            let model = cam.coc_m(&p).unwrap();
            assert!(mean.is_finite() && model.is_finite());
            if mean > 1e-7 {
                worst = worst.max((model - mean).abs() / mean / tan);
            }
        }
    }
    println!("tilted blur: |model − traced| / traced ≤ {worst:.3} · tanθ");
    assert!(worst < 0.6, "coefficient {worst}");
}

#[test]
fn thin_lens_limit() {
    // G9.1: g = e = 0, no tilt.
    let f = 0.025;
    let n = 4.0;
    for focus in [0.15, 0.3, 1.0, 5.0] {
        let cam = camera(0.0, 0.0, focus, [0.0, 0.0]);
        let v0 = cam.image_distance_m();
        let d = cam.object_distance_m();
        assert!((d + v0 - focus).abs() <= 1e-15 * focus);
        assert!((1.0 / d + 1.0 / v0 - 1.0 / f).abs() <= 1e-12 / f);
        let IntrinsicsParams::FxFyCxCySkew { params } = &cam.pinhole().intrinsics;
        let expected = f * d / ((d - f) * PITCH);
        assert!(
            (params.fx - expected).abs() <= 1e-12 * expected,
            "fx {} vs {expected}",
            params.fx
        );
        assert!((params.fy - expected).abs() <= 1e-12 * expected);
        let t = cam.mech_se3_projection().translation.vector;
        assert!((t.z - v0).abs() <= 1e-12 * v0 && t.x == 0.0 && t.y == 0.0);
        // Depth of field against the textbook (distances from the lens): H = f²/(Nc) + f,
        // near = s(H − f)/(H + s − 2f), far = s(H − f)/(H − s).
        let c = 10e-6;
        let dof = cam.depth_of_field(c).unwrap();
        let h = f * f / (n * c) + f;
        let near = d * (h - f) / (h + d - 2.0 * f);
        assert!(
            (dof.near - v0 - near).abs() <= 1e-12 * near,
            "near {} vs {near}",
            dof.near - v0
        );
        if d < h {
            let far = d * (h - f) / (h - d);
            assert!((dof.far.unwrap() - v0 - far).abs() <= 1e-10 * far, "far");
        } else {
            assert!(dof.far.is_none());
        }
        // The hyperfocal focus distance, from the datum, is H plus its own image distance.
        let v_h = f * h / (h - f);
        assert!(
            (dof.hyperfocal - (h + v_h)).abs() <= 1e-12 * h,
            "hyperfocal {} vs {}",
            dof.hyperfocal,
            h + v_h
        );
    }
}

#[test]
fn equivalent_tilt_and_focal_lengths() {
    // m_p = 1: the model tilt is the physical one.
    let cam = camera(0.006, 0.0, 0.3, [0.07, -0.05]);
    let SensorParams::Scheimpflug { params } = &cam.pinhole().sensor else {
        panic!("tilted")
    };
    assert!((params.tilt_x - 0.07).abs() < 1e-12 && (params.tilt_y + 0.05).abs() < 1e-12);
    assert_eq!(cam.mech_se3_projection().rotation.angle(), 0.0);
    // m_p ≠ 1, one axis: tan τ′ = tan τ / m_p, and fy = fx · cos τ′ / cos τ.
    let tau: f64 = 0.1;
    let cam = camera(0.0, 0.005, 0.3, [tau, 0.0]);
    let mp = cam.pupil_magnification();
    let SensorParams::Scheimpflug { params } = &cam.pinhole().sensor else {
        panic!("tilted")
    };
    assert!(
        (params.tilt_x.tan() - tau.tan() / mp).abs() < 1e-12,
        "{} vs {}",
        params.tilt_x.tan(),
        tau.tan() / mp
    );
    let IntrinsicsParams::FxFyCxCySkew { params: k } = &cam.pinhole().intrinsics;
    assert!((k.fy / k.fx - params.tilt_x.cos() / tau.cos()).abs() < 1e-12);
    // The principal point stays where the axis meets the sensor.
    assert_eq!((k.cx, k.cy), (960.3, 600.7));
    let axis = cam.project(&Point3::new(0.0, 0.0, 0.5)).unwrap();
    assert!((axis - Point2::new(960.3, 600.7)).norm() < 1e-9);
}

#[test]
fn refocusing_moves_the_optics_not_the_body() {
    // Focus breathing: the lens moves out as it focuses closer, fx grows, and the mechanical
    // frame (where every point here is given) stays put.
    let far = camera(0.006, 0.004, 2.0, [0.05, 0.0]);
    let near = camera(0.006, 0.004, 0.25, [0.05, 0.0]);
    let fx = |c: &PhysicalCamera| match &c.pinhole().intrinsics {
        IntrinsicsParams::FxFyCxCySkew { params } => params.fx,
    };
    assert!(fx(&near) > fx(&far));
    assert!(near.cardinal_points().entrance_pupil > far.cardinal_points().entrance_pupil);
    for cam in [&far, &near] {
        let z = cam.focus_distance_m();
        let (p0, n) = cam.plane_of_best_focus().unwrap();
        assert!((p0.z - z).abs() < 1e-12);
        let on_plane = p0 + 0.03 * n.cross(&nalgebra::Vector3::y()).normalize();
        assert!(cam.coc_m(&on_plane).unwrap() < 1e-15);
        let traced = sensor_pixel(cam, &chief_ray(cam, &on_plane).unwrap().on_sensor);
        assert!((cam.project(&on_plane).unwrap() - traced).norm() < 1e-9);
    }
}

#[test]
fn agrees_with_the_legacy_model_for_a_thin_untilted_lens() {
    // The legacy model's origin is H′ and its focus distance is measured from H′.
    let cam = camera(0.0, 0.0, 0.3, [0.0, 0.0]);
    let v0 = cam.image_distance_m();
    for p in points(0.3) {
        let legacy = coc_diameter_at_sensor(
            &Point3::new(p.x, p.y, p.z - v0),
            0.025,
            4.0,
            0.3 - v0,
            0.0,
            0.0,
            0.0,
        )
        .unwrap();
        let model = cam.coc_m(&p).unwrap();
        // Rounding near focus is relative to the pupil, not to the (small) blur.
        assert!(
            (legacy - model).abs() <= 1e-12 * cam.exit_pupil_diameter_m(),
            "{legacy} vs {model}"
        );
    }
}

#[test]
fn the_legacy_lens_reads_calibration_rs_tilt_the_same_way() {
    // ThickLens takes calibration-rs's tilt from CameraParams; its plane of best focus and blur
    // must be the physical camera's (g = e = 0, where the legacy model applies). For a tilt about
    // one axis they agree exactly. About both, the legacy model composes the axes independently
    // (slopes tan τy and tan τx), where calibration-rs's sensor, `Ry·Rx`, has slope
    // tan τy / cos τx along x: a second-order difference.
    use etendue_core::calibration::ScheimpflugParams;
    use etendue_core::optics::ThickLens;
    for tilt in [[0.08, 0.0], [0.0, 0.06], [-0.07, 0.05]] {
        let cam = camera(0.0, 0.0, 0.3, tilt);
        let v0 = cam.image_distance_m();
        let mut params = as_built();
        params.sensor = SensorParams::Scheimpflug {
            params: ScheimpflugParams {
                tilt_x: tilt[0],
                tilt_y: tilt[1],
            },
        };
        let legacy = ThickLens::new(params, 0.025, 4.0, 0.3 - v0, 0.0, PITCH).unwrap();
        let (p0, n) = cam.plane_of_best_focus().unwrap();
        let pobf = legacy.plane_of_best_focus().unwrap();
        let compound = tilt[0] != 0.0 && tilt[1] != 0.0;
        // Relative: exact for one axis; within 1 − cos τx for two.
        let tol = if compound { 1.0 - tilt[0].cos() } else { 1e-12 };
        let (gx, gy) = (-n.x / n.z, -n.y / n.z);
        assert!(
            (pobf.tan_tilt_y - gx).abs() <= tol * gx.abs().max(1e-12),
            "dz/dx {} vs {gx}",
            pobf.tan_tilt_y
        );
        assert!(
            (pobf.tan_tilt_x - gy).abs() <= tol * gy.abs().max(1e-12),
            "dz/dy {} vs {gy}",
            pobf.tan_tilt_x
        );
        assert!(pobf.tan_tilt_y.signum() == gx.signum() || gx == 0.0);
        assert!(pobf.tan_tilt_x.signum() == gy.signum() || gy == 0.0);
        assert!((pobf.focus_depth + v0 - p0.z).abs() < 1e-12);
        if compound {
            continue;
        }
        for p in points(0.3).into_iter().take(40) {
            let old = legacy
                .coc_diameter(&Point3::new(p.x, p.y, p.z - v0))
                .unwrap();
            let new = cam.coc_m(&p).unwrap();
            assert!(
                (old - new).abs() <= 1e-12 * cam.exit_pupil_diameter_m(),
                "{old} vs {new}"
            );
        }
    }
}
