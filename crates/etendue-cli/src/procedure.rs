//! Calibration procedures (ADR 0011): run a [`ProcedurePart`]'s stages against a dataset.
//!
//! Each stage is resolved against the dataset into calibration-rs's own config (a patch over the
//! workflow's defaults) and seeds (`StageRun`), then run by the workflow functions of
//! [`crate::calibrate`]. The procedure's report is its last stage's, with the laser planes of the
//! latest stage that estimated them, and a summary of every stage.
//!
//! `auto` is today's choice by the dataset's shape, written as built-in procedures
//! ([`auto`]); [`builtin`] are the procedures etendue ships for people to start from.

use anyhow::{Context, Result, bail, ensure};
use etendue_scene::{
    CameraChoice, Freedoms, LIBRARY_VERSION, LibraryItem, Loss, Problem, ProcedurePart, Seed,
    Seeds, SensorChoice, SolverChoice, Stage,
};
use etendue_synth::dataset::GroundTruth;
use etendue_synth::observe::{ObservationNoise, Observations};
use nalgebra::Isometry3;
use serde_json::{Value, json};
use vision_calibration_core::{DistortionParams, SensorParams};

use crate::calibrate::{
    Options, Report, SeedValues, StageRun, StageSummary, Truth, Workflow, check_observations,
    handeye_mode, robot_poses, with_planes,
};

fn stage(id: &str, problem: Problem) -> Stage {
    Stage {
        id: id.into(),
        problem,
        camera: None,
        model: None,
        free: Freedoms::default(),
        solver: None,
        seed: Seeds::default(),
    }
}

/// The procedure `etendue calibrate` runs without `--procedure`: by the dataset's shape, as before
/// procedures existed. Returns its id and stages.
///
/// # Errors
///
/// `options.camera` on a laser dataset, or a camera the dataset does not have.
pub fn auto(gt: &GroundTruth, options: &Options) -> Result<(String, ProcedurePart)> {
    let ids: Vec<&str> = gt.cameras.iter().map(|c| c.id.as_str()).collect();
    if let Some(name) = &options.camera {
        ensure!(
            gt.lasers.is_empty(),
            "--camera does not apply to a laser dataset"
        );
        ensure!(
            ids.contains(&name.as_str()),
            "no camera `{name}` (cameras: {ids:?})"
        );
        let mut s = stage("single_cam_handeye", Problem::SingleCamHandeye);
        s.camera = Some(name.clone());
        return Ok(("auto_single".into(), ProcedurePart { stages: vec![s] }));
    }
    let truth = Truth::new(gt);
    let all: Vec<usize> = (0..gt.cameras.len()).collect();
    Ok(match (gt.cameras.len(), gt.lasers.is_empty()) {
        (1, true) => (
            "auto_single".into(),
            ProcedurePart {
                stages: vec![stage("single_cam_handeye", Problem::SingleCamHandeye)],
            },
        ),
        (_, true) => (
            "auto_rig".into(),
            ProcedurePart {
                stages: vec![stage("rig_handeye", Problem::RigHandeye)],
            },
        ),
        (1, false) => {
            let mut s = stage("laserline_device", Problem::LaserlineDevice);
            // No target pose is held: a single camera has no gauge to fix, and the linear
            // estimate of any one pose is not exact.
            s.free.fix_poses = Some(vec![]);
            s.solver = Some(SolverChoice {
                max_iters: Some(200),
                robust_loss: None,
            });
            s.seed.sensor = Some(Seed::Nominal);
            ("auto_head".into(), ProcedurePart { stages: vec![s] })
        }
        (_, false) => {
            let mut s = stage("rig_handeye_laserline", Problem::RigHandeyeLaserline);
            if truth.scheimpflug(&all) {
                // A Scheimpflug rig is seeded from nominal values (calibration-rs ADR 0022).
                s.free.refine_intrinsics = Some(true);
                s.free.refine_sensor = Some(true);
                s.seed.intrinsics = Some(Seed::Nominal);
                s.seed.sensor = Some(Seed::Nominal);
                (
                    "auto_rig_laser_scheimpflug".into(),
                    ProcedurePart { stages: vec![s] },
                )
            } else {
                ("auto_rig_laser".into(), ProcedurePart { stages: vec![s] })
            }
        }
    })
}

fn item(id: &str, name: &str, description: &str, stages: Vec<Stage>) -> LibraryItem {
    let mut item: LibraryItem = serde_json::from_value(json!({
        "version": LIBRARY_VERSION, "id": id, "name": name, "description": description,
    }))
    .expect("a procedure item");
    item.procedure = Some(ProcedurePart { stages });
    item
}

fn seeded(id: &str, problem: Problem, from: &str, rig: bool) -> Stage {
    let mut s = stage(id, problem);
    s.seed.intrinsics = Some(Seed::Stage(from.into()));
    if rig {
        s.seed.rig = Some(Seed::Stage(from.into()));
    }
    s
}

/// The procedures etendue ships (`examples/library/procedures/`, the built-in layer).
#[must_use]
pub fn builtin() -> Vec<LibraryItem> {
    let intrinsics = stage("intrinsics", Problem::PlanarIntrinsics);
    let mut handeye = seeded("handeye", Problem::SingleCamHandeye, "intrinsics", false);
    handeye.free.refine_robot_poses = Some(true);
    let mut rig_he = seeded("handeye", Problem::RigHandeye, "rig", true);
    rig_he.free.refine_robot_poses = Some(true);
    let mut device = stage("device", Problem::LaserlineDevice);
    device.free.fix_poses = Some(vec![]);
    device.solver = Some(SolverChoice {
        max_iters: Some(200),
        robust_loss: None,
    });
    device.seed.sensor = Some(Seed::Nominal);
    let mut head_he = stage("handeye", Problem::SingleCamHandeye);
    head_he.model = Some(CameraChoice {
        sensor: SensorChoice::Design,
        ..CameraChoice::default()
    });
    head_he.seed.intrinsics = Some(Seed::Stage("device".into()));
    head_he.seed.sensor = Some(Seed::Stage("device".into()));
    let mut joint = stage("joint", Problem::RigHandeyeLaserline);
    joint.free.refine_intrinsics = Some(true);
    joint.free.refine_sensor = Some(true);
    joint.seed.intrinsics = Some(Seed::Nominal);
    joint.seed.sensor = Some(Seed::Nominal);
    vec![
        item(
            "intrinsics",
            "Intrinsics",
            "Each camera's intrinsics and distortion from its own planar views; no robot.",
            vec![intrinsics.clone()],
        ),
        item(
            "stereo",
            "Stereo or multi-camera rig",
            "Each camera's intrinsics, then the rig's extrinsics from the shared views, refining the intrinsics jointly; no robot.",
            vec![
                intrinsics.clone(),
                seeded("rig", Problem::RigExtrinsics, "intrinsics", false),
            ],
        ),
        item(
            "handeye",
            "Hand-eye, one camera",
            "The camera's intrinsics from planar views, then the hand-eye (eye-in-hand or eye-to-hand) with the robot poses refined under their prior. On a rig, each camera on its own.",
            vec![intrinsics.clone(), handeye],
        ),
        item(
            "rig_handeye",
            "Hand-eye, rig",
            "Each camera's intrinsics, the rig's extrinsics, then the rig's hand-eye with the robot poses refined.",
            vec![
                intrinsics,
                seeded("rig", Problem::RigExtrinsics, "intrinsics", false),
                rig_he,
            ],
        ),
        item(
            "laser_head",
            "Laser triangulation head",
            "The camera, its sensor tilt and its laser plane from the device workflow, seeded from the datasheet tilt; then the hand-eye with the same camera model, seeded from that result.",
            vec![device, head_he],
        ),
        item(
            "laser_rig",
            "Rig with lasers",
            "Cameras, rig, laser planes and hand-eye in one joint solve, seeded from the datasheet focal lengths and tilts.",
            vec![joint],
        ),
    ]
}

/// Find a procedure by id among [`builtin`] and `auto`'s.
#[must_use]
pub fn find_builtin(id: &str) -> Option<LibraryItem> {
    builtin().into_iter().find(|p| p.id == id)
}

fn loss_json(l: Loss) -> Value {
    match l {
        Loss::None => json!("None"),
        Loss::Huber { scale } => json!({"Huber": {"scale": scale}}),
        Loss::Cauchy { scale } => json!({"Cauchy": {"scale": scale}}),
        Loss::Arctan { scale } => json!({"Arctan": {"scale": scale}}),
    }
}

/// Set `value` at `path` in `v` (creating objects), when there is a value.
fn put(v: &mut Value, path: &[&str], value: Option<Value>) {
    let Some(value) = value else { return };
    let mut at = v;
    for key in &path[..path.len() - 1] {
        at = at
            .as_object_mut()
            .expect("an object")
            .entry(*key)
            .or_insert_with(|| json!({}));
    }
    at[path[path.len() - 1]] = value;
}

/// calibration-rs's config patch for `s` (see [`Freedoms`] for where each option goes).
fn patch(s: &Stage, eye_in_hand: bool, scheimpflug: bool, seeds: &SeedValues, n: usize) -> Value {
    let mut camera = json!({});
    if scheimpflug {
        camera["model"] = json!({"sensor": "scheimpflug"});
    }
    if let Some(m) = &s.model {
        for (slot, list) in [
            ("intrinsics", &m.fix.intrinsics),
            ("distortion", &m.fix.distortion),
            ("sensor", &m.fix.sensor),
        ] {
            put(&mut camera, &["fix", slot], list.as_ref().map(|l| json!(l)));
        }
    }
    let camera = (camera != json!({})).then_some(camera);
    let f = &s.free;
    let mut robot = json!({});
    put(
        &mut robot,
        &["refine"],
        f.refine_robot_poses.map(Value::from),
    );
    put(
        &mut robot,
        &["rot_sigma"],
        f.robot_rot_sigma_rad.map(Value::from),
    );
    put(
        &mut robot,
        &["trans_sigma"],
        f.robot_trans_sigma_m.map(Value::from),
    );
    let robot = (robot != json!({})).then_some(robot);
    let mut solver = json!({});
    if let Some(c) = &s.solver {
        put(&mut solver, &["max_iters"], c.max_iters.map(Value::from));
        put(&mut solver, &["robust_loss"], c.robust_loss.map(loss_json));
    }
    let solver = (solver != json!({})).then_some(solver);
    let mut handeye_init = json!({"handeye_mode": handeye_mode(eye_in_hand)});
    put(
        &mut handeye_init,
        &["min_motion_angle_deg"],
        f.min_motion_angle_deg.map(Value::from),
    );
    let all: Vec<usize> = (0..n).collect();
    let rig_handeye = |v: &mut Value| {
        put(v, &["handeye_init"], Some(handeye_init.clone()));
        put(v, &["camera"], camera.clone());
        put(
            v,
            &["rig", "refine_intrinsics"],
            f.refine_intrinsics.map(Value::from),
        );
        put(
            v,
            &["rig", "refine_sensor"],
            f.refine_sensor.map(Value::from),
        );
        put(
            v,
            &["handeye_optimize", "refine_cam_se3_rig"],
            f.refine_rig.map(Value::from),
        );
        put(v, &["handeye_optimize", "robot_poses"], robot.clone());
        put(v, &["solver"], solver.clone());
        put(v, &["manual_init"], seeds.rig_manual_init(&all));
    };
    let mut v = json!({});
    match s.problem {
        Problem::SingleCamHandeye => {
            put(&mut v, &["handeye_init"], Some(handeye_init.clone()));
            put(&mut v, &["camera"], camera);
            put(&mut v, &["robot_poses"], robot);
            put(&mut v, &["solver"], solver);
        }
        Problem::RigHandeye => rig_handeye(&mut v),
        Problem::RigHandeyeLaserline => {
            let mut he = json!({});
            rig_handeye(&mut he);
            v["handeye"] = he;
            put(
                &mut v,
                &["joint_optimize", "fix_handeye"],
                f.fix_handeye.map(Value::from),
            );
            put(
                &mut v,
                &["joint_optimize", "fix_target_ref"],
                f.fix_target.map(Value::from),
            );
            put(&mut v, &["joint_optimize", "robot_poses"], robot);
            put(&mut v, &["joint_optimize", "solver"], solver);
        }
        Problem::LaserlineDevice => {
            put(&mut v, &["camera"], camera);
            put(
                &mut v,
                &["optimize", "fix_poses"],
                f.fix_poses.as_ref().map(|p| json!(p)),
            );
            put(
                &mut v,
                &["optimize", "fix_plane"],
                f.fix_plane.map(Value::from),
            );
            put(&mut v, &["solver"], solver);
        }
        Problem::PlanarIntrinsics => {
            put(&mut v, &["camera"], camera);
            put(
                &mut v,
                &["fix_poses"],
                f.fix_poses.as_ref().map(|p| json!(p)),
            );
            put(&mut v, &["solver"], solver);
        }
        Problem::RigExtrinsics => {
            put(&mut v, &["camera"], camera);
            put(
                &mut v,
                &["rig", "refine_intrinsics"],
                f.refine_intrinsics.map(Value::from),
            );
            put(
                &mut v,
                &["rig", "refine_sensor"],
                f.refine_sensor.map(Value::from),
            );
            put(&mut v, &["solver"], solver);
        }
    }
    v
}

/// The design's starting values, as a user would enter them from datasheets
/// ([`crate::calibrate::nominal_seed`]'s rounding): per camera.
fn nominal(truth: &Truth) -> (Vec<[f64; 4]>, Vec<[f64; 2]>) {
    let cams: Vec<_> = truth.gt.cameras.iter().collect();
    let seed = crate::calibrate::nominal_seed(&cams);
    let n = |v: &Value, k: &str| v[k].as_f64().unwrap_or(f64::NAN);
    let k = seed["per_cam_intrinsics"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| [n(v, "fx"), n(v, "fy"), n(v, "cx"), n(v, "cy")])
        .collect();
    let t = seed["per_cam_sensors"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| [n(v, "tilt_x"), n(v, "tilt_y")])
        .collect();
    (k, t)
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
        d.push(match &c.params.distortion {
            DistortionParams::BrownConrady5 { params: p } => Some([p.k1, p.k2, p.k3, p.p1, p.p2]),
            DistortionParams::None => Some([0.0; 5]),
            _ => None,
        });
        t.push(match c.params.sensor {
            SensorParams::Scheimpflug { params } => [params.tilt_x, params.tilt_y],
            _ => [0.0, 0.0],
        });
    }
    (k, d, t)
}

/// The reports of the stages run so far, by stage id.
type Done = Vec<(String, Vec<Report>)>;

fn reports_of<'a>(done: &'a Done, id: &str) -> Result<&'a [Report]> {
    done.iter()
        .find(|(s, _)| s == id)
        .map(|(_, r)| r.as_slice())
        .with_context(|| format!("no earlier stage `{id}`"))
}

/// Camera `id`'s estimate in an earlier stage.
fn estimated<'a>(reports: &'a [Report], id: &str) -> Option<&'a crate::calibrate::EstimatedCamera> {
    reports
        .iter()
        .flat_map(|r| &r.estimate.cameras)
        .find(|c| c.id == id)
}

/// Resolve `s`'s seeds for the stage's cameras `cams` (all others keep the truth's slots
/// empty-equivalent: only `cams` are read by the workflows).
fn seeds(
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
            .find(|r| r.export.get("rig_se3_target").is_some())
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
        out.rig_se3_target = Some(
            serde_json::from_value(from.export["rig_se3_target"].clone())
                .context("the rig's target poses")?,
        );
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

/// The cameras a stage runs on, each run separately for a single-camera workflow.
fn cameras_of(truth: &Truth, s: &Stage) -> Result<Vec<usize>> {
    let ids: Vec<&str> = truth.gt.cameras.iter().map(|c| c.id.as_str()).collect();
    if s.problem.rig() {
        return Ok((0..ids.len()).collect());
    }
    Ok(match &s.camera {
        Some(name) => {
            vec![ids.iter().position(|c| c == name).with_context(|| {
                format!("stage `{}`: no camera `{name}` (cameras: {ids:?})", s.id)
            })?]
        }
        None => (0..ids.len()).collect(),
    })
}

/// Check that `procedure` fits the dataset before anything runs.
///
/// # Errors
///
/// Every mismatch, in one error.
pub fn check(gt: &GroundTruth, procedure: &ProcedurePart) -> Result<()> {
    let mut problems = Vec::new();
    let truth = Truth::new(gt);
    for s in &procedure.stages {
        let name = etendue_scene::procedure::problem_name(s.problem);
        if s.problem.handeye() && gt.handeye.is_none() {
            problems.push(format!(
                "stage `{}` ({name}) needs a robot: the dataset has none",
                s.id
            ));
        }
        if s.problem.laser() && gt.lasers.is_empty() {
            problems.push(format!(
                "stage `{}` ({name}) needs a laser: the dataset has none",
                s.id
            ));
        }
        if s.problem == Problem::LaserlineDevice && gt.cameras.len() != 1 {
            problems.push(format!(
                "stage `{}` (laserline_device) calibrates a one-camera head; the dataset has {} cameras",
                s.id,
                gt.cameras.len()
            ));
        }
        if s.problem == Problem::RigHandeyeLaserline
            && (s.seed.rig.is_some() || s.seed.handeye.is_some())
        {
            problems.push(format!(
                "stage `{}`: rig_handeye_laserline takes only intrinsics and sensor seeds (calibration-rs 0.11)",
                s.id
            ));
        }
        if let Err(e) = cameras_of(&truth, s) {
            problems.push(format!("{e}"));
        }
    }
    ensure!(
        problems.is_empty(),
        "the procedure does not fit the dataset:\n  {}",
        problems.join("\n  ")
    );
    Ok(())
}

/// Run one stage: one report, or one per camera for a single-camera workflow on several.
fn run_stage(
    truth: &Truth,
    obs: &Observations,
    s: &Stage,
    done: &Done,
    gated: bool,
) -> Result<Vec<Report>> {
    let cams = cameras_of(truth, s)?;
    let n = truth.gt.cameras.len();
    let scheimpflug = |cams: &[usize]| match s.model.as_ref().map(|m| m.sensor) {
        None | Some(SensorChoice::Design) => truth.scheimpflug(cams),
        Some(SensorChoice::Identity) => false,
        Some(SensorChoice::Scheimpflug) => true,
    };
    let eye_in_hand = if s.problem.handeye() {
        truth.eye_in_hand()?
    } else {
        true
    };
    let poses = if s.problem.handeye() {
        robot_poses(obs)?
    } else {
        vec![]
    };
    let resolve = |cams: &[usize]| -> Result<StageRun> {
        let sch = scheimpflug(cams);
        let seeds = seeds(truth, s, cams, done, sch)?;
        Ok(StageRun {
            patch: patch(s, eye_in_hand, sch, &seeds, n),
            seeds,
            scheimpflug: sch,
        })
    };
    use crate::calibrate as c;
    Ok(match s.problem {
        Problem::RigHandeye => vec![c::rig(
            truth,
            obs,
            &poses,
            eye_in_hand,
            gated,
            &resolve(&cams)?,
        )?],
        Problem::RigHandeyeLaserline => {
            vec![c::rig_laser(
                truth,
                obs,
                &poses,
                eye_in_hand,
                gated,
                &resolve(&cams)?,
            )?]
        }
        Problem::RigExtrinsics => vec![c::rig_extrinsics(truth, obs, gated, &resolve(&cams)?)?],
        Problem::LaserlineDevice => {
            // The hand-eye on the same views: the pinhole single-camera workflow, as before.
            let handeye = StageRun {
                patch: json!({"handeye_init": {"handeye_mode": handeye_mode(eye_in_hand)}}),
                ..StageRun::default()
            };
            vec![c::head(
                truth,
                obs,
                &poses,
                eye_in_hand,
                gated,
                &resolve(&cams)?,
                &handeye,
            )?]
        }
        Problem::SingleCamHandeye => cams
            .iter()
            .map(|&i| c::single(truth, obs, &poses, i, eye_in_hand, gated, &resolve(&[i])?))
            .collect::<Result<_>>()?,
        Problem::PlanarIntrinsics => cams
            .iter()
            .map(|&i| c::intrinsics(truth, obs, i, gated, &resolve(&[i])?))
            .collect::<Result<_>>()?,
    })
}

/// Several single-camera reports of one stage as one: rows prefixed with their camera, counts
/// and residuals pooled.
fn pooled(mut reports: Vec<Report>) -> Report {
    if reports.len() == 1 {
        return reports.pop().expect("one");
    }
    let mut out = reports[0].clone();
    out.rows.clear();
    out.cameras.clear();
    out.estimate.cameras.clear();
    out.estimate.laser_planes.clear();
    out.estimate.handeye = None;
    out.estimate.offsets = None;
    let (mut views, mut points, mut pixels) = (0, 0, 0);
    let mut errors = Vec::new();
    let mut features = Vec::new();
    let (mut sum, mut sq, mut count) = (0.0, 0.0, 0usize);
    let mut per_camera = Vec::new();
    let mut max: f64 = 0.0;
    let mut export = serde_json::Map::new();
    for r in reports {
        let id = r.cameras.first().cloned().unwrap_or_default();
        for mut row in r.rows {
            if !row.label.starts_with(&format!("{id} ")) {
                row.label = format!("{id} {}", row.label);
            }
            out.rows.push(row);
        }
        views += r.views;
        points += r.points;
        pixels += r.laser_pixels;
        let k = r.residuals.count;
        sum += r.residuals.mean_px * k as f64;
        sq += r.residuals.rms_axis_px.powi(2) * k as f64;
        count += k;
        max = max.max(r.residuals.max_px);
        per_camera.extend(r.residuals.per_camera_mean_px);
        errors.extend(r.residuals.errors_px);
        features.extend(r.residuals.features);
        out.estimate.cameras.extend(r.estimate.cameras);
        out.estimate.laser_planes.extend(r.estimate.laser_planes);
        export.insert(id.clone(), r.export);
        out.cameras.push(id);
    }
    out.views = views;
    out.points = points;
    out.laser_pixels = pixels;
    out.residuals = crate::calibrate::Residuals {
        count,
        mean_px: if count == 0 {
            f64::NAN
        } else {
            sum / count as f64
        },
        rms_axis_px: if count == 0 {
            f64::NAN
        } else {
            (sq / count as f64).sqrt()
        },
        max_px: max,
        per_camera_mean_px: per_camera,
        features,
        errors_px: errors,
    };
    out.pass = !out.gated || out.rows.iter().all(crate::calibrate::Row::within);
    out.export = Value::Object(export);
    out
}

/// Run `procedure` (named `id`) on `obs` of `gt`'s dataset and compare its result with the truth.
///
/// # Errors
///
/// When the procedure does not fit the dataset, the observations are not the truth's, or a
/// calibration-rs workflow fails.
pub fn run(
    gt: &GroundTruth,
    obs: &Observations,
    options: &Options,
    id: &str,
    procedure: &ProcedurePart,
) -> Result<Report> {
    check_observations(gt, obs)?;
    check(gt, procedure)?;
    let truth = Truth::new(gt);
    let gated = obs.noise == ObservationNoise::default() && !options.detected;
    let mut done: Done = Vec::new();
    let mut summaries = Vec::new();
    for s in &procedure.stages {
        let reports =
            run_stage(&truth, obs, s, &done, gated).with_context(|| format!("stage `{}`", s.id))?;
        for r in &reports {
            summaries.push(StageSummary {
                id: s.id.clone(),
                workflow: r.workflow,
                cameras: r.cameras.clone(),
                views: r.views,
                points: r.points,
                mean_reproj_px: r.residuals.mean_px,
            });
        }
        done.push((s.id.clone(), reports));
    }
    let (_, last) = done.pop().context("the procedure has no stage")?;
    let mut report = pooled(last);
    // Laser planes the last stage did not estimate come from the latest stage that did.
    for (_, earlier) in done.iter().rev() {
        let had = pooled(earlier.clone());
        report = with_planes(&truth, report, &had);
    }
    if !done.is_empty() {
        // Several stages: every stage's export, by stage id.
        let mut all = serde_json::Map::new();
        for (sid, reports) in &done {
            all.insert(sid.clone(), pooled(reports.clone()).export);
        }
        let last_id = procedure.stages.last().expect("a stage").id.clone();
        all.insert(last_id, std::mem::take(&mut report.export));
        report.export = Value::Object(all);
    }
    report.procedure = id.to_owned();
    report.stages = summaries;
    Ok(report)
}

/// A procedure named on the command line: a built-in's id, or a library item file.
///
/// # Errors
///
/// When it is neither, or the file does not read or hold a procedure.
pub fn named(name: &str) -> Result<(String, ProcedurePart)> {
    if let Some(item) = find_builtin(name) {
        return Ok((item.id, item.procedure.expect("a procedure")));
    }
    let path = std::path::Path::new(name);
    if !path.is_file() {
        let ids: Vec<String> = builtin().into_iter().map(|p| p.id).collect();
        bail!(
            "`{name}` is neither a procedure file nor a built-in procedure ({})",
            ids.join(", ")
        );
    }
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {name}"))?;
    let item = crate::library::read_item(&text).map_err(|e| anyhow::anyhow!("{name}: {e}"))?;
    item.validate()
        .map_err(|e| anyhow::anyhow!("{name}: {e}"))?;
    let part = item
        .procedure
        .with_context(|| format!("{name}: holds no procedure"))?;
    Ok((item.id, part))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/library/procedures")
    }

    /// The built-ins are also files of the examples' library (the built-in layer). With
    /// `ETENDUE_WRITE_PROCEDURES=1` this writes them; otherwise it checks them.
    #[test]
    fn the_builtin_procedures_are_the_examples_files() {
        let write = std::env::var_os("ETENDUE_WRITE_PROCEDURES").is_some();
        if write {
            std::fs::create_dir_all(library()).unwrap();
        }
        for item in builtin() {
            item.validate().unwrap();
            let mut text = serde_json::to_string_pretty(&item).unwrap();
            text.push('\n');
            let path = library().join(format!("{}.json", item.id));
            if write {
                std::fs::write(&path, &text).unwrap();
            } else {
                let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
                assert_eq!(
                    on_disk,
                    text,
                    "{} is stale: run with ETENDUE_WRITE_PROCEDURES=1",
                    path.display()
                );
            }
        }
    }

    #[test]
    fn a_built_in_is_found_by_id_and_a_bad_name_lists_them() {
        assert_eq!(named("stereo").unwrap().1.stages.len(), 2);
        let err = named("nope").unwrap_err().to_string();
        assert!(err.contains("laser_head"), "{err}");
    }
}
