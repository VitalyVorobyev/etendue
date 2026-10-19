//! Gate G9.4 (ADR 0008 §4), scene part: a scene camera with optics, resolved into projection
//! frames, projects as its physical camera does; whatever hangs on it keeps its world pose; and
//! where the camera's frame is drawn (another parent, a moved cell) changes no pixel.
//!
//! ```bash
//! cargo test -p etendue-core --test resolve_scene
//! ```

use etendue_core::calibration::{
    CameraParams, DistortionParams, FxFyCxCySkew, IntrinsicsParams, ProjectionParams, SensorParams,
};
use etendue_core::from_scene::{derive, physical_camera, resolve};
use etendue_scene::{
    FrameGraph, FrameRef, FrameSpec, LensOptics, OpticsSpec, SceneSpec, SensorOptics,
};
use nalgebra::{Isometry3, Point3, Translation3, UnitQuaternion, Vector3};

fn optics(tilt: [f64; 2]) -> OpticsSpec {
    OpticsSpec {
        sensor: SensorOptics {
            name: Some("Sony IMX174".into()),
            pixel_pitch_m: 5.86e-6,
        },
        lens: LensOptics {
            name: Some("25 mm".into()),
            focal_length_m: 0.025,
            f_number: 4.0,
            principal_gap_m: 0.006,
            entrance_pupil_m: 0.004,
            focusing: Default::default(),
        },
        focus_distance_m: 0.4,
        tilt_rad: tilt,
    }
}

fn as_built() -> CameraParams {
    CameraParams {
        projection: ProjectionParams::Pinhole,
        distortion: DistortionParams::None,
        sensor: SensorParams::Identity,
        intrinsics: IntrinsicsParams::FxFyCxCySkew {
            params: FxFyCxCySkew {
                fx: 1.0,
                fy: 1.0,
                cx: 960.0,
                cy: 600.0,
                skew: 0.0,
            },
        },
    }
}

fn pose(axis: Vector3<f64>, angle: f64, t: [f64; 3]) -> Isometry3<f64> {
    Isometry3::from_parts(
        Translation3::new(t[0], t[1], t[2]),
        UnitQuaternion::from_axis_angle(&nalgebra::Unit::new_normalize(axis), angle),
    )
}

/// A cell: a mount frame, a camera with optics on it, a laser on the camera, a board.
fn cell(tilt: [f64; 2], cell_pose: Isometry3<f64>) -> SceneSpec {
    let o = optics(tilt);
    let (params, _) = derive(&o, &as_built()).unwrap();
    let json = serde_json::json!({
        "version": 1,
        "frames": [
            { "id": "cell", "parent": "world", "parent_se3_self": cell_pose },
            { "id": "mount", "parent": "cell", "parent_se3_self": pose(Vector3::new(1.0, 0.2, 0.0), 2.7, [0.1, -0.05, 0.6]) }
        ],
        "cameras": [{
            "id": "cam", "parent": "mount", "parent_se3_self": pose(Vector3::new(0.0, 1.0, 0.3), 0.2, [0.02, 0.0, 0.01]),
            "params": params, "resolution": [1920, 1200], "optics": o
        }],
        "lasers": [{
            "id": "laser", "parent": "cam", "parent_se3_self": pose(Vector3::new(1.0, 0.0, 0.0), -0.3, [0.06, 0.0, 0.0]),
            "fan_half_angle": 0.4, "fan_length": 1.0, "wavelength_nm": 660.0, "beam_waist_m": 0.0005
        }]
    });
    let scene: SceneSpec = serde_json::from_value(json).unwrap();
    scene.validate().unwrap();
    scene
}

fn world_poses(scene: &SceneSpec) -> (FrameGraph, Vec<Isometry3<f64>>) {
    let graph = FrameGraph::build(scene, &[]).unwrap();
    let poses = graph.resolve(|_, _| Isometry3::identity());
    (graph, poses)
}

fn world_of(scene: &SceneSpec, frame: &str) -> Isometry3<f64> {
    let (graph, poses) = world_poses(scene);
    poses[graph.index_of(frame).unwrap()]
}

fn world_points() -> Vec<Point3<f64>> {
    // In front of the camera, around its line of sight.
    let mut out = Vec::new();
    for i in 0..7 {
        for j in 0..5 {
            out.push(Point3::new(
                -0.15 + 0.05 * f64::from(i),
                -0.1 + 0.05 * f64::from(j),
                0.25 + 0.03 * f64::from(i + j),
            ));
        }
    }
    out
}

/// Pixels of `points` (given in the camera's mechanical frame) through its physical camera.
fn physical_pixels(
    scene: &SceneSpec,
    points: &[Point3<f64>],
) -> Vec<Option<nalgebra::Point2<f64>>> {
    let cam = &scene.cameras[0];
    let physical = physical_camera(cam.optics.as_ref().unwrap(), &cam.params).unwrap();
    points.iter().map(|p| physical.project(p)).collect()
}

/// Pixels of the same points (given in the mechanical frame) through the resolved pinhole.
fn resolved_pixels(
    scene: &SceneSpec,
    points: &[Point3<f64>],
) -> Vec<Option<nalgebra::Point2<f64>>> {
    let resolved = resolve(scene).unwrap();
    let world_se3_mech = world_of(scene, "cam");
    let world_se3_projection = world_of(&resolved.scene, "cam");
    let model = resolved.scene.cameras[0].params.build().unwrap();
    points
        .iter()
        .map(|p| model.project_point(&(world_se3_projection.inverse() * (world_se3_mech * p))))
        .collect()
}

fn worst(a: &[Option<nalgebra::Point2<f64>>], b: &[Option<nalgebra::Point2<f64>>]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x.expect("in view") - y.expect("in view")).norm())
        .fold(0.0, f64::max)
}

#[test]
fn the_resolved_pinhole_projects_as_the_physical_camera() {
    for tilt in [[0.0, 0.0], [0.06, 0.0], [0.05, -0.04]] {
        let scene = cell(tilt, Isometry3::identity());
        let gap = worst(
            &physical_pixels(&scene, &world_points()),
            &resolved_pixels(&scene, &world_points()),
        );
        assert!(gap.is_finite() && gap < 1e-9, "tilt {tilt:?}: {gap} px");
    }
}

#[test]
fn the_camera_moves_to_its_projection_frame_and_what_hangs_on_it_stays() {
    let scene = cell([0.05, -0.04], Isometry3::identity());
    let resolved = resolve(&scene).unwrap();
    let m_se3_p = resolved.mech_se3_projection["cam"];
    assert!(
        (m_se3_p.translation.z
            - physical_camera(
                scene.cameras[0].optics.as_ref().unwrap(),
                &scene.cameras[0].params
            )
            .unwrap()
            .cardinal_points()
            .entrance_pupil)
            .abs()
            < 1e-15
    );
    let expected = world_of(&scene, "cam") * m_se3_p;
    let got = world_of(&resolved.scene, "cam");
    assert!(
        (got.to_homogeneous() - expected.to_homogeneous())
            .abs()
            .max()
            < 1e-12
    );
    let laser_before = world_of(&scene, "laser");
    let laser_after = world_of(&resolved.scene, "laser");
    assert!(
        (laser_before.to_homogeneous() - laser_after.to_homogeneous())
            .abs()
            .max()
            < 1e-12
    );
    assert!(resolved.scene.cameras[0].optics.is_none());
}

#[test]
fn resolving_twice_changes_nothing() {
    let scene = cell([0.05, -0.04], Isometry3::identity());
    let once = resolve(&scene).unwrap();
    let twice = resolve(&once.scene).unwrap();
    assert!(twice.mech_se3_projection.is_empty());
    assert_eq!(
        serde_json::to_string(&once.scene).unwrap(),
        serde_json::to_string(&twice.scene).unwrap()
    );
}

#[test]
fn stale_params_are_reported_where_they_are() {
    let mut scene = cell([0.0, 0.0], Isometry3::identity());
    let IntrinsicsParams::FxFyCxCySkew { params } = &mut scene.cameras[0].params.intrinsics;
    params.fx *= 1.0 + 1e-6;
    let err = resolve(&scene).unwrap_err();
    assert_eq!(err.issues.len(), 1, "{err}");
    assert_eq!(err.issues[0].path, "cameras[0].params.intrinsics.fx");
    // A focus closer than the lens can reach is the optics' own problem.
    let mut scene = cell([0.0, 0.0], Isometry3::identity());
    scene.cameras[0].optics.as_mut().unwrap().focus_distance_m = 0.05;
    assert_eq!(
        resolve(&scene).unwrap_err().issues[0].path,
        "cameras[0].optics"
    );
}

#[test]
fn where_the_frames_are_drawn_changes_no_pixel() {
    let points = world_points();
    let reference = cell([0.05, -0.04], Isometry3::identity());
    let world_se3_mech = world_of(&reference, "cam");
    // World points the reference camera sees.
    let in_world: Vec<Point3<f64>> = points.iter().map(|p| world_se3_mech * p).collect();
    let pixels = |scene: &SceneSpec, world: &[Point3<f64>]| {
        let resolved = resolve(scene).unwrap();
        let world_se3_projection = world_of(&resolved.scene, "cam");
        let model = resolved.scene.cameras[0].params.build().unwrap();
        world
            .iter()
            .map(|p| model.project_point(&(world_se3_projection.inverse() * p)))
            .collect::<Vec<_>>()
    };
    let base = pixels(&reference, &in_world);
    // 1. The whole cell moved: the points move with it.
    let moved_by = pose(Vector3::new(0.3, -1.0, 0.5), 1.1, [2.0, -1.0, 0.4]);
    let moved = cell([0.05, -0.04], moved_by);
    let moved_points: Vec<Point3<f64>> = in_world.iter().map(|p| moved_by * p).collect();
    assert!(worst(&base, &pixels(&moved, &moved_points)) < 1e-9);
    // 2. The camera re-parented to the world with the pose it had: nothing moves.
    let mut reparented = reference.clone();
    reparented.cameras[0].parent = FrameRef::world();
    reparented.cameras[0].parent_se3_self = world_se3_mech;
    let laser_world = world_of(&reference, "laser");
    reparented.lasers[0].parent = FrameRef::world();
    reparented.lasers[0].parent_se3_self = laser_world;
    reparented.frames.retain(|f: &FrameSpec| f.id == "cell");
    assert!(worst(&base, &pixels(&reparented, &in_world)) < 1e-9);
}
