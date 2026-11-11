//! The procedures etendue ships: the built-in layer of the library
//! (`examples/library/procedures/`).

use etendue_scene::{
    CameraChoice, FixLists, LIBRARY_VERSION, LibraryItem, Problem, ProcedurePart, Seed,
    SensorChoice, SolverChoice, Stage,
};
use serde_json::json;

use super::stage;

pub(super) fn item(id: &str, name: &str, description: &str, stages: Vec<Stage>) -> LibraryItem {
    let mut item: LibraryItem = serde_json::from_value(json!({
        "version": LIBRARY_VERSION, "id": id, "name": name, "description": description,
    }))
    .expect("a procedure item");
    item.procedure = Some(ProcedurePart { stages });
    item
}

pub(super) fn seeded(id: &str, problem: Problem, from: &str, rig: bool) -> Stage {
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
    // The hand-eye holds the device stage's camera: the laser plane is in that camera's frame,
    // and a camera re-estimated here would settle on another frame (and other rays) for it.
    let held = |names: &[&str]| Some(names.iter().map(|n| (*n).to_owned()).collect());
    let mut head_he = stage("handeye", Problem::SingleCamHandeye);
    head_he.model = Some(CameraChoice {
        sensor: SensorChoice::Design,
        fix: FixLists {
            intrinsics: held(&["fx", "fy", "cx", "cy"]),
            distortion: held(&["k1", "k2", "k3", "p1", "p2"]),
            sensor: held(&["tilt_x", "tilt_y"]),
        },
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
            "The camera, its sensor tilt and its laser plane from the device workflow, seeded from the datasheet tilt; then the hand-eye with that camera held, so the plane and the hand-eye share its frame.",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procedure::named;

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
                // A Windows checkout may have CRLF line endings.
                let on_disk = std::fs::read_to_string(&path)
                    .unwrap_or_default()
                    .replace("\r\n", "\n");
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
