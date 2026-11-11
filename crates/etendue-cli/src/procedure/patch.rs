//! A stage's options as calibration-rs's config: a patch over the workflow's defaults, every
//! option routed to where calibration-rs reads it (and checked to be read).

use etendue_scene::{Loss, Problem, Stage};
use serde_json::{Value, json};

use crate::calibrate::{SeedValues, handeye_mode};

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
pub(super) fn patch(
    s: &Stage,
    eye_in_hand: bool,
    scheimpflug: bool,
    seeds: &SeedValues,
    n: usize,
) -> Value {
    let mut camera = json!({});
    if scheimpflug {
        camera["model"] = json!({"sensor": "scheimpflug"});
    }
    if let Some(m) = &s.model {
        // An untilted model has no tilt to hold: its sensor list is moot, and calibration-rs
        // refuses one.
        let sensor = if scheimpflug { &m.fix.sensor } else { &None };
        for (slot, list) in [
            ("intrinsics", &m.fix.intrinsics),
            ("distortion", &m.fix.distortion),
            ("sensor", sensor),
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
        if s.problem == Problem::RigHandeye {
            put(
                v,
                &["handeye_optimize", "refine_cam_se3_rig"],
                f.refine_rig.map(Value::from),
            );
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procedure::stage;
    use etendue_scene::Problem;
    use serde_json::json;

    /// `patch` routes every option a workflow takes (`Problem::takes`, the table validation and
    /// the studio read) into calibration-rs's config, where calibration-rs reads it, and no
    /// option the workflow does not take.
    #[test]
    fn every_option_a_workflow_takes_reaches_its_config_and_no_other() {
        let value = |key: &str| match key {
            "fix_poses" => json!([0]),
            "robot_rot_sigma_rad" | "robot_trans_sigma_m" | "min_motion_angle_deg" => json!(0.5),
            _ => json!(true),
        };
        for problem in Problem::ALL {
            let bare = stage("s", problem);
            let base = patch(&bare, true, true, &SeedValues::default(), 2);
            crate::calibrate::check_config(problem, &base).unwrap();
            for key in etendue_scene::FREEDOM_KEYS {
                let mut s = bare.clone();
                s.free = serde_json::from_value(json!({ key: value(key) })).unwrap();
                let with = patch(&s, true, true, &SeedValues::default(), 2);
                if problem.takes(key) {
                    assert_ne!(with, base, "{key} does not reach {problem:?}'s config");
                    crate::calibrate::check_config(problem, &with)
                        .unwrap_or_else(|e| panic!("{key} on {problem:?}: {e:#}"));
                } else {
                    assert_eq!(
                        with, base,
                        "{key} changes {problem:?}'s config, which does not take it"
                    );
                }
            }
        }
    }
}
