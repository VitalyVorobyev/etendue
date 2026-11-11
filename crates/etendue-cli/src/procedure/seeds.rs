//! Where a stage's starting values come from: the design (nominal), the truth, or an earlier
//! stage's estimate.

use anyhow::{Context, Result, bail, ensure};
use etendue_scene::{Seed, Stage};
use nalgebra::Isometry3;

use crate::calibrate::{Report, SeedValues, Truth, Workflow};

/// The design's starting values, as a user would enter them from datasheets: per camera.
pub(super) fn nominal(truth: &Truth) -> (Vec<[f64; 4]>, Vec<[f64; 2]>) {
    let cams: Vec<_> = truth.gt.cameras.iter().collect();
    crate::calibrate::nominal_values(&cams)
}

/// The true camera parameters, per camera.
/// Per camera: intrinsics, distortion (where it is Brown-Conrady or none), tilt.
type CameraValues = (Vec<[f64; 4]>, Vec<Option<[f64; 5]>>, Vec<[f64; 2]>);

fn true_cameras(truth: &Truth) -> CameraValues {
    let mut k = Vec::new();
    let mut d = Vec::new();
    let mut t = Vec::new();
    for c in &truth.gt.cameras {
        let i = &c.params.intrinsics;
        k.push([i.fx, i.fy, i.cx, i.cy]);
        d.push(crate::calibrate::distortion_of(&c.params));
        t.push(crate::calibrate::tilt_of(&c.params));
    }
    (k, d, t)
}

/// The reports of the stages run so far, by stage id.
pub(super) type Done = Vec<(String, Vec<Report>)>;

fn reports_of<'a>(done: &'a Done, id: &str) -> Result<&'a [Report]> {
    done.iter()
        .find(|(s, _)| s == id)
        .map(|(_, r)| r.as_slice())
        .with_context(|| format!("no earlier stage `{id}`"))
}

/// Camera `id`'s estimate in an earlier stage.
pub(super) fn estimated<'a>(
    reports: &'a [Report],
    id: &str,
) -> Option<&'a crate::calibrate::EstimatedCamera> {
    reports
        .iter()
        .flat_map(|r| &r.estimate.cameras)
        .find(|c| c.id == id)
}

/// Resolve `s`'s seeds for the stage's cameras `cams` (all others keep the truth's slots
/// empty-equivalent: only `cams` are read by the workflows).
pub(super) fn seeds(
    truth: &Truth,
    s: &Stage,
    cams: &[usize],
    done: &Done,
    scheimpflug: bool,
) -> Result<SeedValues> {
    let n = truth.gt.cameras.len();
    let ids: Vec<&str> = truth.gt.cameras.iter().map(|c| c.id.as_str()).collect();
    let (nk, nt) = nominal(truth);
    let (tk, td, tt) = true_cameras(truth);
    let mut out = SeedValues::default();
    match &s.seed.intrinsics {
        None => {}
        Some(Seed::Nominal) => {
            out.intrinsics = Some(nk);
            out.distortion = None;
        }
        Some(Seed::Truth) => {
            out.intrinsics = Some(tk.clone());
            out.distortion = Some(td);
        }
        Some(Seed::Stage(id)) => {
            let reports = reports_of(done, id)?;
            let mut k = vec![[f64::NAN; 4]; n];
            let mut d = vec![None; n];
            for &i in cams {
                let e = estimated(reports, ids[i]).with_context(|| {
                    format!("stage `{id}` did not calibrate camera `{}`", ids[i])
                })?;
                k[i] = e.intrinsics;
                d[i] = Some(e.distortion);
            }
            out.intrinsics = Some(k);
            out.distortion = Some(d);
        }
    }
    // A sensor seed applies to a Scheimpflug model only (an untilted camera takes no tilt).
    if scheimpflug {
        out.sensor = match &s.seed.sensor {
            None => None,
            Some(Seed::Nominal) => Some(nt),
            Some(Seed::Truth) => Some(tt),
            Some(Seed::Stage(id)) => {
                let reports = reports_of(done, id)?;
                let mut t = vec![[0.0; 2]; n];
                for &i in cams {
                    t[i] = estimated(reports, ids[i])
                        .with_context(|| {
                            format!("stage `{id}` did not calibrate camera `{}`", ids[i])
                        })?
                        .tilt;
                }
                Some(t)
            }
        };
    }
    if let Some(seed) = &s.seed.rig {
        let Seed::Stage(id) = seed else {
            bail!(
                "a rig seeds only from an earlier stage (its per-view target poses come with it)"
            );
        };
        let reports = reports_of(done, id)?;
        let from = reports
            .iter()
            .find(|r| !r.rig_se3_target.is_empty())
            .with_context(|| format!("stage `{id}` did not calibrate a rig"))?;
        out.cam_se3_rig = Some(
            ids.iter()
                .map(|cid| {
                    estimated(reports, cid)
                        .map(|e| e.cam_se3_ref)
                        .with_context(|| {
                            format!("stage `{id}` did not place camera `{cid}` in the rig")
                        })
                })
                .collect::<Result<_>>()?,
        );
        out.rig_se3_target = Some(from.rig_se3_target.clone());
    }
    let frame_of = |cams: &[usize]| -> Isometry3<f64> {
        if s.problem.rig() {
            truth.cam_se3_rig[0]
        } else {
            truth.cam_se3_rig[cams[0]]
        }
    };
    out.handeye = match &s.seed.handeye {
        None => None,
        Some(Seed::Nominal | Seed::Truth) => Some(truth.handeye(&frame_of(cams))),
        Some(Seed::Stage(id)) => {
            let reports = reports_of(done, id)?;
            let from = reports
                .iter()
                .find(|r| r.estimate.handeye.is_some())
                .with_context(|| format!("stage `{id}` gave no hand-eye"))?;
            let he = from.estimate.handeye.expect("found");
            let from_rig = from.cameras.len() > 1 || from.workflow == Workflow::RigHandeye;
            if s.problem.rig() || !from_rig {
                // The same frame: a rig's to a rig, a camera's to the same camera.
                ensure!(
                    s.problem.rig() == from_rig
                        && (s.problem.rig() || from.cameras == [ids[cams[0]]]),
                    "the hand-eye of stage `{id}` is in another frame than this stage's"
                );
                Some(he)
            } else {
                // A rig's hand-eye to one of its cameras.
                let cam = estimated(reports, ids[cams[0]]).with_context(|| {
                    format!("stage `{id}` did not place camera `{}`", ids[cams[0]])
                })?;
                Some(if truth.eye_in_hand()? {
                    he * cam.cam_se3_ref.inverse()
                } else {
                    cam.cam_se3_ref * he
                })
            }
        }
    };
    Ok(out)
}
