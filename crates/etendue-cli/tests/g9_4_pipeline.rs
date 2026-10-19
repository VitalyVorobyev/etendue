//! Gate G9.4 (ADR 0008 §4.1), through the pipeline: a camera with physical optics, placed by its
//! body, gives the same ground truth, dataset and render job as the plain pinhole at its entrance
//! pupil; and its ground-truth pixels are the physical camera's projection of the target, posed
//! from the body.
//!
//! ```bash
//! cargo test -p etendue-cli --test g9_4_pipeline
//! ```

use std::path::{Path, PathBuf};

use etendue_cli::{bake_file, gt, load};
use etendue_core::from_scene::{derive, physical_camera, resolve};
use etendue_kinematics::{BakeOptions, bake_with};
use etendue_scene::{LensOptics, OpticsSpec, ScenarioSpec, SceneSpec, SensorOptics};
use etendue_synth::dataset::GroundTruth;
use etendue_synth::job::{RenderSettings, build_job};
use etendue_synth::{CanonicalCamera, CanonicalSpec, PixelCentre};
use nalgebra::Point2;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `examples/eye_in_hand_ur5e` with `cam_left` given physical optics (a tilted sensor, a thick
/// lens with its pupil off H) and the params they derive; robot manifests made absolute.
fn physical_scene() -> SceneSpec {
    let dir = root().join("examples/eye_in_hand_ur5e");
    let mut scene: SceneSpec =
        serde_json::from_str(&std::fs::read_to_string(dir.join("scene.json")).unwrap()).unwrap();
    for r in &mut scene.robots {
        r.manifest = dir.join(&r.manifest).to_string_lossy().into_owned();
    }
    let optics = OpticsSpec {
        sensor: SensorOptics {
            name: None,
            pixel_pitch_m: 4.8e-6,
        },
        lens: LensOptics {
            name: None,
            focal_length_m: 0.012,
            f_number: 5.6,
            principal_gap_m: 0.004,
            entrance_pupil_m: 0.003,
            focusing: Default::default(),
        },
        focus_distance_m: 0.5,
        tilt_rad: [0.02, -0.01],
    };
    let camera = scene
        .cameras
        .iter_mut()
        .find(|c| c.id == "cam_left")
        .unwrap();
    camera.params = derive(&optics, &camera.params).unwrap().0;
    camera.optics = Some(optics);
    scene.validate().unwrap();
    scene
}

struct Run {
    gt: GroundTruth,
    gt_text: serde_json::Value,
    dataset: String,
    job: String,
}

fn run(scene: &SceneSpec, name: &str) -> Run {
    let dir = root().join("target/g9_4").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("scene.json");
    std::fs::write(&path, serde_json::to_string_pretty(scene).unwrap()).unwrap();
    let loaded = load(&path).unwrap();
    let scenario = root().join("examples/eye_in_hand_ur5e/scenario.json");
    let baked = bake_file(&loaded, &scenario, BakeOptions::default()).unwrap();
    let out = dir.join("gt");
    gt::write(&loaded, &baked, &out).unwrap();
    let read = |name: &str| std::fs::read_to_string(out.join(name)).unwrap();
    let canonical: Vec<(String, CanonicalCamera)> = loaded
        .scene
        .cameras
        .iter()
        .map(|c| {
            let cam = CanonicalCamera::cover(
                &c.params,
                c.resolution,
                &CanonicalSpec::default(),
                PixelCentre::Integer,
            )
            .unwrap();
            (c.id.clone(), cam)
        })
        .collect();
    let job = build_job(
        &loaded.scene,
        &baked,
        Vec::new(),
        &canonical
            .iter()
            .map(|(id, c)| (id.as_str(), c))
            .collect::<Vec<_>>(),
        RenderSettings::default(),
    )
    .unwrap();
    let gt_text: serde_json::Value = serde_json::from_str(&read("gt.json")).unwrap();
    Run {
        gt: serde_json::from_value(gt_text.clone()).unwrap(),
        gt_text,
        dataset: read("dataset.json"),
        job: serde_json::to_string(&job).unwrap(),
    }
}

#[test]
fn a_physical_camera_is_its_derived_pinhole_through_the_pipeline() {
    let physical = physical_scene();
    let pinhole = resolve(&physical).unwrap();
    let a = run(&physical, "physical");
    let b = run(&pinhole.scene, "pinhole");

    // The dataset and the render job are the same files.
    assert_eq!(a.dataset, b.dataset, "dataset.json differs");
    assert_eq!(a.job, b.job, "render job differs");
    // The ground truth too, but for where the physical camera's body is.
    let mut without = a.gt_text.clone();
    for c in without["cameras"].as_array_mut().unwrap() {
        c.as_object_mut().unwrap().remove("mech_se3_projection");
    }
    assert_eq!(without, b.gt_text, "gt.json differs");
    let left = a.gt.cameras.iter().find(|c| c.id == "cam_left").unwrap();
    assert_eq!(
        left.mech_se3_projection,
        Some(pinhole.mech_se3_projection["cam_left"])
    );
    assert!(
        a.gt.cameras
            .iter()
            .filter(|c| c.id != "cam_left")
            .all(|c| c.mech_se3_projection.is_none())
    );
}

#[test]
fn ground_truth_pixels_are_the_physical_cameras_from_its_body() {
    let physical = physical_scene();
    let a = run(&physical, "physical-pixels");
    // The body's pose comes from a bake of the scene as written (no resolution): the camera
    // frame there is the mechanical one.
    let dir = root().join("target/g9_4/physical-pixels");
    let loaded = load(&dir.join("scene.json")).unwrap();
    let scenario: ScenarioSpec = serde_json::from_str(
        &std::fs::read_to_string(root().join("examples/eye_in_hand_ur5e/scenario.json")).unwrap(),
    )
    .unwrap();
    let mechanical =
        bake_with(&physical, &scenario, &loaded.models, BakeOptions::default()).unwrap();
    let cam = mechanical
        .frames
        .iter()
        .position(|f| f == "cam_left")
        .unwrap();
    let spec = physical
        .cameras
        .iter()
        .find(|c| c.id == "cam_left")
        .unwrap();
    let model = physical_camera(spec.optics.as_ref().unwrap(), &spec.params).unwrap();

    let mut compared = 0;
    let mut worst: f64 = 0.0;
    for capture in &a.gt.captures {
        let world_se3_mech = mechanical.samples[capture.sample].world_se3_frame[cam];
        let view = capture
            .views
            .iter()
            .find(|v| v.camera == "cam_left")
            .unwrap();
        for p in view.points.iter().filter(|p| p.visible()) {
            let in_world = capture.world_se3_target * a.gt.target.points[p.point].xyz();
            let expected = model
                .project(&(world_se3_mech.inverse() * in_world))
                .unwrap();
            let [u, v] = p.pixel.unwrap();
            worst = worst.max((Point2::new(u, v) - expected).norm());
            compared += 1;
        }
    }
    println!("{compared} ground-truth pixels against the physical camera: worst {worst:.3e} px");
    assert!(compared > 100, "only {compared} points compared");
    assert!(worst < 1e-9, "worst {worst} px");
}
