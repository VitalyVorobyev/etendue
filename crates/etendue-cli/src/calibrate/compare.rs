//! The comparison with the truth: the report's rows (pose, intrinsics, tilt, plane, hand-eye,
//! triangulation) and the estimate's offsets from the true frames.

use anyhow::Result;
use etendue_synth::dataset::{GtCamera, GtHandeye};
use etendue_synth::laser::GtPlane;
use nalgebra::{Isometry3, Point2, Point3, Vector3};

use super::gate;
use super::report::{Estimate, EstimatedCamera, Offsets, Plane, Report, Row};
use super::truth::{Truth, distortion_of, tilt_of};
use super::upstream::params_of;

/// Rotation angle (deg) and translation distance (mm) between two poses, as the script measures
/// them: the angle of `truth⁻¹·estimate` from its matrix (atan2, precise near zero), and the
/// distance between the translations.
pub(super) fn difference(estimate: &Isometry3<f64>, truth: &Isometry3<f64>) -> (f64, f64) {
    let d = (truth.inverse() * estimate).rotation.to_rotation_matrix();
    let r = d.matrix();
    let sin = 0.5
        * Vector3::new(
            r[(2, 1)] - r[(1, 2)],
            r[(0, 2)] - r[(2, 0)],
            r[(1, 0)] - r[(0, 1)],
        )
        .norm();
    let angle = sin.atan2((r.trace() - 1.0) / 2.0).to_degrees();
    (
        angle,
        (estimate.translation.vector - truth.translation.vector).norm() * 1e3,
    )
}

pub(super) fn intrinsics_rows(e: &EstimatedCamera, truth: &GtCamera) -> Vec<Row> {
    let k = &truth.params.intrinsics;
    let name = &truth.id;
    let [fx, fy, cx, cy] = e.intrinsics;
    let mut rows = vec![
        Row::new(
            format!("{name} fx rel"),
            (fx - k.fx).abs() / k.fx,
            Some(gate::FOCAL_REL),
        ),
        Row::new(
            format!("{name} fy rel"),
            (fy - k.fy).abs() / k.fy,
            Some(gate::FOCAL_REL),
        ),
        Row::new(
            format!("{name} cx px"),
            (cx - k.cx).abs(),
            Some(gate::PRINCIPAL_PX),
        ),
        Row::new(
            format!("{name} cy px"),
            (cy - k.cy).abs(),
            Some(gate::PRINCIPAL_PX),
        ),
    ];
    if let Some(d) = distortion_of(&truth.params) {
        for (i, c) in ["k1", "k2", "k3", "p1", "p2"].iter().enumerate() {
            rows.push(Row::new(
                format!("{name} {c} abs"),
                (e.distortion[i] - d[i]).abs(),
                None,
            ));
        }
    }
    rows
}

pub(super) fn tilt_rows(e: &EstimatedCamera, truth: &GtCamera) -> Vec<Row> {
    let [tx, ty] = tilt_of(&truth.params);
    vec![
        Row::new(
            format!("{} tilt_x deg", truth.id),
            (e.tilt[0] - tx).to_degrees().abs(),
            None,
        ),
        Row::new(
            format!("{} tilt_y deg", truth.id),
            (e.tilt[1] - ty).to_degrees().abs(),
            None,
        ),
    ]
}

/// Normal angle and offset difference of two planes `n·p + d = 0`, sign-free.
pub(super) fn plane_rows(e: &Plane, truth: &GtPlane, name: &str) -> Vec<Row> {
    let (mut ne, mut de) = (Vector3::from(e.normal), e.distance);
    let (mut nt, mut dt) = (Vector3::from(truth.normal), truth.distance);
    let (se, st) = (ne.norm(), nt.norm());
    ne /= se;
    de /= se;
    nt /= st;
    dt /= st;
    if ne.dot(&nt) < 0.0 {
        ne = -ne;
        de = -de;
    }
    let angle = ne.cross(&nt).norm().atan2(ne.dot(&nt)).to_degrees();
    vec![
        Row::new(
            format!("{name} laser normal deg"),
            angle,
            Some(gate::PLANE_NORMAL_DEG),
        ),
        Row::new(
            format!("{name} laser distance mm"),
            (de - dt).abs() * 1e3,
            Some(gate::PLANE_DISTANCE_MM),
        ),
    ]
}

/// Each laser camera's stripe triangulated as the calibrated head measures it, against the truth
/// (G5.3c): every true stripe pixel back-projected with the estimated camera, met with the
/// estimated laser plane, and carried into the robot base through the estimated hand-eye and the
/// true robot pose. Unlike the camera-frame rows, this does not depend on how an estimate splits
/// the projection among sensor tilt, principal point and camera frame
/// (`docs/derivations/scheimpflug_observability.md` §5): it is what a measurement inherits. It
/// takes a report's estimate as a whole, so it also catches parts that come from different camera
/// solutions.
///
/// # Errors
///
/// When an estimated camera does not make a model.
pub(crate) fn triangulation_rows(truth: &Truth, estimate: &Estimate) -> Result<Vec<Row>> {
    let (Some(he), Some(handeye)) = (estimate.handeye, truth.gt.handeye.as_ref()) else {
        return Ok(vec![]);
    };
    let mut rows = Vec::new();
    for (cam, plane) in estimate.cameras.iter().zip(&estimate.laser_planes) {
        let Some(plane) = plane else { continue };
        let Some(i) = truth.gt.cameras.iter().position(|c| c.id == cam.id) else {
            continue;
        };
        let Some(laser) = truth.laser(i) else {
            continue;
        };
        let model = params_of(cam, &truth.gt.cameras[i].params).build()?;
        let (n, d) = (Vector3::from(plane.normal), plane.distance);
        let ref_se3_cam = cam.cam_se3_ref.inverse();
        let (mut sq, mut max, mut count) = (0.0, 0.0_f64, 0_usize);
        for c in &truth.gt.captures {
            let Some(base_se3_tcp) = c.base_se3_tcp else {
                continue;
            };
            let (base_se3_cam, base_se3_target) = match handeye {
                GtHandeye::EyeInHand {
                    base_se3_target, ..
                } => (base_se3_tcp * he * ref_se3_cam, *base_se3_target),
                GtHandeye::EyeToHand {
                    gripper_se3_target, ..
                } => (
                    he.inverse() * ref_se3_cam,
                    base_se3_tcp * gripper_se3_target,
                ),
            };
            let stripe = c.laser_stripes.iter().find(|s| s.laser == laser.id);
            let seen = c.views[i].laser_points.iter().find(|s| s.laser == laser.id);
            let (Some(stripe), Some(seen)) = (stripe, seen) else {
                continue;
            };
            for p in seen.points.iter().filter(|p| p.occluded.is_none()) {
                let (Some([u, v]), Some(b)) = (p.pixel, stripe.points.get(p.point)) else {
                    continue;
                };
                let ray = model.backproject_pixel(&Point2::new(u, v)).direction;
                let measured = base_se3_cam * Point3::from(ray * (-d / n.dot(&ray)));
                let [x, y, z] = b.xyz_m.unwrap_or([b.position_m[0], b.position_m[1], 0.0]);
                let e = (measured - base_se3_target * Point3::new(x, y, z)).norm();
                sq += e * e;
                max = max.max(e);
                count += 1;
            }
        }
        if count > 0 {
            rows.push(Row::new(
                format!("{} triangulated rms mm", cam.id),
                (sq / count as f64).sqrt() * 1e3,
                None,
            ));
            rows.push(Row::new(
                format!("{} triangulated max mm", cam.id),
                max * 1e3,
                Some(gate::TRIANGULATION_MM),
            ));
        }
    }
    Ok(rows)
}

/// `report` with its [`triangulation_rows`]: the result of a procedure, as a whole.
pub(crate) fn with_triangulation(truth: &Truth, mut report: Report) -> Result<Report> {
    let rows = triangulation_rows(truth, &report.estimate)?;
    report.rows.extend(rows);
    report.pass = !report.gated || report.rows.iter().all(Row::within);
    Ok(report)
}

pub(super) fn handeye_rows(
    truth: &Truth,
    (he, target): (Isometry3<f64>, Isometry3<f64>),
    ref_se3_rig: &Isometry3<f64>,
) -> Vec<Row> {
    let (r, t) = difference(&he, &truth.handeye(ref_se3_rig));
    let (name, target_truth) = truth.target();
    let (tr, tt) = difference(&target, &target_truth);
    vec![
        Row::new("hand-eye rotation deg", r, Some(gate::HANDEYE_ROT_DEG)),
        Row::new("hand-eye translation mm", t, Some(gate::HANDEYE_TRANS_MM)),
        Row::new(format!("{name} rot deg"), tr, None),
        Row::new(format!("{name} trans mm"), tt, None),
    ]
}

/// The estimated camera `j` (truth camera `i`) in the true camera's frame.
fn camera_offset(
    truth: &Truth,
    estimate: &Estimate,
    handeye: &Isometry3<f64>,
    i: usize,
    j: usize,
) -> Option<Isometry3<f64>> {
    let cam_se3_ref = estimate.cameras.get(j)?.cam_se3_ref;
    let true_x = truth.handeye(&truth.cam_se3_rig[i]);
    Some(match truth.gt.handeye.as_ref()? {
        // gripper_se3_camera
        GtHandeye::EyeInHand { .. } => true_x.inverse() * (handeye * cam_se3_ref.inverse()),
        // camera_se3_base
        GtHandeye::EyeToHand { .. } => true_x * (cam_se3_ref * handeye).inverse(),
    })
}

/// A plane with a unit normal, `n·p + d = 0`.
pub(super) fn unit(p: &Plane) -> Plane {
    let n = Vector3::from(p.normal);
    let s = n.norm();
    Plane {
        normal: (n / s).into(),
        distance: p.distance / s,
    }
}

pub(crate) fn offsets(
    truth: &Truth,
    cameras: &[usize],
    estimate: &Estimate,
    target: Option<Isometry3<f64>>,
) -> Option<Offsets> {
    let handeye = estimate.handeye?;
    let cams: Vec<Isometry3<f64>> = cameras
        .iter()
        .enumerate()
        .map(|(j, &i)| camera_offset(truth, estimate, &handeye, i, j))
        .collect::<Option<_>>()?;
    let planes = cameras
        .iter()
        .enumerate()
        .map(|(j, &i)| {
            let e = estimate.laser_planes.get(j)?.as_ref()?;
            let t = truth.plane(i)?;
            let t = unit(&Plane {
                normal: t.normal,
                distance: t.distance,
            });
            // n·p_e + d = 0 with p_e = R⁻¹(p − t): (R n)·p + d − (R n)·t = 0.
            let e = unit(e);
            let n = cams[j].rotation * Vector3::from(e.normal);
            let mut e = Plane {
                normal: n.into(),
                distance: e.distance - n.dot(&cams[j].translation.vector),
            };
            if n.dot(&Vector3::from(t.normal)) < 0.0 {
                e = Plane {
                    normal: (-n).into(),
                    distance: -e.distance,
                };
            }
            Some([t, e])
        })
        .collect();
    Some(Offsets {
        cameras: cams,
        target: truth.target().1.inverse() * target?,
        planes,
    })
}

/// `report` with the laser planes (and their rows) of `from`, an earlier stage's report, when
/// its own workflow estimated none: a procedure's result keeps the latest planes it has.
pub(crate) fn with_planes(truth: &Truth, mut report: Report, from: &Report) -> Report {
    let has = |r: &Report| r.estimate.laser_planes.iter().any(Option::is_some);
    if has(&report) || !has(from) {
        return report;
    }
    let plane_rows: Vec<Row> = from
        .rows
        .iter()
        .filter(|r| {
            r.label.ends_with(" laser normal deg") || r.label.ends_with(" laser distance mm")
        })
        .cloned()
        .collect();
    let at = report
        .rows
        .iter()
        .position(|r| r.label == "hand-eye rotation deg")
        .unwrap_or(report.rows.len());
    report.rows.splice(at..at, plane_rows);
    report.estimate.laser_planes = from.estimate.laser_planes.clone();
    let cameras: Vec<usize> = report
        .cameras
        .iter()
        .filter_map(|id| truth.gt.cameras.iter().position(|c| &c.id == id))
        .collect();
    let target = report
        .estimate
        .offsets
        .as_ref()
        .map(|o| truth.target().1 * o.target);
    report.estimate.offsets = offsets(truth, &cameras, &report.estimate, target);
    report.pass = !report.gated || report.rows.iter().all(Row::within);
    report
}

pub(super) fn reprojection_row(mean_px: f64) -> Row {
    Row::new("mean reprojection px", mean_px, Some(gate::MEAN_REPROJ_PX))
}
