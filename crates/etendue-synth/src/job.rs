//! Render jobs for the Blender backend ([ADR 0005](https://github.com/VitalyVorobyev/etendue/blob/main/docs/adrs/0005-blender-renderer.md)).
//!
//! A [`RenderJob`] is everything the embedded Blender script needs and nothing
//! it would have to compute: meshes and boards bound to frame names, lights,
//! each camera's canonical pinhole (ADR 0004), and per shot the baked
//! `world_se3_frame` of every frame it uses, as row-major 4 × 4 matrices. The
//! script builds the scene once and only sets matrices per shot — no
//! kinematics, no camera-model math.

use std::collections::{BTreeMap, BTreeSet};

use etendue_scene::{BakedScenario, LightShape, SceneSpec, TargetGeometry};
use nalgebra::Isometry3;
use serde::{Deserialize, Serialize};

use crate::remap::CanonicalCamera;
use crate::{Error, Result};

/// The job format version the embedded script reads.
pub const JOB_VERSION: u32 = 1;

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

/// A checkerboard's squares.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checker {
    /// Squares along the board's X.
    pub cols: u32,
    /// Squares along the board's Y.
    pub rows: u32,
}

/// A planar target, built by the script in the target frame (`z = 0`, facing
/// +Z, columns along X, the dark square at −X/−Y — the layout etendue's web
/// viewers draw).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobBoard {
    /// Object name.
    pub id: String,
    /// Frame it moves with.
    pub frame: String,
    /// Extent along X, metres.
    pub width: f64,
    /// Extent along Y, metres.
    pub height: f64,
    /// Checkerboard, if the target is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checker: Option<Checker>,
    /// Object-index pass value.
    pub pass_index: u32,
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
}

/// One image to render.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobOutput {
    /// Camera id.
    pub camera: String,
    /// EXR path relative to the output directory.
    pub path: String,
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
    /// Boards and plain targets.
    pub boards: Vec<JobBoard>,
    /// Lights.
    pub lights: Vec<JobLight>,
    /// Emissive probe spheres (fixed in the world).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spheres: Vec<JobSphere>,
    /// Cameras.
    pub cameras: Vec<JobCamera>,
    /// Shots, in time order.
    pub shots: Vec<JobShot>,
}

/// Row-major 4 × 4 of an isometry.
#[must_use]
pub fn row_major(iso: &Isometry3<f64>) -> [f64; 16] {
    let m = iso.to_homogeneous();
    std::array::from_fn(|k| m[(k / 4, k % 4)])
}

/// The squares of a board target, columns along X; `None` for a plain
/// surface or a layout without squares.
#[must_use]
pub fn checker_of(geometry: &TargetGeometry) -> Option<Checker> {
    use vision_calibration_dataset::TargetSpec as Board;
    match geometry {
        TargetGeometry::Board {
            board: Board::Chessboard { rows, cols, .. },
        } => Some(Checker {
            cols: cols + 1,
            rows: rows + 1,
        }),
        TargetGeometry::Board {
            board: Board::Charuco { rows, cols, .. },
        } => Some(Checker {
            cols: *cols,
            rows: *rows,
        }),
        _ => None,
    }
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
/// is unknown, or a target's extent is undetermined (puzzleboard, until its
/// layout source lands with P3-3).
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
    let boards = scene
        .targets
        .iter()
        .map(|t| {
            let [width, height] = t.geometry.extent_m().ok_or_else(|| {
                Error::InvalidInput(format!("target `{}`: extent unknown for this layout", t.id))
            })?;
            Ok(JobBoard {
                id: t.id.clone(),
                frame: t.id.clone(),
                width,
                height,
                checker: checker_of(&t.geometry),
                pass_index: TARGET_PASS_INDEX,
            })
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
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let used: BTreeSet<&str> = meshes
        .iter()
        .map(|m| m.frame.as_str())
        .chain(boards.iter().map(|b| b.frame.as_str()))
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
        boards,
        lights,
        spheres: Vec::new(),
        cameras: job_cameras,
        shots,
    })
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
