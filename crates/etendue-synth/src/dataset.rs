//! Synthetic dataset emission ([ADR 0006](https://github.com/VitalyVorobyev/etendue/blob/main/docs/adrs/0006-ground-truth.md)).
//!
//! From a scene, its baked scenario and the target's feature points, build:
//!
//! - `dataset.json` — a calibration-rs [`DatasetSpec`] that
//!   `vision_calibration_dataset::validate` accepts: one image list per
//!   camera (`images/<camera>/<capture>.png`, written by a render backend),
//!   the board as the dataset `TargetSpec`, the topology the frame tree
//!   implies, and for hand-eye scenes the robot poses with an explicit
//!   `pose_convention` and `pose_pairing: by_index`;
//! - `robot_poses.json` — `base_se3_<tcp_link>` per capture;
//! - `gt.json` — [`GroundTruth`]: true camera models and mounts, hand-eye,
//!   and every capture's projected target points with visibility.
//!
//! The topology comes from where the cameras and the target hang in the frame
//! tree: both fixed → intrinsics / rig extrinsics; cameras on a robot →
//! eye-in-hand; target on a robot → eye-to-hand. Lasers are not part of the
//! geometric dataset yet (laser topologies are P5).

use std::collections::BTreeMap;

use etendue_scene::{BakedScenario, ParsedFrameRef, RobotManifest, SceneSpec, TargetGeometry};
use nalgebra::Isometry3;
use serde::{Deserialize, Serialize};
use serde_json::json;
use vision_calibration_core::{CameraParams, SensorParams};
use vision_calibration_dataset::{DatasetSpec, Topology};

use crate::gt::{BoardPoint, GtPoint, VisibilitySpec, project_points};
use crate::{Error, PixelCentre, Result};

/// The `gt.json` format version.
pub const GT_VERSION: u32 = 1;

/// Everything a synthetic dataset needs besides the renders.
#[derive(Clone, Debug)]
pub struct Bundle {
    /// `dataset.json`.
    pub dataset: DatasetSpec,
    /// `robot_poses.json` (hand-eye topologies only).
    pub robot_poses: Option<serde_json::Value>,
    /// `gt.json`.
    pub gt: GroundTruth,
}

/// The ground truth of a synthetic dataset (`gt.json`, etendue-owned).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroundTruth {
    /// Format version, [`GT_VERSION`].
    pub version: u32,
    /// The calibration problem the dataset poses.
    pub topology: Topology,
    /// The pixel-centre convention pixels are given in.
    pub pixel_centre: PixelCentre,
    /// True camera models, in dataset camera order.
    pub cameras: Vec<GtCamera>,
    /// The rig the cameras are mounted on, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rig: Option<String>,
    /// True hand-eye, for hand-eye topologies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handeye: Option<GtHandeye>,
    /// The target and its feature points.
    pub target: GtTarget,
    /// One entry per capture, in time order (= dataset image order).
    pub captures: Vec<GtCapture>,
}

/// A camera's true parameters and mount.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GtCamera {
    /// Camera id (= dataset camera id).
    pub id: String,
    /// The calibration-rs model.
    pub params: CameraParams,
    /// Image size `[width, height]`.
    pub resolution: [u32; 2],
    /// Pose of the rig in the camera frame, if the camera is on a rig.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cam_se3_rig: Option<Isometry3<f64>>,
}

/// True hand-eye transform, in calibration-rs `HandeyeMountSpec` terms.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum GtHandeye {
    /// Cameras on the robot: the rig (or camera) in the gripper frame, and the
    /// fixed target in the robot base frame.
    EyeInHand {
        /// Robot id.
        robot: String,
        /// The link robot poses refer to (`base_se3_<tcp_link>`).
        tcp_link: String,
        /// `gripper_se3_rig`.
        gripper_se3_rig: Isometry3<f64>,
        /// `base_se3_target`.
        base_se3_target: Isometry3<f64>,
    },
    /// Target on the robot: the fixed rig and the target in the gripper frame.
    EyeToHand {
        /// Robot id.
        robot: String,
        /// The link robot poses refer to (`base_se3_<tcp_link>`).
        tcp_link: String,
        /// `rig_se3_base`.
        rig_se3_base: Isometry3<f64>,
        /// `gripper_se3_target`.
        gripper_se3_target: Isometry3<f64>,
    },
}

/// The target of the dataset.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GtTarget {
    /// Target id in the scene.
    pub id: String,
    /// Feature points in the target frame; [`GtPoint::point`] indexes this.
    pub points: Vec<BoardPoint>,
}

/// One capture.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GtCapture {
    /// Capture id.
    pub id: String,
    /// Baked sample index.
    pub sample: usize,
    /// Time in seconds.
    pub t: f64,
    /// Robot pose `base_se3_<tcp_link>` (hand-eye topologies).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_se3_tcp: Option<Isometry3<f64>>,
    /// Target pose in the world.
    pub world_se3_target: Isometry3<f64>,
    /// One view per camera, in dataset camera order.
    pub views: Vec<GtView>,
}

/// What one camera sees at one capture.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GtView {
    /// Camera id.
    pub camera: String,
    /// Image path relative to the dataset directory.
    pub image: String,
    /// Camera pose in the world.
    pub world_se3_camera: Isometry3<f64>,
    /// Every target point, projected, with visibility.
    pub points: Vec<GtPoint>,
}

/// Options of [`emit`].
#[derive(Clone, Copy, Debug)]
pub struct EmitOptions {
    /// Visibility tests.
    pub visibility: VisibilitySpec,
    /// The pixel-centre convention of the emitted pixels.
    pub pixel_centre: PixelCentre,
}

/// Where a frame is ultimately attached.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Anchor {
    World,
    Robot { robot: String },
}

fn anchor(scene: &SceneSpec, id: &str) -> Result<Anchor> {
    let parents: BTreeMap<&str, &etendue_scene::FrameRef> =
        scene.mounted().map(|m| (m.id, m.parent)).collect();
    let mut at = id;
    for _ in 0..=parents.len() {
        let parent = parents
            .get(at)
            .ok_or_else(|| Error::InvalidInput(format!("no scene entity `{at}`")))?;
        match parent.parse().map_err(Error::InvalidInput)? {
            ParsedFrameRef::World => return Ok(Anchor::World),
            ParsedFrameRef::RobotLink { robot, .. } => {
                return Ok(Anchor::Robot {
                    robot: robot.to_owned(),
                });
            }
            ParsedFrameRef::Entity(e) => at = e,
        }
    }
    Err(Error::InvalidInput(format!("parent cycle above `{id}`")))
}

struct Poses<'a> {
    baked: &'a BakedScenario,
    index: BTreeMap<&'a str, usize>,
}

impl<'a> Poses<'a> {
    fn new(baked: &'a BakedScenario) -> Self {
        let index = baked
            .frames
            .iter()
            .enumerate()
            .map(|(i, f)| (f.as_str(), i))
            .collect();
        Self { baked, index }
    }

    fn at(&self, frame: &str, k: usize) -> Result<Isometry3<f64>> {
        let i = *self.index.get(frame).ok_or_else(|| {
            Error::InvalidInput(format!("frame `{frame}` is not in the baked scenario"))
        })?;
        Ok(self.baked.samples[k].world_se3_frame[i])
    }
}

/// Build the dataset bundle.
///
/// `manifests[i]` is the manifest of `scene.robots[i]`; `points` are the
/// feature points of the scene's (single) board target, in the target frame.
///
/// # Errors
///
/// [`Error::InvalidInput`] if the scene does not describe one calibration
/// problem calibration-rs can take: no camera, cameras not sharing one rig,
/// not exactly one board target, cameras and target both on robots, a
/// scenario without captures, or a frame missing from `baked`.
pub fn emit(
    scene: &SceneSpec,
    baked: &BakedScenario,
    manifests: &[RobotManifest],
    points: &[BoardPoint],
    options: &EmitOptions,
) -> Result<Bundle> {
    let bad = |m: String| Err(Error::InvalidInput(m));
    if scene.cameras.is_empty() {
        return bad("the scene has no camera".into());
    }
    if manifests.len() != scene.robots.len() {
        return bad(format!(
            "{} manifests for {} scene robots",
            manifests.len(),
            scene.robots.len()
        ));
    }
    // The rig: every camera's parent, if they share one.
    let rig = {
        let parents: Vec<Option<&str>> = scene
            .cameras
            .iter()
            .map(|c| match c.parent.parse() {
                Ok(ParsedFrameRef::Entity(e)) if scene.rigs.iter().any(|r| r.id == e) => Some(e),
                _ => None,
            })
            .collect();
        match parents.first().copied().flatten() {
            Some(r) if parents.iter().all(|p| *p == Some(r)) => Some(r.to_owned()),
            _ if scene.cameras.len() == 1 => None,
            _ => return bad("cameras must all be mounted on one rig".into()),
        }
    };
    let boards: Vec<_> = scene
        .targets
        .iter()
        .filter(|t| matches!(t.geometry, TargetGeometry::Board { .. }))
        .collect();
    let [target] = boards.as_slice() else {
        return bad(format!(
            "a dataset needs exactly one board target, the scene has {}",
            boards.len()
        ));
    };
    let TargetGeometry::Board { board } = &target.geometry else {
        unreachable!("filtered to boards")
    };
    let captures: Vec<usize> = baked.capture_indices().collect();
    if captures.is_empty() {
        return bad("the scenario has no capture".into());
    }

    let mount = rig.clone().unwrap_or_else(|| scene.cameras[0].id.clone());
    let robot = match (anchor(scene, &mount)?, anchor(scene, &target.id)?) {
        (Anchor::World, Anchor::World) => None,
        (Anchor::Robot { robot }, Anchor::World) => Some((robot, true)),
        (Anchor::World, Anchor::Robot { robot }) => Some((robot, false)),
        (Anchor::Robot { .. }, Anchor::Robot { .. }) => {
            return bad("cameras and target both on robots is not a supported topology".into());
        }
    };
    let n = scene.cameras.len();
    let topology = match (&robot, n) {
        (None, 1) => match scene.cameras[0].params.sensor {
            SensorParams::Scheimpflug { .. } => Topology::ScheimpflugIntrinsics,
            _ => Topology::PlanarIntrinsics,
        },
        (None, _) => Topology::RigExtrinsics,
        (Some(_), 1) => Topology::SingleCamHandeye,
        (Some(_), _) => Topology::RigHandeye,
    };

    let poses = Poses::new(baked);
    let first = captures[0];
    let robot_links = match &robot {
        Some((id, _)) => {
            let i = scene
                .robots
                .iter()
                .position(|r| &r.id == id)
                .ok_or_else(|| Error::InvalidInput(format!("no robot `{id}`")))?;
            let m = &manifests[i];
            Some((
                format!("{id}/{}", m.base_link),
                format!("{id}/{}", m.tcp_link),
                m.tcp_link.clone(),
            ))
        }
        None => None,
    };
    let base_se3_tcp = |k: usize| -> Result<Option<Isometry3<f64>>> {
        match &robot_links {
            Some((base, tcp, _)) => Ok(Some(poses.at(base, k)?.inverse() * poses.at(tcp, k)?)),
            None => Ok(None),
        }
    };

    let cameras = scene
        .cameras
        .iter()
        .map(|c| {
            let cam_se3_rig = match &rig {
                Some(r) => Some(poses.at(&c.id, first)?.inverse() * poses.at(r, first)?),
                None => None,
            };
            Ok(GtCamera {
                id: c.id.clone(),
                params: c.params.clone(),
                resolution: c.resolution,
                cam_se3_rig,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let handeye = match (&robot, &robot_links) {
        (Some((id, eye_in_hand)), Some((base, tcp, tcp_link))) => {
            let world_se3_mount = poses.at(&mount, first)?;
            let world_se3_target = poses.at(&target.id, first)?;
            let world_se3_base = poses.at(base, first)?;
            let world_se3_tcp = poses.at(tcp, first)?;
            Some(if *eye_in_hand {
                GtHandeye::EyeInHand {
                    robot: id.clone(),
                    tcp_link: tcp_link.clone(),
                    gripper_se3_rig: world_se3_tcp.inverse() * world_se3_mount,
                    base_se3_target: world_se3_base.inverse() * world_se3_target,
                }
            } else {
                GtHandeye::EyeToHand {
                    robot: id.clone(),
                    tcp_link: tcp_link.clone(),
                    rig_se3_base: world_se3_mount.inverse() * world_se3_base,
                    gripper_se3_target: world_se3_tcp.inverse() * world_se3_target,
                }
            })
        }
        _ => None,
    };

    let models = scene
        .cameras
        .iter()
        .map(|c| c.params.build())
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let image = |camera: &str, capture: &str| format!("images/{camera}/{capture}.png");
    let mut gt_captures = Vec::with_capacity(captures.len());
    for &k in &captures {
        let sample = &baked.samples[k];
        let id = sample
            .capture
            .as_ref()
            .map(|c| c.id.clone())
            .unwrap_or_default();
        let world_se3_target = poses.at(&target.id, k)?;
        let views = scene
            .cameras
            .iter()
            .zip(&models)
            .map(|(c, model)| {
                let world_se3_camera = poses.at(&c.id, k)?;
                Ok(GtView {
                    camera: c.id.clone(),
                    image: image(&c.id, &id),
                    world_se3_camera,
                    points: project_points(
                        model,
                        c.resolution,
                        &world_se3_camera,
                        &world_se3_target,
                        points,
                        &options.visibility,
                        options.pixel_centre.edge(),
                    ),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        gt_captures.push(GtCapture {
            id,
            sample: k,
            t: sample.t,
            base_se3_tcp: base_se3_tcp(k)?,
            world_se3_target,
            views,
        });
    }

    let robot_poses = if robot.is_some() {
        Some(serde_json::Value::Array(
            gt_captures
                .iter()
                .map(|c| {
                    let p = c.base_se3_tcp.expect("hand-eye captures carry robot poses");
                    let q = p.rotation.quaternion().coords;
                    let t = p.translation.vector;
                    json!({
                        "capture": c.id,
                        "tx": t.x, "ty": t.y, "tz": t.z,
                        "qx": q.x, "qy": q.y, "qz": q.z, "qw": q.w,
                    })
                })
                .collect(),
        ))
    } else {
        None
    };

    let mut dataset = json!({
        "version": 1,
        "cameras": scene.cameras.iter().map(|c| json!({
            "id": c.id,
            "images": {
                "kind": "list",
                "paths": gt_captures.iter().map(|g| image(&c.id, &g.id)).collect::<Vec<_>>(),
            },
        })).collect::<Vec<_>>(),
        "target": board,
        "topology": topology,
    });
    if robot.is_some() {
        let obj = dataset.as_object_mut().expect("an object");
        obj.insert(
            "robot_poses".into(),
            json!({
                "path": "robot_poses.json",
                "format": "json",
                "columns": { "tx": "tx", "ty": "ty", "tz": "tz", "rotation": ["qx", "qy", "qz", "qw"] },
            }),
        );
        obj.insert(
            "pose_convention".into(),
            json!({ "transform": "t_base_tcp", "rotation_format": "quat_xyzw", "translation_units": "m" }),
        );
        obj.insert("pose_pairing".into(), json!({ "kind": "by_index" }));
    }
    let dataset: DatasetSpec = serde_json::from_value(dataset)
        .map_err(|e| Error::InvalidInput(format!("building dataset.json: {e}")))?;

    Ok(Bundle {
        dataset,
        robot_poses,
        gt: GroundTruth {
            version: GT_VERSION,
            topology,
            pixel_centre: options.pixel_centre,
            cameras,
            rig,
            handeye,
            target: GtTarget {
                id: target.id.clone(),
                points: points.to_vec(),
            },
            captures: gt_captures,
        },
    })
}
