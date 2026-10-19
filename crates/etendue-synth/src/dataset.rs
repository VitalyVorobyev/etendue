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
//! eye-in-hand; target on a robot → eye-to-hand. Lasers on a hand-eye rig
//! make it the joint rig hand-eye laserline problem (P5-3): each camera then
//! also lists laser images (`images/<camera>/<capture>_laser.png`), and
//! `gt.json` carries the true laser planes and the stripes' points
//! ([`crate::laser`]). Either one laser that every camera images, or one
//! laser per camera (P7-3, `LaserSpec::camera`): each camera then images,
//! and calibrates, its own laser only. Other laser topologies are not
//! emitted yet.

use std::collections::BTreeMap;

use etendue_scene::{
    BakedScenario, LaserSpec, ParsedFrameRef, RobotManifest, SceneSpec, TargetGeometry,
};
use nalgebra::Isometry3;
use serde::{Deserialize, Serialize};
use serde_json::json;
use vision_calibration_core::{CameraParams, SensorParams};
use vision_calibration_dataset::{DatasetSpec, Topology};

use crate::gt::{BoardPoint, GtPoint, VisibilitySpec, project_points};
use crate::laser::{GtPlane, StripeSurface, beam_widths, plane_in, stripe};
use crate::{Error, PixelCentre, Result};

/// The `gt.json` format version. Version 2 adds the laser fields; version 3
/// the target's geometry and 3-D points, laser images and stripe widths.
pub const GT_VERSION: u32 = 3;

/// Everything a synthetic dataset needs besides the renders.
#[derive(Clone, Debug)]
pub struct Bundle {
    /// `dataset.json`, when calibration-rs takes the target (a chessboard,
    /// a ChArUco board in OpenCV's layout, a ring grid, a `board`
    /// puzzleboard); `None` for a target it does not calibrate from yet (a
    /// marker board, a printed PuzzleBoard, the PuzzlePole), whose ground
    /// truth is still written.
    pub dataset: Option<DatasetSpec>,
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
    /// True laser planes, for laser topologies.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lasers: Vec<GtLaser>,
    /// One entry per capture, in time order (= dataset image order).
    pub captures: Vec<GtCapture>,
}

/// A laser's true plane.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GtLaser {
    /// Laser id in the scene.
    pub id: String,
    /// The camera that images it, with one laser per camera; absent when
    /// every camera images it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera: Option<String>,
    /// The plane in etendue's rig frame (the scene's rig entity). That is
    /// **not** calibration-rs's rig frame, which is the first camera's: compare
    /// a solved rig-frame plane with `camera_planes[0]` instead.
    pub rig_plane: GtPlane,
    /// The plane in each camera's frame, in dataset camera order.
    pub camera_planes: Vec<GtPlane>,
}

/// A laser's stripe on the target at one capture.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GtStripe {
    /// Laser id.
    pub laser: String,
    /// Stripe points in the target frame; [`GtPoint::point`] of a
    /// [`GtStripeView`] indexes this. Empty if the laser misses the target.
    pub points: Vec<BoardPoint>,
    /// The beam's `1/e²` diameter at each point, metres (the laser's Gaussian
    /// beam, waist at its origin).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub width_m: Vec<f64>,
}

/// What one camera sees of one laser's stripe.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GtStripeView {
    /// Laser id.
    pub laser: String,
    /// Every stripe point, projected, with visibility.
    pub points: Vec<GtPoint>,
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
    /// For a camera with physical optics (ADR 0008): its projection frame (the entrance pupil,
    /// the frame of `params` and of every camera pose here) in its body's frame, where the scene
    /// places it. A calibration recovers the projection frame; this gives the body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mech_se3_projection: Option<Isometry3<f64>>,
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
    /// What the target is (the scene's geometry), so the dataset's tools
    /// know which detector to run even when there is no `dataset.json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometry: Option<TargetGeometry>,
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
    /// Each laser's stripe on the target, in [`GroundTruth::lasers`] order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub laser_stripes: Vec<GtStripe>,
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
    /// Laser image path relative to the dataset directory, for laser
    /// topologies: what `etendue render` writes for this camera (P7-7). The
    /// analytic loop reads the stripe pixels from `laser_points` instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub laser_image: Option<String>,
    /// The stripe of each laser this camera images, projected, in
    /// [`GtCapture::laser_stripes`] order (a subsequence: match by laser id).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub laser_points: Vec<GtStripeView>,
}

/// Options of [`emit`].
#[derive(Clone, Copy, Debug)]
pub struct EmitOptions {
    /// Visibility tests.
    pub visibility: VisibilitySpec,
    /// The pixel-centre convention of the emitted pixels.
    pub pixel_centre: PixelCentre,
    /// Spacing of the laser-stripe points on the target, in metres.
    pub laser_spacing_m: f64,
}

/// Where a frame is ultimately attached.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Anchor {
    World,
    Robot { robot: String },
}

fn anchor(scene: &SceneSpec, id: &str) -> Result<Anchor> {
    let (_, end) = ancestry(scene, id)?;
    Ok(match end {
        ParsedFrameRef::RobotLink { robot, .. } => Anchor::Robot {
            robot: robot.to_owned(),
        },
        _ => Anchor::World,
    })
}

/// Whether `id` hangs (through entity parents) below the entity `ancestor`.
fn mounted_on(scene: &SceneSpec, id: &str, ancestor: &str) -> bool {
    ancestry(scene, id).is_ok_and(|(chain, _)| chain.contains(&ancestor))
}

/// The entities above `id`, nearest first, and the frame the chain ends on
/// (the world or a robot link).
///
/// # Errors
///
/// If an entity on the way is missing, a parent does not parse, or the
/// parents form a cycle.
fn ancestry<'s>(scene: &'s SceneSpec, id: &'s str) -> Result<(Vec<&'s str>, ParsedFrameRef<'s>)> {
    let parents: BTreeMap<&str, &etendue_scene::FrameRef> =
        scene.mounted().map(|m| (m.id, m.parent)).collect();
    let mut chain = Vec::new();
    let mut at = id;
    for _ in 0..=parents.len() {
        let parent = parents
            .get(at)
            .ok_or_else(|| Error::InvalidInput(format!("no scene entity `{at}`")))?;
        match parent.parse().map_err(Error::InvalidInput)? {
            ParsedFrameRef::Entity(e) => {
                chain.push(e);
                at = e;
            }
            end => return Ok((chain, end)),
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

/// The calibration-rs dataset target of a scene target, if calibration-rs
/// takes it: a `board` as written; a printed chessboard; a printed ChArUco
/// board in OpenCV's layout with one-bit borders and no insets (what the
/// calibration-rs detector assumes).
#[must_use]
pub fn dataset_target(geometry: &TargetGeometry) -> Option<vision_calibration_dataset::TargetSpec> {
    use etendue_scene::{CharucoLayout, PrintedTarget};
    use vision_calibration_dataset::TargetSpec as Board;
    match geometry {
        TargetGeometry::Board { board } => Some(board.clone()),
        TargetGeometry::Printed {
            target:
                PrintedTarget::Chessboard {
                    inner_rows,
                    inner_cols,
                    square_size_mm,
                    inner_square_rel: None,
                },
        } => Some(Board::Chessboard {
            rows: *inner_rows,
            cols: *inner_cols,
            square_size_m: square_size_mm / 1000.0,
        }),
        TargetGeometry::Printed {
            target:
                PrintedTarget::Charuco {
                    rows,
                    cols,
                    square_size_mm,
                    marker_size_rel,
                    dictionary,
                    marker_layout: CharucoLayout::OpencvCharuco,
                    border_bits: 1,
                    inner_square_rel: None,
                },
        } => Some(Board::Charuco {
            rows: *rows,
            cols: *cols,
            square_size_m: square_size_mm / 1000.0,
            marker_size_m: marker_size_rel * square_size_mm / 1000.0,
            dictionary: dictionary.clone(),
        }),
        _ => None,
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
/// not exactly one board target, cameras and target both on robots, a laser
/// anywhere but on a hand-eye rig of two or more cameras (or more than one
/// laser), a scenario without captures, or a frame missing from `baked`.
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
        .filter(|t| !matches!(t.geometry, TargetGeometry::Rectangle { .. }))
        .collect();
    let [target] = boards.as_slice() else {
        return bad(format!(
            "a dataset needs exactly one board target, the scene has {}",
            boards.len()
        ));
    };
    let board = dataset_target(&target.geometry);
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
    // Lasers on the hand-eye rig: the joint rig hand-eye laserline problem,
    // with one laser that every camera images, or one per camera.
    // A single camera is a rig of one, its laser on the camera's body (a triangulation head).
    let lasers_on_rig: Vec<&LaserSpec> = scene.lasers.iter().collect();
    for laser in &lasers_on_rig {
        if robot.is_none() || !mounted_on(scene, &laser.id, &mount) {
            return bad(format!(
                "laser `{}`: the only laser topology emitted so far is lasers on a hand-eye rig, \
                 or on a single hand-eye camera (rig hand-eye laserline)",
                laser.id
            ));
        }
    }
    let per_camera = match lasers_on_rig.as_slice() {
        [] | [LaserSpec { camera: None, .. }] => false,
        all if all.iter().all(|l| l.camera.is_some()) && all.len() == n => true,
        all => {
            return bad(format!(
                "{} lasers: a dataset takes one laser that every camera images, or one laser per \
                 camera (each naming its `camera`)",
                all.len()
            ));
        }
    };
    let extent = if lasers_on_rig.is_empty() {
        None
    } else {
        Some(StripeSurface::of(&target.geometry).ok_or_else(|| {
            Error::InvalidInput(format!("target `{}` has no known extent", target.id))
        })?)
    };
    // The lasers camera `c` images.
    let imaged_by = |c: &str| -> Vec<usize> {
        (0..lasers_on_rig.len())
            .filter(|&i| !per_camera || lasers_on_rig[i].camera.as_deref() == Some(c))
            .collect()
    };
    let topology = match (&robot, n) {
        // One camera with its laser: the device (intrinsics and plane); the robot's poses ride
        // along for a hand-eye solve on the same views.
        (_, 1) if extent.is_some() => Topology::LaserlineDevice,
        _ if extent.is_some() => Topology::RigHandeyeLaserline,
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
                mech_se3_projection: None,
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

    // The lasers are on the rig, so their planes are fixed in the rig and
    // camera frames: take them at the first capture.
    let lasers = match &rig {
        Some(r) => lasers_on_rig
            .iter()
            .map(|l| {
                let world_se3_laser = poses.at(&l.id, first)?;
                let camera_planes = scene
                    .cameras
                    .iter()
                    .map(|c| {
                        Ok(plane_in(
                            &c.id,
                            &(poses.at(&c.id, first)?.inverse() * world_se3_laser),
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(GtLaser {
                    id: l.id.clone(),
                    camera: if per_camera { l.camera.clone() } else { None },
                    rig_plane: plane_in(r, &(poses.at(r, first)?.inverse() * world_se3_laser)),
                    camera_planes,
                })
            })
            .collect::<Result<Vec<_>>>()?,
        None => Vec::new(),
    };

    let models = scene
        .cameras
        .iter()
        .map(|c| c.params.build())
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let image = |camera: &str, capture: &str| format!("images/{camera}/{capture}.png");
    let laser_image = |camera: &str, capture: &str| format!("images/{camera}/{capture}_laser.png");
    let mut gt_captures = Vec::with_capacity(captures.len());
    for &k in &captures {
        let sample = &baked.samples[k];
        let id = sample
            .capture
            .as_ref()
            .map(|c| c.id.clone())
            .unwrap_or_default();
        let world_se3_target = poses.at(&target.id, k)?;
        let laser_stripes = match extent {
            Some(extent) => lasers_on_rig
                .iter()
                .map(|l| {
                    let world_se3_laser = poses.at(&l.id, k)?;
                    let points = stripe(
                        l,
                        &world_se3_laser,
                        &world_se3_target,
                        extent,
                        options.laser_spacing_m,
                    )?;
                    Ok(GtStripe {
                        laser: l.id.clone(),
                        width_m: beam_widths(l, &world_se3_laser, &world_se3_target, &points)?,
                        points,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
            None => Vec::new(),
        };
        let views = scene
            .cameras
            .iter()
            .zip(&models)
            .map(|(c, model)| {
                let world_se3_camera = poses.at(&c.id, k)?;
                let project = |points: &[BoardPoint]| {
                    project_points(
                        model,
                        c.resolution,
                        &world_se3_camera,
                        &world_se3_target,
                        points,
                        &options.visibility,
                        options.pixel_centre.edge(),
                    )
                };
                Ok(GtView {
                    camera: c.id.clone(),
                    image: image(&c.id, &id),
                    world_se3_camera,
                    points: project(points),
                    laser_image: extent.is_some().then(|| laser_image(&c.id, &id)),
                    laser_points: imaged_by(&c.id)
                        .into_iter()
                        .map(|i| GtStripeView {
                            laser: laser_stripes[i].laser.clone(),
                            points: project(&laser_stripes[i].points),
                        })
                        .collect(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        gt_captures.push(GtCapture {
            id,
            sample: k,
            t: sample.t,
            base_se3_tcp: base_se3_tcp(k)?,
            world_se3_target,
            laser_stripes,
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

    let cameras_json = scene
        .cameras
        .iter()
        .map(|c| {
            let list = |path: &dyn Fn(&str, &str) -> String| {
                json!({
                    "kind": "list",
                    "paths": gt_captures.iter().map(|g| path(&c.id, &g.id)).collect::<Vec<_>>(),
                })
            };
            let mut camera = json!({ "id": c.id, "images": list(&image) });
            if extent.is_some() {
                camera["laser_images"] = list(&laser_image);
            }
            camera
        })
        .collect::<Vec<_>>();
    let mut dataset = json!({
        "version": 1,
        "cameras": cameras_json,
        "target": board,
        "topology": topology,
    });
    let with_dataset = board.is_some();
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
    let dataset: Option<DatasetSpec> = if with_dataset {
        Some(
            serde_json::from_value(dataset)
                .map_err(|e| Error::InvalidInput(format!("building dataset.json: {e}")))?,
        )
    } else {
        None
    };

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
                geometry: Some(target.geometry.clone()),
                points: points.to_vec(),
            },
            lasers,
            captures: gt_captures,
        },
    })
}
