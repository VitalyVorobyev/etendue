//! Gates G10.2 and G10.3 (PLAN P10): a triangulation head's Scheimpflug setting puts the plane of
//! best focus on the laser plane, as the hinge rule says; the optimiser reproduces it on a thin
//! window and never ends worse than where it started.
//!
//! ```bash
//! cargo test -p etendue-core --test head -- --nocapture
//! ```

use etendue_core::calibration::{
    CameraParams, DistortionParams, FxFyCxCySkew, IntrinsicsParams, ProjectionParams, SensorParams,
};
use etendue_core::head::{Beam, Head, Search, Setting, Window};
use etendue_core::optics::camera::{Focusing, Lens, PhysicalCamera};
use etendue_core::optics::coc::pobf_tilt_tan;
use nalgebra::{Isometry3, Matrix3, Point3, Rotation3, Translation3, UnitQuaternion, Vector3};

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
                cx: 968.0,
                cy: 608.0,
                skew: 0.0,
            },
        },
    }
}

/// A 25 mm camera at f/4 and a laser 100 mm off its axis (camera −y), aimed at the axis 0.35 m
/// out; the laser line runs along camera x.
fn head(g: f64, e: f64) -> Head {
    head_with(g, e, None, 0.0)
}

fn head_with(g: f64, e: f64, waist: Option<f64>, waist_distance: f64) -> Head {
    let lens = Lens {
        focal_length_m: 0.025,
        f_number: 4.0,
        principal_gap_m: g,
        entrance_pupil_m: e,
        focusing: Focusing::Unit,
    };
    let camera = PhysicalCamera::new(lens, PITCH, 0.3, [0.0, 0.0], &as_built()).unwrap();
    let origin = Point3::new(0.0, -0.1, 0.05);
    let aim = Point3::new(0.0, 0.0, 0.35);
    let central = (aim - origin).normalize();
    let line = Vector3::x();
    let normal = line.cross(&central);
    let rotation =
        Rotation3::from_matrix_unchecked(Matrix3::from_columns(&[normal, line, central]));
    let pose = Isometry3::from_parts(
        Translation3::from(origin.coords),
        UnitQuaternion::from_rotation_matrix(&rotation),
    );
    Head::new(
        camera,
        [1936, 1216],
        Beam {
            half_angle: 0.4,
            length: 1.0,
            wavelength: 660e-9,
            waist,
            waist_distance,
        },
        &pose,
    )
}

/// The window around the aim point, `depth` deep along the laser and 30 mm wide.
fn window(depth: f64) -> Window {
    let b = (Point3::new(0.0, 0.0, 0.35) - Point3::new(0.0, -0.1, 0.05)).norm();
    Window {
        near: b - depth / 2.0,
        far: b + depth / 2.0,
        width: 0.03,
    }
}

#[test]
fn the_scheimpflug_setting_matches_the_hinge_formula() {
    // Thin lens: the legacy derivation's tan θ = (s_o + g)/v0 · tan τ, inverted, in its own tilt
    // convention (calibration-rs's tilt_x is its −τx).
    let h = head(0.0, 0.0);
    let setting = h.scheimpflug().unwrap();
    let tuned = h.with_setting(&setting).unwrap();
    let cam = tuned.camera();
    let v0 = cam.image_distance_m();
    // The laser plane's depth gradient along camera y: z = z0 + y · tan θ.
    let normal = (Point3::new(0.0, 0.0, 0.35) - Point3::new(0.0, -0.1, 0.05))
        .normalize()
        .cross(&-Vector3::x());
    let tan_theta = -normal.y / normal.z;
    let s_o = setting.focus_distance_m - v0; // from H′, as the legacy model measures it (g = 0)
    let tau_core = (tan_theta * v0 / s_o).atan();
    assert!((pobf_tilt_tan(tau_core, s_o, 0.0, 0.025).unwrap() - tan_theta).abs() < 1e-12);
    assert!(
        (setting.tilt[0] - (-tau_core)).abs() < 1e-9,
        "tilt_x {} vs {}",
        setting.tilt[0],
        -tau_core
    );
    assert!(setting.tilt[1].abs() < 1e-12);
    assert!((setting.focus_distance_m - 0.35).abs() < 1e-12);
}

#[test]
fn after_it_the_plane_of_best_focus_is_the_laser_plane() {
    // G10.2, with a thick lens and its pupil off H.
    for (g, e) in [(0.0, 0.0), (0.006, 0.004), (-0.003, -0.008)] {
        let h = head(g, e);
        let tuned = h.with_setting(&h.scheimpflug().unwrap()).unwrap();
        let worst_coc = tuned
            .map(&window(0.08), [9, 9])
            .iter()
            .map(|s| s.coc_px)
            .fold(0.0, f64::max);
        assert!(
            worst_coc < 1e-9,
            "g {g} e {e}: blur {worst_coc} px on the laser plane"
        );
        let section = tuned
            .section(Some(&window(0.08)), &[0.3, 0.4], Some(2.0))
            .unwrap();
        // The focus trace is the laser trace.
        let (f, l) = (section.focus, section.laser_trace);
        let cross = f.direction[0] * l.direction[1] - f.direction[1] * l.direction[0];
        let gap =
            (l.point[0] - f.point[0]) * f.direction[1] - (l.point[1] - f.point[1]) * f.direction[0];
        assert!(
            cross.abs() < 1e-9 && gap.abs() < 1e-9,
            "g {g} e {e}: traces {f:?} vs {l:?}"
        );
        // The hinge rule: the sensor meets H′, and the plane of best focus meets H, at one height.
        let (a, b) = (
            section.sensor_at_rear_principal.unwrap(),
            section.focus_at_front_principal.unwrap(),
        );
        assert!(
            (a[0] - b[0]).abs() < 1e-9,
            "g {g} e {e}: hinge at {} and {}",
            a[0],
            b[0]
        );
    }
}

#[test]
fn the_optimiser_reproduces_the_hinge_on_a_thin_window_and_never_ends_worse() {
    // G10.3.
    let h = head(0.006, 0.004);
    let analytic = h.scheimpflug().unwrap();
    let thin = h
        .optimise(
            &window(1e-4),
            &Search {
                grid: [7, 3],
                f_number_range: None,
                tilt_free: true,
            },
        )
        .unwrap();
    let off = (thin.best.tilt[0] - analytic.tilt[0])
        .abs()
        .max((thin.best.tilt[1] - analytic.tilt[1]).abs());
    println!(
        "thin window: tilt off the hinge by {:.4}°; worst blur {:.4} px (start {:.4})",
        off.to_degrees(),
        thin.worst_blur_px,
        thin.start_worst_blur_px
    );
    assert!(off.to_degrees() < 0.05, "{}°", off.to_degrees());
    assert!(thin.worst_blur_px <= thin.start_worst_blur_px + 1e-9);
    // With the tilt free, the whole laser plane is in focus: only diffraction is left, and the
    // widest aperture wins.
    let tilted = h
        .optimise(
            &window(0.12),
            &Search {
                grid: [7, 9],
                f_number_range: Some([2.0, 16.0]),
                tilt_free: true,
            },
        )
        .unwrap();
    assert!(
        (tilted.best.f_number - 2.0).abs() < 1e-3,
        "f/{}",
        tilted.best.f_number
    );
    // A square sensor on a deep window: focus and f-number balance defocus against
    // diffraction, between the bounds.
    let square = h
        .optimise(
            &window(0.12),
            &Search {
                grid: [7, 9],
                f_number_range: Some([2.0, 32.0]),
                tilt_free: false,
            },
        )
        .unwrap();
    println!(
        "square sensor, deep window: worst blur {:.3} px at f/{:.2}, focus {:.4} m (start {:.3} px at f/{:.1}), {} iterations",
        square.worst_blur_px,
        square.best.f_number,
        square.best.focus_distance_m,
        square.start_worst_blur_px,
        square.start.f_number,
        square.iterations
    );
    assert_eq!(square.best.tilt, [0.0, 0.0]);
    assert!(square.worst_blur_px < square.start_worst_blur_px);
    assert!(
        square.best.f_number > 2.5 && square.best.f_number < 31.0,
        "f/{}",
        square.best.f_number
    );
}

#[test]
fn samples_see_what_is_in_view_and_resolve_depth_by_triangulation() {
    let h = head(0.0, 0.0);
    let tuned = h
        .with_setting(&Setting {
            f_number: 4.0,
            ..h.scheimpflug().unwrap()
        })
        .unwrap();
    let w = window(0.08);
    let centre = tuned.sample(0.0, 0.5 * (w.near + w.far));
    assert!(centre.visible);
    // Depth resolution: a pixel across the imaged line is the lateral pixel scaled by the
    // triangulation geometry, about pixel size / sin(triangulation angle).
    let rough = centre.lateral_m_per_px / centre.triangulation_rad.sin();
    assert!(
        (centre.depth_m_per_px / rough - 1.0).abs() < 0.3,
        "{} vs {}",
        centre.depth_m_per_px,
        rough
    );
    // Out of the fan: unseen.
    assert!(!tuned.sample(0.5, 0.2).visible);
    assert_eq!(tuned.map(&w, [4, 3]).len(), 12);
}

#[test]
fn the_depth_of_field_lines_are_where_the_blur_reaches_the_limit() {
    let h = head(0.006, 0.004);
    let tuned = h.with_setting(&h.scheimpflug().unwrap()).unwrap();
    let section = tuned.section(None, &[], Some(3.0)).unwrap();
    let [near, far] = section.depth_of_field.unwrap();
    assert!(
        near.len() > 10 && far.len() > 10,
        "{} near, {} far",
        near.len(),
        far.len()
    );
    let s = Vector3::from(section.s_axis);
    let cam = tuned.camera();
    let airy = cam.airy_diameter_m(660e-9) / cam.pixel_pitch_m();
    for p in near.iter().chain(&far) {
        let point = Point3::from(p[0] * s + p[1] * Vector3::z());
        let blur = cam.coc_px(&point).unwrap().hypot(airy);
        assert!((blur - 3.0).abs() < 1e-6, "blur {blur} px at {p:?}");
    }
}

#[test]
fn the_beam_images_as_a_gaussian_beam_across_the_stripe() {
    // The window's centre, 0.32 m from the laser.
    let w = window(0.1);
    let at = |waist: Option<f64>| head_with(0.004, 0.003, waist, 0.0).map(&w, [3, 3])[4];
    assert!(at(None).beam_px.is_nan());
    // Near field (z_R = πw₀²/λ ≈ 4.8 m ≫ 0.32 m): the waist's own thickness, doubling with it.
    let (a, b) = (at(Some(1e-3)).beam_px, at(Some(2e-3)).beam_px);
    assert!(a > 1.0 && (b / a - 2.0).abs() < 1e-2, "{a} {b}");
    // Far field (z_R ≈ 1.9 mm ≪ 0.32 m): w ≈ λr/(πw₀), so halving the waist doubles it.
    let (c, d) = (at(Some(20e-6)).beam_px, at(Some(10e-6)).beam_px);
    assert!((d / c - 2.0).abs() < 1e-3, "{c} {d}");
    // The thinnest stripe at this range is between: w₀ = √(λr/π) minimises w(r).
    let best = (660e-9 * 0.32 / std::f64::consts::PI).sqrt();
    assert!(at(Some(best)).beam_px < at(Some(best * 2.0)).beam_px);
    assert!(at(Some(best)).beam_px < at(Some(best / 2.0)).beam_px);
}

#[test]
fn a_focused_beam_is_its_waist_where_it_is_focused() {
    let w = window(0.1);
    let r = (Point3::new(0.0, 0.0, 0.35) - Point3::new(0.0, -0.1, 0.05)).norm();
    let w0 = 20e-6;
    let at = |d: f64| head_with(0.004, 0.003, Some(w0), d).map(&w, [3, 3]);
    // Focused at the centre's range, the stripe there is the waist, w(r)/w₀ thinner than with
    // the waist at the laser: the same imaging, a different thickness.
    let rayleigh = std::f64::consts::PI * w0 * w0 / 660e-9;
    let ratio = at(r)[4].beam_px / at(0.0)[4].beam_px;
    let expected = 1.0 / (1.0 + (r / rayleigh).powi(2)).sqrt();
    assert!((ratio / expected - 1.0).abs() < 1e-9, "{ratio} {expected}");
    // Away from the focus it widens on both sides: the near and far rows are thicker.
    let focused = at(r);
    assert!(focused[1].beam_px > focused[4].beam_px && focused[7].beam_px > focused[4].beam_px);
}

#[test]
fn the_solid_is_the_camera_in_three_dimensions() {
    use etendue_core::optics::paraxial::sensor_pixel;
    // A tilted sensor, so the corners are not in one z plane.
    let mut h = head(0.004, 0.003);
    h = h
        .with_setting(&Setting {
            tilt: [0.1, 0.03],
            focus_distance_m: 0.35,
            f_number: 4.0,
        })
        .unwrap();
    let section = h.section(Some(&window(0.1)), &[], None).unwrap();
    let solid = section.solid;
    let cam = h.camera();
    let corners = [[0.0, 0.0], [1936.0, 0.0], [1936.0, 1216.0], [0.0, 1216.0]];
    for (i, [u, v]) in corners.into_iter().enumerate() {
        // Each corner is on the sensor at its pixel.
        let p = Point3::from(solid.sensor[i]);
        let px = sensor_pixel(cam, &p);
        assert!(
            (px.x - u).abs() < 1e-9 && (px.y - v).abs() < 1e-9,
            "{px:?} for {u}, {v}"
        );
        // A point along its field ray projects to that pixel.
        let r = solid.field[i];
        let q = Point3::from(r.point) + 0.4 * Vector3::from(r.direction);
        let back = cam.project(&q).unwrap();
        assert!(
            (back.x - u).abs() < 1e-6 && (back.y - v).abs() < 1e-6,
            "{back:?} for {u}, {v}"
        );
    }
    // The plane of best focus is the camera's own.
    let (point, normal) = cam.plane_of_best_focus().unwrap();
    assert_eq!(solid.focus.point, <[f64; 3]>::from(point.coords));
    assert_eq!(solid.focus.normal, <[f64; 3]>::from(normal));
}
