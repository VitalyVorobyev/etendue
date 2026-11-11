//! The rig workflows: rig hand-eye, rig hand-eye laserline and rig extrinsics.

use anyhow::{Result, ensure};
use etendue_synth::observe::Observations;
use nalgebra::Isometry3;
use serde_json::{Value, json};
use vision_calibration_core::{
    CameraParams, CorrespondenceView, NoMeta, RigDataset, RigView, RigViewObs,
};
use vision_calibration_pipeline::common::RobotPoseMeta;
use vision_calibration_pipeline::session::CalibrationSession;
use vision_calibration_pipeline::{rig_extrinsics, rig_handeye, rig_handeye_laserline};

use super::compare::{
    difference, handeye_rows, intrinsics_rows, plane_rows, reprojection_row, tilt_rows,
};
use super::observations::{correspondences, laser_pixels, used_views};
use super::report::{Estimate, EstimatedCamera, Plane, Report, Row, Workflow};
use super::truth::Truth;
use super::upstream::{
    StageRun, camera_of, config, exported, plane_of, predicted, residuals, rig_handeye, sizes,
    strict,
};
use super::workflows::{Solved, finish};

/// The estimated cameras of a rig export, in the truth's camera order.
fn rig_estimate(
    truth: &Truth,
    cameras: &[CameraParams],
    cam_se3_rig: &[Isometry3<f64>],
) -> Result<Vec<EstimatedCamera>> {
    let n = truth.gt.cameras.len();
    ensure!(
        cameras.len() == n && cam_se3_rig.len() == n,
        "calibration-rs returned {} cameras and {} rig poses for a rig of {n}",
        cameras.len(),
        cam_se3_rig.len()
    );
    truth
        .gt
        .cameras
        .iter()
        .zip(cameras)
        .zip(cam_se3_rig)
        .map(|((c, p), x)| camera_of(p, &c.id, *x))
        .collect()
}

/// The rows of a rig workflow: the mean reprojection error; per camera its intrinsics, its
/// `cam_se3_rig` and `per_camera`'s; then the hand-eye's, with one.
fn rig_rows(
    truth: &Truth,
    mean_px: f64,
    cameras: &[EstimatedCamera],
    handeye: Option<(Isometry3<f64>, Isometry3<f64>)>,
    mut per_camera: impl FnMut(usize) -> Vec<Row>,
) -> Vec<Row> {
    let ref_se3_rig = truth.cam_se3_rig[0];
    let rig_se3_ref = ref_se3_rig.inverse();
    let mut rows = vec![reprojection_row(mean_px)];
    for (i, (e, t)) in cameras.iter().zip(&truth.gt.cameras).enumerate() {
        rows.extend(intrinsics_rows(e, t));
        if i > 0 {
            let (r, d) = difference(&e.cam_se3_ref, &(truth.cam_se3_rig[i] * rig_se3_ref));
            rows.push(Row::new(format!("{} cam_se3_rig deg", t.id), r, None));
            rows.push(Row::new(format!("{} cam_se3_rig mm", t.id), d, None));
        }
        rows.extend(per_camera(i));
    }
    if let Some(pair) = handeye {
        rows.extend(handeye_rows(truth, pair, &ref_se3_rig));
    }
    rows
}

/// The captures a rig sees the target in, each with every camera's correspondences (by capture
/// index), and the target points in them.
type RigCaptures = (Vec<(usize, Vec<Option<CorrespondenceView>>)>, usize);

fn rig_captures(obs: &Observations) -> RigCaptures {
    let mut points = 0;
    let captures = obs
        .captures
        .iter()
        .enumerate()
        .filter_map(|(k, c)| {
            let cams: Vec<Option<CorrespondenceView>> =
                c.views.iter().map(correspondences).collect();
            cams.iter().any(Option::is_some).then(|| {
                points += cams
                    .iter()
                    .flatten()
                    .map(CorrespondenceView::len)
                    .sum::<usize>();
                (k, cams)
            })
        })
        .collect();
    (captures, points)
}

fn rig_views_used(obs: &Observations) -> Vec<usize> {
    used_views(obs, |c| {
        c.views.iter().any(|v| correspondences(v).is_some())
    })
}

pub(crate) fn rig(
    truth: &Truth,
    obs: &Observations,
    poses: &[Isometry3<f64>],
    gated: bool,
    run: &StageRun,
) -> Result<Report> {
    let n = truth.gt.cameras.len();
    let all: Vec<usize> = (0..n).collect();
    let (captures, points) = rig_captures(obs);
    let views: Vec<_> = captures
        .into_iter()
        .map(|(k, cameras)| RigView {
            obs: RigViewObs { cameras },
            meta: RobotPoseMeta {
                base_se3_gripper: poses[k],
            },
        })
        .collect();
    let count = views.len();
    let mut session = CalibrationSession::<rig_handeye::RigHandeyeProblem>::new();
    session.set_input(RigDataset::new(views, n)?)?;
    session.set_config(config(&run.patch)?)?;
    let s = &run.seeds;
    if s.cam_se3_rig.is_none() && s.handeye.is_none() {
        // Intrinsics seeds, if any, are in the config's `manual_init`.
        rig_handeye::run_calibration(&mut session)?;
    } else {
        let manual = strict(
            s.rig_manual_init(&all).unwrap_or_else(|| json!({})),
            "intrinsics seed",
        )?;
        rig_handeye::step_intrinsics_init_all_with_seed(&mut session, manual, None)?;
        rig_handeye::step_intrinsics_optimize_all(&mut session, None)?;
        if let Some(rigs) = &s.cam_se3_rig {
            let manual = strict(
                json!({ "cam_se3_rig": rigs, "rig_se3_target": s.rig_se3_target }),
                "rig seed",
            )?;
            rig_handeye::step_rig_init_with_seed(&mut session, manual, None)?;
        } else {
            rig_handeye::step_rig_init(&mut session, None)?;
        }
        rig_handeye::step_rig_optimize(&mut session, None)?;
        if let Some(he) = &s.handeye {
            let manual = strict(json!({ "handeye": he }), "hand-eye seed")?;
            rig_handeye::step_handeye_init_with_seed(&mut session, manual, None)?;
        } else {
            rig_handeye::step_handeye_init(&mut session, None)?;
        }
        rig_handeye::step_handeye_optimize(&mut session, None)?;
    }
    let export = session.export()?;
    let (json, quality) = exported(&export, &sizes(truth, &all))?;
    let cameras = rig_estimate(truth, &export.cameras, &export.cam_se3_rig)?;
    let (he, target) = rig_handeye(truth, &export.handeye)?;
    let rows = rig_rows(
        truth,
        export.mean_reproj_error,
        &cameras,
        Some((he, target)),
        |i| {
            if run.scheimpflug {
                tilt_rows(&cameras[i], &truth.gt.cameras[i])
            } else {
                vec![]
            }
        },
    );
    let estimate = Estimate {
        cameras,
        handeye: Some(he),
        laser_planes: vec![],
        offsets: None,
    };
    Ok(finish(
        truth,
        obs,
        gated,
        Solved {
            workflow: Workflow::RigHandeye,
            cameras: all,
            rows,
            counts: (count, points, 0),
            residuals: residuals(&export.diagnostics, export.mean_reproj_error, n),
            estimate,
            target: Some(target),
            export: json,
            views: rig_views_used(obs),
            quality,
            uncertainty: predicted(&export.uncertainty),
            rig_se3_target: export.rig_se3_target.clone(),
        },
    ))
}

pub(crate) fn rig_laser(
    truth: &Truth,
    obs: &Observations,
    poses: &[Isometry3<f64>],
    gated: bool,
    run: &StageRun,
) -> Result<Report> {
    let n = truth.gt.cameras.len();
    let all: Vec<usize> = (0..n).collect();
    let (captures, points) = rig_captures(obs);
    let mut pixels = 0;
    let views: Vec<Value> = captures
        .into_iter()
        .map(|(k, cams)| {
            let lasers: Vec<Option<Vec<[f64; 2]>>> =
                obs.captures[k].views.iter().map(laser_pixels).collect();
            pixels += lasers.iter().flatten().map(Vec::len).sum::<usize>();
            json!({
                "obs": {"cameras": cams, "laser_pixels": lasers},
                "meta": {"base_se3_gripper": poses[k]},
            })
        })
        .collect();
    let count = views.len();
    let input: rig_handeye_laserline::RigHandeyeLaserlineInput =
        strict(json!({"views": views, "num_cameras": n}), "laserline input")?;
    let mut session =
        CalibrationSession::<rig_handeye_laserline::RigHandeyeLaserlineProblem>::new();
    session.set_input(input)?;
    session.set_config(config(&run.patch)?)?;
    rig_handeye_laserline::run_calibration(&mut session)?;
    let export = session.export()?;
    let (json, quality) = exported(&export, &sizes(truth, &all))?;
    let cameras = rig_estimate(truth, &export.cameras, &export.cam_se3_rig)?;
    let planes: Vec<Option<Plane>> = (0..n)
        .map(|i| export.laser_planes_cam.get(i).map(plane_of))
        .collect();
    let (he, target) = rig_handeye(truth, &export.handeye)?;
    let rows = rig_rows(
        truth,
        export.mean_reproj_error,
        &cameras,
        Some((he, target)),
        |i| {
            let mut rows = if run.scheimpflug {
                tilt_rows(&cameras[i], &truth.gt.cameras[i])
            } else {
                vec![]
            };
            if let (Some(e), Some(t)) = (&planes[i], truth.plane(i)) {
                rows.extend(plane_rows(e, t, &truth.gt.cameras[i].id));
            }
            rows
        },
    );
    let estimate = Estimate {
        cameras,
        handeye: Some(he),
        laser_planes: planes,
        offsets: None,
    };
    Ok(finish(
        truth,
        obs,
        gated,
        Solved {
            workflow: Workflow::RigHandeyeLaserline,
            cameras: all,
            rows,
            counts: (count, points, pixels),
            residuals: residuals(&export.diagnostics, export.mean_reproj_error, n),
            estimate,
            target: Some(target),
            export: json,
            views: rig_views_used(obs),
            quality,
            uncertainty: predicted(&export.uncertainty),
            rig_se3_target: export.rig_se3_target.clone(),
        },
    ))
}

/// A rig's cameras and extrinsics from planar views, no robot (`rig_extrinsics`).
pub(crate) fn rig_extrinsics(
    truth: &Truth,
    obs: &Observations,
    gated: bool,
    run: &StageRun,
) -> Result<Report> {
    let n = truth.gt.cameras.len();
    let all: Vec<usize> = (0..n).collect();
    let (captures, points) = rig_captures(obs);
    let views: Vec<_> = captures
        .into_iter()
        .map(|(_, cameras)| RigView {
            obs: RigViewObs { cameras },
            meta: NoMeta,
        })
        .collect();
    let count = views.len();
    let mut session = CalibrationSession::<rig_extrinsics::RigExtrinsicsProblem>::new();
    session.set_input(RigDataset::new(views, n)?)?;
    session.set_config(config(&run.patch)?)?;
    let s = &run.seeds;
    if s.none() {
        rig_extrinsics::run_calibration(&mut session)?;
    } else {
        let manual = strict(
            s.rig_manual_init(&all).unwrap_or_else(|| json!({})),
            "intrinsics seed",
        )?;
        rig_extrinsics::step_intrinsics_init_all_with_seed(&mut session, manual, None)?;
        rig_extrinsics::step_intrinsics_optimize_all(&mut session, None)?;
        if let Some(rigs) = &s.cam_se3_rig {
            let manual = strict(
                json!({ "cam_se3_rig": rigs, "rig_se3_target": s.rig_se3_target }),
                "rig seed",
            )?;
            rig_extrinsics::step_rig_init_with_seed(&mut session, manual, None)?;
        } else {
            rig_extrinsics::step_rig_init(&mut session, None)?;
        }
        rig_extrinsics::step_rig_optimize(&mut session, None)?;
    }
    let export = session.export()?;
    let (json, quality) = exported(&export, &sizes(truth, &all))?;
    let cameras = rig_estimate(truth, &export.cameras, &export.cam_se3_rig)?;
    let rows = rig_rows(truth, export.mean_reproj_error, &cameras, None, |i| {
        if run.scheimpflug {
            tilt_rows(&cameras[i], &truth.gt.cameras[i])
        } else {
            vec![]
        }
    });
    let estimate = Estimate {
        cameras,
        handeye: None,
        laser_planes: vec![],
        offsets: None,
    };
    Ok(finish(
        truth,
        obs,
        gated,
        Solved {
            workflow: Workflow::RigExtrinsics,
            cameras: all,
            rows,
            counts: (count, points, 0),
            residuals: residuals(&export.diagnostics, export.mean_reproj_error, n),
            estimate,
            target: None,
            export: json,
            views: rig_views_used(obs),
            quality,
            uncertainty: predicted(&export.uncertainty),
            rig_se3_target: export.rig_se3_target.clone(),
        },
    ))
}
