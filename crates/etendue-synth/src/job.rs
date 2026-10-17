//! Render jobs for the Blender backend ([ADR 0005](https://github.com/VitalyVorobyev/etendue/blob/main/docs/adrs/0005-blender-renderer.md)).
//!
//! A [`RenderJob`] is everything the embedded Blender script needs and nothing
//! it would have to compute: meshes and target surfaces (as triangles) bound
//! to frame names, lights,
//! each camera's canonical pinhole (ADR 0004), and per shot the baked
//! `world_se3_frame` of every frame it uses, as row-major 4 × 4 matrices. The
//! script builds the scene once and only sets matrices per shot — no
//! kinematics, no camera-model math.

use std::collections::{BTreeMap, BTreeSet};

use etendue_scene::{BakedScenario, LightShape, SceneSpec, TargetGeometry};
use nalgebra::Isometry3;
use serde::{Deserialize, Serialize};

use crate::remap::CanonicalCamera;
use crate::target_mesh::geometry_mesh;
use crate::{Error, Result};

/// The job format version the embedded script reads. Version 3 draws
/// targets from triangle meshes (any calib-targets target, the PuzzlePole
/// cylinder included) instead of planar cells. Version 4 adds laser views:
/// cameras offset from their frame, depth-only outputs, and the Normal and
/// Diffuse Color passes laser images are composited from.
pub const JOB_VERSION: u32 = 4;

/// Object-index pass value of targets (ADR 0006 occlusion: `IndexOB == this`
/// means the target is visible at that pixel).
pub const TARGET_PASS_INDEX: u32 = 1;
/// Object-index pass value of robot meshes.
pub const ROBOT_PASS_INDEX: u32 = 100;

/// Where Cycles runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Device {
    /// The first GPU backend available (Metal, OptiX, CUDA, HIP, oneAPI), else CPU.
    Gpu,
    /// CPU only.
    Cpu,
}

/// Render settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderSettings {
    /// Cycles samples per pixel.
    pub samples: u32,
    /// Cycles seed (fixed: renders are reproducible, P4-5).
    pub seed: u32,
    /// Where Cycles runs.
    pub device: Device,
    /// Radiance of a uniform white environment. A scene without lights needs
    /// some; `0` turns it off.
    pub ambient: f64,
    /// Near and far clip distances in metres.
    pub clip: [f64; 2],
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            samples: 64,
            seed: 0,
            device: Device::Gpu,
            ambient: 1.0,
            clip: [0.01, 50.0],
        }
    }
}

/// A mesh file placed at a frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobMesh {
    /// Object name.
    pub id: String,
    /// Frame it moves with.
    pub frame: String,
    /// Absolute path of the GLB (vertices in the frame, metres, Z up).
    pub path: String,
    /// Object-index pass value.
    pub pass_index: u32,
}

/// A target surface, built by the script in its frame from triangles, ink or
/// paper ([`crate::target_mesh`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobTarget {
    /// Object name.
    pub id: String,
    /// Frame it moves with.
    pub frame: String,
    /// Vertex positions in the frame, metres.
    pub positions: Vec<[f64; 3]>,
    /// Triangles (counter-clockwise from outside).
    pub triangles: Vec<[u32; 3]>,
    /// Per triangle: ink (`true`) or paper.
    pub dark: Vec<bool>,
    /// Object-index pass value.
    pub pass_index: u32,
}

impl JobTarget {
    /// The job entry of target `id`, drawn at frame `frame`.
    ///
    /// # Errors
    ///
    /// As [`geometry_mesh`].
    pub fn new(id: &str, frame: &str, geometry: &TargetGeometry) -> Result<Self> {
        let mesh = geometry_mesh(geometry)?;
        Ok(Self {
            id: id.to_owned(),
            frame: frame.to_owned(),
            positions: mesh.positions,
            triangles: mesh.triangles,
            dark: mesh.dark,
            pass_index: TARGET_PASS_INDEX,
        })
    }
}

/// A light (etendue `LightSpec`; directional lights emit along local +Z).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobLight {
    /// Object name.
    pub id: String,
    /// Frame it moves with.
    pub frame: String,
    /// Radiant power, watts.
    pub power_w: f64,
    /// Linear RGB.
    pub color: [f64; 3],
    /// Emitter shape.
    pub shape: LightShape,
}

/// A small emissive sphere in world coordinates, for convention probes (P4-2). Its
/// radiance is `emission · (N·V)²` — smooth, zero at the limb — so its image is a smooth
/// blob whose intensity-weighted centroid is the projection of its centre.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobSphere {
    /// Object name.
    pub id: String,
    /// Centre in the world, metres.
    pub center: [f64; 3],
    /// Radius, metres.
    pub radius: f64,
    /// Emission strength (radiance of the white emitter).
    pub emission: f64,
}

/// A camera's canonical pinhole.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobCamera {
    /// Camera id.
    pub id: String,
    /// Its frame (a CV camera frame).
    pub frame: String,
    /// Canonical width, pixels.
    pub width: u32,
    /// Canonical height, pixels.
    pub height: u32,
    /// Canonical focal length, pixels.
    pub focal_px: f64,
    /// Pose of the camera in its frame, row-major 4 × 4, when it is not the
    /// frame itself (a laser view: a CV camera frame in the laser's frame).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_se3_self: Option<[f64; 16]>,
}

/// What an output is for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputKind {
    /// A camera image at the job's samples.
    #[default]
    Image,
    /// Only the Depth pass matters (a laser view, for shadows): one sample.
    Depth,
}

/// One image to render.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobOutput {
    /// Camera id.
    pub camera: String,
    /// EXR path relative to the output directory.
    pub path: String,
    /// What it is for.
    #[serde(default, skip_serializing_if = "is_image")]
    pub kind: OutputKind,
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde's skip_serializing_if signature
fn is_image(kind: &OutputKind) -> bool {
    *kind == OutputKind::Image
}

/// One posed instant: a capture of the scenario.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobShot {
    /// Capture id.
    pub capture: String,
    /// Baked sample index.
    pub sample: usize,
    /// `world_se3_frame`, row-major 4 × 4, for every frame the job uses.
    pub poses: BTreeMap<String, [f64; 16]>,
    /// The images to render at this shot.
    pub outputs: Vec<JobOutput>,
}

/// A render job (`job.json`).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderJob {
    /// [`JOB_VERSION`].
    pub version: u32,
    /// Render settings.
    pub render: RenderSettings,
    /// Meshes (robot links, parts).
    pub meshes: Vec<JobMesh>,
    /// Targets.
    pub targets: Vec<JobTarget>,
    /// Lights.
    pub lights: Vec<JobLight>,
    /// Emissive probe spheres (fixed in the world).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spheres: Vec<JobSphere>,
    /// Cameras.
    pub cameras: Vec<JobCamera>,
    /// Shots, in time order.
    pub shots: Vec<JobShot>,
    /// Also write the Normal and Diffuse Color passes (for laser images).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub laser_passes: bool,
}

/// Row-major 4 × 4 of an isometry.
#[must_use]
pub fn row_major(iso: &Isometry3<f64>) -> [f64; 16] {
    let m = iso.to_homogeneous();
    std::array::from_fn(|k| m[(k / 4, k % 4)])
}

/// Build the job for every capture of `baked`.
///
/// `meshes` are the scene's mesh files (robot links as `"<robot>/<link>"`
/// frames, parts at their own frame) with absolute paths; `cameras` gives each
/// camera to render its canonical camera. Output paths are
/// `exr/<camera>/<capture>.exr`.
///
/// # Errors
///
/// [`Error::InvalidInput`] if the scenario has no capture, a camera or frame
/// is unknown, or a target's surface is undetermined (a `board` puzzleboard
/// named by layout).
pub fn build_job(
    scene: &SceneSpec,
    baked: &BakedScenario,
    meshes: Vec<JobMesh>,
    cameras: &[(&str, &CanonicalCamera)],
    render: RenderSettings,
) -> Result<RenderJob> {
    let captures: Vec<usize> = baked.capture_indices().collect();
    if captures.is_empty() {
        return Err(Error::InvalidInput("the scenario has no capture".into()));
    }
    let targets = scene
        .targets
        .iter()
        .map(|t| {
            JobTarget::new(&t.id, &t.id, &t.geometry)
                .map_err(|e| Error::InvalidInput(format!("target `{}`: {e}", t.id)))
        })
        .collect::<Result<Vec<_>>>()?;
    let lights: Vec<JobLight> = scene
        .lights
        .iter()
        .map(|l| JobLight {
            id: l.id.clone(),
            frame: l.id.clone(),
            power_w: l.power_w,
            color: l.color,
            shape: l.shape.clone(),
        })
        .collect();
    let job_cameras = cameras
        .iter()
        .map(|(id, canonical)| {
            if !scene.cameras.iter().any(|c| c.id == *id) {
                return Err(Error::InvalidInput(format!("no camera `{id}`")));
            }
            Ok(JobCamera {
                id: (*id).to_owned(),
                frame: (*id).to_owned(),
                width: canonical.resolution[0],
                height: canonical.resolution[1],
                focal_px: canonical.focal_px(),
                frame_se3_self: None,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let used: BTreeSet<&str> = meshes
        .iter()
        .map(|m| m.frame.as_str())
        .chain(targets.iter().map(|b| b.frame.as_str()))
        .chain(lights.iter().map(|l| l.frame.as_str()))
        .chain(job_cameras.iter().map(|c| c.frame.as_str()))
        .collect();
    let index: BTreeMap<&str, usize> = baked
        .frames
        .iter()
        .enumerate()
        .map(|(i, f)| (f.as_str(), i))
        .collect();
    let columns = used
        .iter()
        .map(|f| {
            index.get(f).map(|&i| (*f, i)).ok_or_else(|| {
                Error::InvalidInput(format!("frame `{f}` is not in the baked scenario"))
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let shots = captures
        .iter()
        .map(|&k| {
            let sample = &baked.samples[k];
            let capture = sample
                .capture
                .as_ref()
                .map(|c| c.id.clone())
                .unwrap_or_default();
            JobShot {
                poses: columns
                    .iter()
                    .map(|&(f, i)| (f.to_owned(), row_major(&sample.world_se3_frame[i])))
                    .collect(),
                outputs: job_cameras
                    .iter()
                    .map(|c| JobOutput {
                        camera: c.id.clone(),
                        path: format!("exr/{}/{capture}.exr", c.id),
                        kind: OutputKind::Image,
                    })
                    .collect(),
                capture,
                sample: k,
            }
        })
        .collect();

    Ok(RenderJob {
        version: JOB_VERSION,
        render,
        meshes,
        targets,
        lights,
        spheres: Vec::new(),
        cameras: job_cameras,
        shots,
        laser_passes: false,
    })
}

/// A depth-only view from a laser's origin, for its shadows.
#[derive(Clone, Debug)]
pub struct LaserView {
    /// Camera id in the job (`<laser>/view<k>`).
    pub id: String,
    /// The laser.
    pub laser: String,
    /// The view's pinhole.
    pub camera: CanonicalCamera,
    /// Its CV camera frame in the laser frame.
    pub laser_se3_view: Isometry3<f64>,
}

impl LaserView {
    /// EXR path of this view at `capture`.
    #[must_use]
    pub fn path(&self, capture: &str) -> String {
        format!("exr/{}/{capture}.exr", self.id)
    }
}

/// Add laser images to `job`: the Normal and Diffuse Color passes on every
/// image, and every view of `views` rendered (depth only) at every shot.
///
/// # Errors
///
/// [`Error::InvalidInput`] if a view's laser is not a frame of `baked`.
pub fn add_laser_views(
    job: &mut RenderJob,
    baked: &BakedScenario,
    views: &[LaserView],
) -> Result<()> {
    job.laser_passes = true;
    for view in views {
        let i = baked
            .frames
            .iter()
            .position(|f| *f == view.laser)
            .ok_or_else(|| {
                Error::InvalidInput(format!(
                    "laser `{}` is not in the baked scenario",
                    view.laser
                ))
            })?;
        job.cameras.push(JobCamera {
            id: view.id.clone(),
            frame: view.laser.clone(),
            width: view.camera.resolution[0],
            height: view.camera.resolution[1],
            focal_px: view.camera.focal_px(),
            frame_se3_self: Some(row_major(&view.laser_se3_view)),
        });
        for shot in &mut job.shots {
            shot.poses.insert(
                view.laser.clone(),
                row_major(&baked.samples[shot.sample].world_se3_frame[i]),
            );
            shot.outputs.push(JobOutput {
                camera: view.id.clone(),
                path: view.path(&shot.capture),
                kind: OutputKind::Depth,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::{Translation3, UnitQuaternion, Vector3};

    #[test]
    fn row_major_layout() {
        let iso = Isometry3::from_parts(
            Translation3::new(1.0, 2.0, 3.0),
            UnitQuaternion::from_axis_angle(&Vector3::z_axis(), std::f64::consts::FRAC_PI_2),
        );
        let m = row_major(&iso);
        // First row: (cos, −sin, 0, tx).
        assert!((m[0]).abs() < 1e-15 && (m[1] + 1.0).abs() < 1e-15 && m[3] == 1.0);
        assert_eq!(&m[12..], &[0.0, 0.0, 0.0, 1.0]);
        assert_eq!(m[7], 2.0);
    }
}
