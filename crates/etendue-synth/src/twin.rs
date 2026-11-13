//! The twin of a real calibration (P20b): a [`GroundTruth`] built directly from a calibration's
//! result, its robot poses and the points its detector found. No scene and no robot model: a
//! real cell's robot may not be in the library.
//!
//! - **Truth:** the calibrated cameras, their `cam_se3_rig` and the hand-eye.
//! - **Target:** the points calibration-rs laid the target out with, as the views name them.
//! - **Captures:** one per calibrated view, in its order, with the robot pose the calibration
//!   took (`base_se3_tcp`, exact).
//! - **Views:** exactly the points the detector found, in its order, projected through the truth
//!   with calibration-rs's camera model. A point the detector found is seen, so no visibility
//!   test applies.

use std::collections::BTreeMap;

use nalgebra::{Isometry3, Point3};
use vision_calibration_core::CameraParams;
use vision_calibration_dataset::Topology;

use crate::dataset::{GT_VERSION, GroundTruth, GtCamera, GtCapture, GtHandeye, GtTarget, GtView};
use crate::gt::{BoardPoint, GtPoint};
use crate::{Error, PixelCentre, Result};

/// A calibrated camera of the real cell.
#[derive(Clone, Debug)]
pub struct TwinCamera {
    /// The camera's id (the manifest's).
    pub id: String,
    /// The calibrated model.
    pub params: CameraParams,
    /// Image size `[width, height]`.
    pub resolution: [u32; 2],
    /// The rig in the camera's frame (identity for the reference camera, and for a camera
    /// without a rig).
    pub cam_se3_rig: Isometry3<f64>,
}

/// One calibrated view of the real cell.
#[derive(Clone, Debug)]
pub struct TwinView {
    /// The robot pose the calibration took, `base_se3_tcp`.
    pub base_se3_tcp: Isometry3<f64>,
    /// Per camera, in camera order: the target points (target frame, metres) the detector found,
    /// in its order; `None` where the camera has no detection in this view.
    pub points: Vec<Option<Vec<[f64; 3]>>>,
}

/// The ground truth of the twin of a real hand-eye calibration: `cameras` and `handeye` (in
/// calibration-rs's frames: the rig is the reference camera's) as the truth, `views` as the
/// captures. `topology` is the real manifest's.
///
/// # Errors
///
/// No camera, a view whose camera count differs from `cameras`', a camera model that does not
/// build, or a found point that does not project (behind its camera).
pub fn twin(
    topology: Topology,
    cameras: &[TwinCamera],
    handeye: GtHandeye,
    views: &[TwinView],
) -> Result<GroundTruth> {
    if cameras.is_empty() {
        return Err(Error::InvalidInput("a twin needs a camera".into()));
    }
    if let Some(k) = views.iter().position(|v| v.points.len() != cameras.len()) {
        return Err(Error::InvalidInput(format!(
            "view {k} has {} cameras, the calibration {}",
            views[k].points.len(),
            cameras.len()
        )));
    }
    let models = cameras
        .iter()
        .map(|c| c.params.build())
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let (points, index) = target_points(views);
    let mut captures = Vec::with_capacity(views.len());
    for (k, view) in views.iter().enumerate() {
        let (world_se3_rig, world_se3_target) = match &handeye {
            GtHandeye::EyeInHand {
                gripper_se3_rig,
                base_se3_target,
                ..
            } => (view.base_se3_tcp * gripper_se3_rig, *base_se3_target),
            GtHandeye::EyeToHand {
                rig_se3_base,
                gripper_se3_target,
                ..
            } => (
                rig_se3_base.inverse(),
                view.base_se3_tcp * gripper_se3_target,
            ),
        };
        let mut gt_views = Vec::with_capacity(cameras.len());
        for ((camera, model), found) in cameras.iter().zip(&models).zip(&view.points) {
            let world_se3_camera = world_se3_rig * camera.cam_se3_rig.inverse();
            let camera_se3_target = world_se3_camera.inverse() * world_se3_target;
            let mut seen = Vec::new();
            for xyz in found.iter().flatten() {
                let p = camera_se3_target * Point3::from(*xyz);
                let pixel = (p.z > 0.0)
                    .then(|| model.project_point_c(&p.coords))
                    .flatten()
                    .ok_or_else(|| {
                        Error::InvalidInput(format!(
                            "view {k}, camera {}: a found point is behind the camera",
                            camera.id
                        ))
                    })?;
                seen.push(GtPoint {
                    point: index[&key(xyz)],
                    pixel: Some([pixel.x, pixel.y]),
                    occluded: None,
                });
            }
            gt_views.push(GtView {
                camera: camera.id.clone(),
                image: String::new(),
                world_se3_camera,
                points: seen,
                laser_image: None,
                laser_points: Vec::new(),
            });
        }
        captures.push(GtCapture {
            id: k.to_string(),
            sample: k,
            t: 0.0,
            base_se3_tcp: Some(view.base_se3_tcp),
            world_se3_target,
            laser_stripes: Vec::new(),
            views: gt_views,
        });
    }
    let rig = cameras.len() > 1;
    Ok(GroundTruth {
        version: GT_VERSION,
        topology,
        pixel_centre: PixelCentre::Integer,
        cameras: cameras
            .iter()
            .map(|c| GtCamera {
                id: c.id.clone(),
                params: c.params.clone(),
                resolution: c.resolution,
                cam_se3_rig: rig.then_some(c.cam_se3_rig),
                mech_se3_projection: None,
            })
            .collect(),
        rig: rig.then(|| "rig".to_owned()),
        handeye: Some(handeye),
        target: GtTarget {
            id: "target".into(),
            geometry: None,
            points,
        },
        lasers: Vec::new(),
        captures,
    })
}

/// A point's key: its coordinates' bits, so equal points (one layout, read once per view) are one.
fn key(xyz: &[f64; 3]) -> [u64; 3] {
    xyz.map(f64::to_bits)
}

/// The distinct points of `views`, ordered by row (`y`), then `x`, then `z`, and each one's index.
fn target_points(views: &[TwinView]) -> (Vec<BoardPoint>, BTreeMap<[u64; 3], usize>) {
    let mut distinct: Vec<[f64; 3]> = Vec::new();
    let mut seen = BTreeMap::new();
    for xyz in views
        .iter()
        .flat_map(|v| v.points.iter().flatten().flatten())
    {
        if seen.insert(key(xyz), 0).is_none() {
            distinct.push(*xyz);
        }
    }
    distinct.sort_by(|a, b| {
        a[1].total_cmp(&b[1])
            .then(a[0].total_cmp(&b[0]))
            .then(a[2].total_cmp(&b[2]))
    });
    let index = distinct
        .iter()
        .enumerate()
        .map(|(i, p)| (key(p), i))
        .collect();
    let points = distinct
        .into_iter()
        .map(|[x, y, z]| {
            if z == 0.0 {
                BoardPoint::planar([x, y], None, None)
            } else {
                BoardPoint {
                    position_m: [x, y],
                    xyz_m: Some([x, y, z]),
                    normal: None,
                    grid: None,
                    id: None,
                }
            }
        })
        .collect();
    (points, index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observe::{ObservationNoise, observe};
    use nalgebra::{Translation3, UnitQuaternion, Vector3};
    use vision_calibration_core::{
        BrownConrady5, DistortionParams, FxFyCxCy, ProjectionParams, SensorParams,
    };

    fn camera(id: &str, x: f64) -> TwinCamera {
        TwinCamera {
            id: id.into(),
            params: CameraParams {
                projection: ProjectionParams::Pinhole,
                distortion: DistortionParams::BrownConrady5 {
                    params: BrownConrady5 {
                        k1: -0.1,
                        k2: 0.02,
                        k3: 0.0,
                        p1: 1e-4,
                        p2: -2e-4,
                        iters: 8,
                    },
                },
                sensor: SensorParams::Identity,
                intrinsics: FxFyCxCy {
                    fx: 1000.0,
                    fy: 1001.0,
                    cx: 640.0,
                    cy: 480.0,
                },
            },
            resolution: [1280, 960],
            cam_se3_rig: Isometry3::translation(x, 0.0, 0.0),
        }
    }

    fn handeye() -> GtHandeye {
        GtHandeye::EyeInHand {
            robot: "real".into(),
            tcp_link: "tcp".into(),
            gripper_se3_rig: Isometry3::from_parts(
                Translation3::new(0.0, 0.05, 0.1),
                UnitQuaternion::from_scaled_axis(Vector3::new(0.0, 0.0, 0.3)),
            ),
            base_se3_target: Isometry3::from_parts(
                Translation3::new(0.0, 0.0, 0.0),
                UnitQuaternion::from_scaled_axis(Vector3::new(std::f64::consts::PI, 0.0, 0.0)),
            ),
        }
    }

    /// The gripper 0.6 m above the target, looking down.
    fn pose(dx: f64) -> Isometry3<f64> {
        Isometry3::translation(dx, 0.02, -0.7)
    }

    fn grid(n: usize) -> Vec<[f64; 3]> {
        (0..n * n)
            .map(|i| [0.02 * (i % n) as f64, 0.02 * (i / n) as f64, 0.0])
            .collect()
    }

    #[test]
    fn found_points_are_projected_through_the_truth_and_indexed_once() {
        let cams = [camera("a", 0.0), camera("b", -0.1)];
        let mut second = grid(4);
        second.reverse();
        let views = [
            TwinView {
                base_se3_tcp: pose(0.0),
                points: vec![Some(grid(4)), None],
            },
            TwinView {
                base_se3_tcp: pose(0.05),
                points: vec![Some(second.clone()), Some(second[..6].to_vec())],
            },
        ];
        let gt = twin(Topology::RigHandeye, &cams, handeye(), &views).unwrap();
        assert_eq!(gt.target.points.len(), 16);
        assert_eq!(gt.cameras[1].cam_se3_rig, Some(cams[1].cam_se3_rig));
        assert!(gt.captures[0].views[1].points.is_empty());
        // The detector's order is kept: the second view lists the points in reverse.
        let ids: Vec<usize> = gt.captures[1].views[0]
            .points
            .iter()
            .map(|p| p.point)
            .collect();
        assert_eq!(ids, (0..16).rev().collect::<Vec<_>>());
        // Each pixel is calibration-rs's projection of the point through the truth.
        let model = cams[1].params.build().unwrap();
        let v = &gt.captures[1].views[1];
        let c_se3_t = v.world_se3_camera.inverse() * gt.captures[1].world_se3_target;
        for (p, xyz) in v.points.iter().zip(&second[..6]) {
            let want = model
                .project_point_c(&(c_se3_t * Point3::from(*xyz)).coords)
                .unwrap();
            assert_eq!(p.pixel, Some([want.x, want.y]));
        }
        // Exact observations are the projections, view by view, in order.
        let obs = observe(&gt, &ObservationNoise::default());
        assert_eq!(obs.captures[1].views[1].points.len(), 6);
        assert_eq!(
            obs.captures[1].views[1].points[0].pixel,
            v.points[0].pixel.unwrap()
        );
        assert_eq!(obs.captures[1].views[1].points[0].target_xyz, second[0]);
    }

    #[test]
    fn a_view_with_another_camera_count_is_refused() {
        let views = [TwinView {
            base_se3_tcp: pose(0.0),
            points: vec![Some(grid(2)), None],
        }];
        let e = twin(
            Topology::SingleCamHandeye,
            &[camera("a", 0.0)],
            handeye(),
            &views,
        )
        .unwrap_err();
        assert!(e.to_string().contains("view 0 has 2 cameras"), "{e}");
    }
}
