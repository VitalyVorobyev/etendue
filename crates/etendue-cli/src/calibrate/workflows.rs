//! The single-camera workflows (single-camera hand-eye, laserline device with its hand-eye,
//! planar intrinsics) and what every workflow's result becomes: a [`Report`].

use anyhow::Result;
use etendue_synth::observe::{ObservationNoise, Observations};
use nalgebra::Isometry3;
use serde_json::{Value, json};
use vision_calibration_core::{NoMeta, PlanarDataset, View};
use vision_calibration_pipeline::common::RobotPoseMeta;
use vision_calibration_pipeline::laserline_device::DeviceManualInit;
use vision_calibration_pipeline::session::CalibrationSession;
use vision_calibration_pipeline::{laserline_device, planar_intrinsics, single_cam_handeye};

use super::REPORT_VERSION;
use super::compare::{
    handeye_rows, intrinsics_rows, offsets, plane_rows, reprojection_row, tilt_rows,
};
use super::observations::{correspondences, laser_pixels, planar_views, used_views};
use super::report::{Estimate, Predicted, Report, Residuals, Row, Workflow};
use super::truth::Truth;
use super::upstream::{
    SeedValues, StageRun, camera_of, config, exported, handeye_mode, plane_of, predicted,
    residuals, single_handeye, sizes, strict,
};

/// What a single-camera hand-eye run gives: the export (typed and as JSON), its quality verdict,
/// views and points.
type SingleRun = (
    single_cam_handeye::SingleCamHandeyeExport,
    Value,
    Option<Value>,
    usize,
    usize,
);

fn run_single(
    obs: &Observations,
    poses: &[Isometry3<f64>],
    i: usize,
    eye_in_hand: bool,
    run: &StageRun,
    size: [u32; 2],
) -> Result<SingleRun> {
    let mut views = Vec::new();
    let mut points = 0;
    for (c, pose) in obs.captures.iter().zip(poses) {
        if let Some(v) = correspondences(&c.views[i]) {
            points += v.len();
            views.push(View::new(
                v,
                RobotPoseMeta {
                    base_se3_gripper: *pose,
                },
            ));
        }
    }
    let n = views.len();
    let mut session = CalibrationSession::<single_cam_handeye::SingleCamHandeyeProblem>::new();
    session.set_input(single_cam_handeye::SingleCamHandeyeInput::new(views)?)?;
    session.set_config(config(&run.patch)?)?;
    let s = &run.seeds;
    if s.none() {
        single_cam_handeye::run_calibration(&mut session)?;
    } else {
        if s.intrinsics.is_some() || s.sensor.is_some() {
            let manual = strict(s.camera_init(i), "intrinsics seed")?;
            single_cam_handeye::step_intrinsics_init_with_seed(&mut session, manual, None)?;
        } else {
            single_cam_handeye::step_intrinsics_init(&mut session, None)?;
        }
        single_cam_handeye::step_intrinsics_optimize(&mut session, None)?;
        if let Some(he) = &s.handeye {
            let key = if eye_in_hand {
                "gripper_se3_camera"
            } else {
                "camera_se3_base"
            };
            let manual = strict(json!({ key: he }), "hand-eye seed")?;
            single_cam_handeye::step_handeye_init_with_seed(&mut session, manual, None)?;
        } else {
            single_cam_handeye::step_handeye_init(&mut session, None)?;
        }
        single_cam_handeye::step_handeye_optimize(&mut session, None)?;
    }
    let export = session.export()?;
    let (json, quality) = exported(&export, &[size])?;
    Ok((export, json, quality, n, points))
}

/// A workflow's result before it is reported.
pub(super) struct Solved {
    pub(super) workflow: Workflow,
    pub(super) cameras: Vec<usize>,
    pub(super) rows: Vec<Row>,
    /// Views, target points, laser pixels.
    pub(super) counts: (usize, usize, usize),
    pub(super) residuals: Residuals,
    pub(super) estimate: Estimate,
    /// The target pose the workflow reports (base frame eye-in-hand, gripper eye-to-hand).
    pub(super) target: Option<Isometry3<f64>>,
    pub(super) export: Value,
    /// The capture of each of calibration-rs's views, in order.
    pub(super) views: Vec<usize>,
    /// calibration-rs's quality verdict.
    pub(super) quality: Option<Value>,
    /// calibration-rs's predicted uncertainty.
    pub(super) uncertainty: Option<Predicted>,
    /// A rig workflow's per-view target poses in the rig frame.
    pub(super) rig_se3_target: Vec<Isometry3<f64>>,
}

pub(super) fn finish(truth: &Truth, obs: &Observations, gated: bool, solved: Solved) -> Report {
    let Solved {
        workflow,
        cameras,
        rows,
        counts,
        mut residuals,
        mut estimate,
        target,
        export,
        views,
        quality,
        uncertainty,
        rig_se3_target,
    } = solved;
    estimate.offsets = offsets(truth, &cameras, &estimate, target);
    // Name each residual's capture, camera and target point.
    residuals.features.retain_mut(|f| {
        let [view, cam, feature] = f.raw;
        let (Some(&k), Some(&i)) = (views.get(view), cameras.get(cam)) else {
            return false;
        };
        let capture = &obs.captures[k];
        let Some(point) = capture.views[i].points.get(feature) else {
            return false;
        };
        f.capture.clone_from(&capture.id);
        f.camera.clone_from(&truth.gt.cameras[i].id);
        f.point = point.point;
        // Named now: the indices are not kept (nor written).
        f.raw = [0; 3];
        true
    });
    let laser = !truth.gt.lasers.is_empty();
    let pass = rows.iter().all(Row::within);
    Report {
        version: REPORT_VERSION,
        workflow,
        gate: if laser { "G5.3" } else { "G5.1" }.into(),
        gated,
        pass: !gated || pass,
        cameras: cameras
            .iter()
            .map(|&i| truth.gt.cameras[i].id.clone())
            .collect(),
        views: counts.0,
        points: counts.1,
        laser_pixels: counts.2,
        noise: (obs.noise != ObservationNoise::default()).then_some(obs.noise),
        rows,
        residuals,
        estimate,
        quality,
        uncertainty,
        procedure: String::new(),
        stages: Vec::new(),
        export,
        rig_se3_target,
    }
}

pub(crate) fn single(
    truth: &Truth,
    obs: &Observations,
    poses: &[Isometry3<f64>],
    i: usize,
    eye_in_hand: bool,
    gated: bool,
    run: &StageRun,
) -> Result<Report> {
    let (export, json, quality, views, points) = run_single(
        obs,
        poses,
        i,
        eye_in_hand,
        run,
        truth.gt.cameras[i].resolution,
    )?;
    let camera = camera_of(
        &export.camera,
        &truth.gt.cameras[i].id,
        Isometry3::identity(),
    )?;
    let (he, target) = single_handeye(truth, &export.handeye)?;
    let mut rows = vec![reprojection_row(export.mean_reproj_error)];
    rows.extend(intrinsics_rows(&camera, &truth.gt.cameras[i]));
    if run.scheimpflug {
        rows.extend(tilt_rows(&camera, &truth.gt.cameras[i]));
    }
    rows.extend(handeye_rows(truth, (he, target), &truth.cam_se3_rig[i]));
    let estimate = Estimate {
        cameras: vec![camera],
        handeye: Some(he),
        laser_planes: vec![],
        offsets: None,
    };
    Ok(finish(
        truth,
        obs,
        gated,
        Solved {
            workflow: Workflow::SingleCamHandeye,
            cameras: vec![i],
            rows,
            counts: (views, points, 0),
            residuals: residuals(&export.diagnostics, export.mean_reproj_error, 1),
            estimate,
            target: Some(target),
            export: json,
            views: used_views(obs, |c| correspondences(&c.views[i]).is_some()),
            quality,
            uncertainty: predicted(&export.uncertainty),
            rig_se3_target: Vec::new(),
        },
    ))
}

/// One camera carrying its laser: the laserline device workflow (`device`) for intrinsics, tilt
/// and plane, then the single-camera hand-eye workflow on the same views for the hand-eye, with
/// the device's camera held. The plane is in that camera's frame; a camera re-estimated for the
/// hand-eye would settle on another frame, and the plane and the hand-eye would not compose
/// (`docs/measurements/head_triangulation.md`).
pub(crate) fn head(
    truth: &Truth,
    obs: &Observations,
    poses: &[Isometry3<f64>],
    eye_in_hand: bool,
    gated: bool,
    device_run: &StageRun,
) -> Result<Report> {
    let mut views = Vec::new();
    let mut points = 0;
    let mut pixels = 0;
    for c in &obs.captures {
        let v = &c.views[0];
        if let (Some(cv), Some(px)) = (correspondences(v), laser_pixels(v)) {
            points += cv.len();
            pixels += px.len();
            views.push(json!({"obs": cv, "meta": {"laser_pixels": px, "laser_weights": vec![1.0; px.len()]}}));
        }
    }
    let count = views.len();
    let input: laserline_device::LaserlineDeviceInput =
        strict(Value::Array(views), "laserline device input")?;
    let tilted = device_run.scheimpflug;
    let mut session = CalibrationSession::<laserline_device::LaserlineDeviceProblem>::new();
    session.set_input(input)?;
    session.set_config(config(&device_run.patch)?)?;
    let seed: DeviceManualInit = strict(device_run.seeds.camera_init(0), "device seed")?;
    laserline_device::step_init_with_seed(&mut session, seed, None)?;
    laserline_device::step_optimize(&mut session, None)?;
    let device = session.export()?;
    let (device_json, quality) = exported(&device, &sizes(truth, &[0]))?;
    let camera = camera_of(
        &device.params.camera,
        &truth.gt.cameras[0].id,
        Isometry3::identity(),
    )?;
    let plane = plane_of(&device.params.plane);
    let mut rows = vec![reprojection_row(device.mean_reproj_error)];
    rows.extend(intrinsics_rows(&camera, &truth.gt.cameras[0]));
    if tilted {
        rows.extend(tilt_rows(&camera, &truth.gt.cameras[0]));
    }
    if let Some(t) = truth.plane(0) {
        rows.extend(plane_rows(&plane, t, &truth.gt.cameras[0].id));
    }
    // The hand-eye, from the same target views, with the device's camera held.
    let mut held = json!({"fix": {
        "intrinsics": ["fx", "fy", "cx", "cy"],
        "distortion": ["k1", "k2", "k3", "p1", "p2"],
    }});
    if tilted {
        held["model"] = json!({"sensor": "scheimpflug"});
        held["fix"]["sensor"] = json!(["tilt_x", "tilt_y"]);
    }
    let handeye_run = StageRun {
        patch: json!({"handeye_init": {"handeye_mode": handeye_mode(eye_in_hand)}, "camera": held}),
        seeds: SeedValues {
            intrinsics: Some(vec![camera.intrinsics]),
            distortion: Some(vec![Some(camera.distortion)]),
            sensor: Some(vec![camera.tilt]),
            ..SeedValues::default()
        },
        scheimpflug: tilted,
    };
    let (he_export, he_json, _, _, _) = run_single(
        obs,
        poses,
        0,
        eye_in_hand,
        &handeye_run,
        truth.gt.cameras[0].resolution,
    )?;
    let (he, target) = single_handeye(truth, &he_export.handeye)?;
    rows.extend(handeye_rows(truth, (he, target), &truth.cam_se3_rig[0]));
    let estimate = Estimate {
        cameras: vec![camera],
        handeye: Some(he),
        laser_planes: vec![Some(plane)],
        offsets: None,
    };
    Ok(finish(
        truth,
        obs,
        gated,
        Solved {
            workflow: Workflow::LaserlineDevice,
            cameras: vec![0],
            rows,
            counts: (count, points, pixels),
            residuals: residuals(&device.diagnostics, device.mean_reproj_error, 1),
            estimate,
            target: Some(target),
            export: json!({"laserline_device": device_json, "single_cam_handeye": he_json}),
            views: used_views(obs, |c| {
                correspondences(&c.views[0]).is_some() && laser_pixels(&c.views[0]).is_some()
            }),
            quality,
            uncertainty: predicted(&device.uncertainty)
                .or_else(|| predicted(&he_export.uncertainty)),
            rig_se3_target: Vec::new(),
        },
    ))
}

/// One camera's intrinsics from its planar views, no robot (`planar_intrinsics`).
pub(crate) fn intrinsics(
    truth: &Truth,
    obs: &Observations,
    i: usize,
    gated: bool,
    run: &StageRun,
) -> Result<Report> {
    let (views, points) = planar_views(obs, i);
    let count = views.len();
    let mut session = CalibrationSession::<planar_intrinsics::PlanarIntrinsicsProblem>::new();
    session.set_input(PlanarDataset::new(
        views.into_iter().map(|v| View::new(v, NoMeta)).collect(),
    )?)?;
    session.set_config(config(&run.patch)?)?;
    let s = &run.seeds;
    if s.intrinsics.is_some() || s.sensor.is_some() {
        let manual = strict(s.camera_init(i), "intrinsics seed")?;
        planar_intrinsics::step_init_with_seed(&mut session, manual, None)?;
        planar_intrinsics::step_optimize(&mut session, None)?;
    } else {
        planar_intrinsics::run_calibration(&mut session)?;
    }
    let export = session.export()?;
    let (json, quality) = exported(&export, &sizes(truth, &[i]))?;
    let camera = camera_of(
        &export.params.camera,
        &truth.gt.cameras[i].id,
        Isometry3::identity(),
    )?;
    let mut rows = vec![reprojection_row(export.mean_reproj_error)];
    rows.extend(intrinsics_rows(&camera, &truth.gt.cameras[i]));
    if run.scheimpflug {
        rows.extend(tilt_rows(&camera, &truth.gt.cameras[i]));
    }
    let estimate = Estimate {
        cameras: vec![camera],
        handeye: None,
        laser_planes: vec![],
        offsets: None,
    };
    Ok(finish(
        truth,
        obs,
        gated,
        Solved {
            workflow: Workflow::PlanarIntrinsics,
            cameras: vec![i],
            rows,
            counts: (count, points, 0),
            residuals: residuals(&export.diagnostics, export.mean_reproj_error, 1),
            estimate,
            target: None,
            export: json,
            views: used_views(obs, |c| correspondences(&c.views[i]).is_some()),
            quality,
            uncertainty: predicted(&export.uncertainty),
            rig_se3_target: Vec::new(),
        },
    ))
}
