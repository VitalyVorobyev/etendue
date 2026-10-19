//! What a rig covers (P11): for every voxel of a box in the rig's frame, how many of its cameras
//! see it sharply, and how many of its (laser, camera) pairs see their own laser line there.
//!
//! The rig is a [`RigLayout`], expanded as the scene expands it. A camera with optics is a
//! [`PhysicalCamera`] at its mechanical frame (ADR 0008): it sees a point when the point projects
//! onto its sensor and the total blur there (circle of confusion and the Airy disc, together) is
//! at most `max_blur_px`. A camera without optics is its pinhole and has no blur. A pair counts a
//! voxel when the voxel meets its laser's fan (the plane cuts the voxel, within the fan's angle
//! and reach) and the laser's camera sees the voxel's centre.
//!
//! Stereo needs two views of a point and a triangulation head one pair, so the two counts are
//! what a rig is judged by: [`Coverage::volume_m3`] gives the volume with at least `m` of either.

use nalgebra::{Isometry3, Point2, Point3, Vector3};
use serde::{Deserialize, Serialize};
use vision_calibration_core::{CameraModel, CameraParams};

use etendue_scene::{CameraSpec, FrameRef, RigLayout};

use crate::from_scene::physical_camera;
use crate::optics::camera::PhysicalCamera;
use crate::{Error, Result};

/// The box and grid sampled, in the rig's frame.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grid {
    /// Minimum corner (m).
    pub min: [f64; 3],
    /// Maximum corner (m).
    pub max: [f64; 3],
    /// Voxels along x, y and z.
    pub n: [usize; 3],
}

impl Grid {
    /// A voxel's edge lengths (m).
    #[must_use]
    pub fn voxel(&self) -> [f64; 3] {
        std::array::from_fn(|i| (self.max[i] - self.min[i]) / self.n[i] as f64)
    }

    /// The centre of voxel `(i, j, k)`.
    #[must_use]
    pub fn centre(&self, i: usize, j: usize, k: usize) -> Point3<f64> {
        let d = self.voxel();
        Point3::new(
            self.min[0] + (i as f64 + 0.5) * d[0],
            self.min[1] + (j as f64 + 0.5) * d[1],
            self.min[2] + (k as f64 + 0.5) * d[2],
        )
    }

    fn check(&self) -> Result<()> {
        let ok = (0..3).all(|i| {
            self.min[i].is_finite() && self.max[i].is_finite() && self.max[i] > self.min[i]
        }) && self.n.iter().all(|&n| (1..=512).contains(&n));
        if ok {
            Ok(())
        } else {
            Err(Error::InvalidInput {
                reason: "a coverage grid needs a box with max > min and 1–512 voxels per axis"
                    .into(),
            })
        }
    }
}

/// What counts as seeing.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Criteria {
    /// The most total blur a camera may have at a point it sees (px).
    pub max_blur_px: f64,
    /// The wavelength of a camera's Airy disc when no laser of its own sets it (nm).
    pub wavelength_nm: f64,
}

impl Default for Criteria {
    fn default() -> Self {
        Self {
            max_blur_px: 2.0,
            wavelength_nm: 550.0,
        }
    }
}

/// A rig's coverage of a grid: two counts per voxel, x fastest, then y, then z.
#[derive(Clone, Debug, Serialize)]
pub struct Coverage {
    /// The grid.
    pub grid: Grid,
    /// The rig's cameras and (laser, camera) pairs.
    pub cameras: usize,
    /// The pairs.
    pub pairs: usize,
    /// How many cameras see each voxel sharply.
    pub views: Vec<u8>,
    /// How many pairs see their laser line in each voxel.
    pub lit: Vec<u8>,
}

impl Coverage {
    /// The volume where at least `views` cameras see and at least `lit` pairs measure (m³).
    #[must_use]
    pub fn volume_m3(&self, views: u8, lit: u8) -> f64 {
        let [dx, dy, dz] = self.grid.voxel();
        let n = self
            .views
            .iter()
            .zip(&self.lit)
            .filter(|&(&v, &l)| v >= views && l >= lit)
            .count();
        n as f64 * dx * dy * dz
    }
}

/// A camera as coverage sees it: where it is, and how it images.
struct Eye {
    /// The camera's (mechanical) frame from the rig's.
    from_rig: Isometry3<f64>,
    model: Model,
    size: [f64; 2],
    /// The Airy disc at the camera's wavelength (px).
    airy_px: f64,
}

enum Model {
    Physical(Box<PhysicalCamera>),
    Pinhole(Box<CameraModel>),
}

impl Eye {
    fn new(spec: &CameraSpec, wavelength_nm: f64) -> Result<Self> {
        let (model, airy_px) = match &spec.optics {
            Some(optics) => {
                let camera = physical_camera(optics, &spec.params)?;
                let airy = camera.airy_diameter_m(wavelength_nm * 1e-9) / camera.pixel_pitch_m();
                (Model::Physical(Box::new(camera)), airy)
            }
            None => (Model::Pinhole(Box::new(build(&spec.params)?)), 0.0),
        };
        Ok(Self {
            from_rig: spec.parent_se3_self.inverse(),
            model,
            size: spec.resolution.map(f64::from),
            airy_px,
        })
    }

    /// Where the camera is sharp along its axis: its depth of field for a blur limit that leaves
    /// room for its Airy disc, so the same depths `sees` passes.
    fn sharpness(&self, max_blur_px: f64) -> Sharpness {
        let Model::Physical(c) = &self.model else {
            return Sharpness::Everywhere;
        };
        if max_blur_px <= self.airy_px {
            return Sharpness::Nowhere;
        }
        let coc_px = (max_blur_px * max_blur_px - self.airy_px * self.airy_px).sqrt();
        // The only refusal left is a limit at least the exit pupil: no depth exceeds it.
        c.depth_of_field(coc_px * c.pixel_pitch_m())
            .map_or(Sharpness::Everywhere, |d| Sharpness::Between {
                near: d.near,
                far: d.far,
            })
    }

    /// Whether the camera sees `p` (rig frame) with at most `max_blur_px`.
    fn sees(&self, p: &Point3<f64>, max_blur_px: f64) -> bool {
        let q = self.from_rig * p;
        if q.z <= 0.0 {
            return false;
        }
        let on = |px: Point2<f64>| {
            (0.0..=self.size[0]).contains(&px.x) && (0.0..=self.size[1]).contains(&px.y)
        };
        match &self.model {
            Model::Pinhole(m) => m.project_point_c(&q.coords).is_some_and(on),
            Model::Physical(c) => {
                c.project(&q).is_some_and(on)
                    && c.coc_px(&q)
                        .is_ok_and(|coc| libm::hypot(coc, self.airy_px) <= max_blur_px)
            }
        }
    }
}

fn build(params: &CameraParams) -> Result<CameraModel> {
    params.build().map_err(|e| Error::InvalidInput {
        reason: format!("a rig camera does not build: {e}"),
    })
}

/// A laser's fan in the rig's frame.
struct Fan {
    from_rig: Isometry3<f64>,
    /// Half the extent of a voxel across the fan's plane (m).
    half_band: f64,
    half_angle: f64,
    length: f64,
    camera: usize,
}

impl Fan {
    fn meets(&self, p: &Point3<f64>) -> bool {
        let q = self.from_rig * p;
        q.x.abs() <= self.half_band
            && q.z > 0.0
            && libm::atan2(q.y.abs(), q.z) <= self.half_angle
            && libm::hypot(q.y, q.z) <= self.length
    }
}

/// Where a rig camera is sharp along its own axis, as [`coverage`] judges sharpness (its blur
/// with its Airy disc at most `max_blur_px`), as depth `z` in the camera's mechanical frame.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Sharpness {
    /// A pinhole, or a limit no defocus reaches: sharp at every depth.
    Everywhere,
    /// Diffraction alone is over the limit.
    Nowhere,
    /// From `near` to `far` (`None`: to infinity).
    Between { near: f64, far: Option<f64> },
}

/// The frame a rig is expanded in.
const RIG: &str = "rig";

/// `layout`, checked and expanded in [`RIG`].
fn expand(layout: &RigLayout) -> Result<(Vec<CameraSpec>, Vec<etendue_scene::LaserSpec>)> {
    layout.validate().map_err(|e| Error::InvalidInput {
        reason: e.to_string(),
    })?;
    Ok(layout.expand(RIG))
}

/// The cameras as coverage sees them, each at its own laser's wavelength when it has one.
fn eyes(
    cameras: &[CameraSpec],
    lasers: &[etendue_scene::LaserSpec],
    criteria: &Criteria,
) -> Result<Vec<Eye>> {
    cameras
        .iter()
        .map(|c| {
            let own = lasers
                .iter()
                .find(|l| l.camera.as_deref() == Some(c.id.as_str()));
            Eye::new(c, own.map_or(criteria.wavelength_nm, |l| l.wavelength_nm))
        })
        .collect()
}

/// Where each of `layout`'s cameras is sharp along its axis, in the order it expands them.
///
/// # Errors
///
/// As [`coverage`], without the grid.
pub fn sharpness(layout: &RigLayout, criteria: &Criteria) -> Result<Vec<Sharpness>> {
    let (cameras, lasers) = expand(layout)?;
    Ok(eyes(&cameras, &lasers, criteria)?
        .iter()
        .map(|e| e.sharpness(criteria.max_blur_px))
        .collect())
}

/// The coverage of `layout`'s rig over `grid`.
///
/// # Errors
///
/// [`Error::InvalidInput`] for a bad grid, a layout that does not validate, or a camera that
/// does not build; as [`PhysicalCamera::new`].
pub fn coverage(layout: &RigLayout, grid: &Grid, criteria: &Criteria) -> Result<Coverage> {
    grid.check()?;
    let (cameras, lasers) = expand(layout)?;
    let index = |id: &str| cameras.iter().position(|c| c.id == id);
    let voxel = grid.voxel();
    let eyes = eyes(&cameras, &lasers, criteria)?;
    let fans: Vec<Fan> = lasers
        .iter()
        .filter_map(|l| {
            let camera = index(l.camera.as_deref()?)?;
            let rig_se3_laser = match &l.parent {
                p if *p == FrameRef::from(RIG) => l.parent_se3_self,
                p => cameras[index(p.as_str())?].parent_se3_self * l.parent_se3_self,
            };
            let n = rig_se3_laser.rotation * Vector3::x();
            Some(Fan {
                from_rig: rig_se3_laser.inverse(),
                half_band: 0.5 * (0..3).map(|i| n[i].abs() * voxel[i]).sum::<f64>(),
                half_angle: l.fan_half_angle,
                length: l.fan_length,
                camera,
            })
        })
        .collect();
    let [nx, ny, nz] = grid.n;
    let mut views = vec![0_u8; nx * ny * nz];
    let mut lit = vec![0_u8; nx * ny * nz];
    let mut seen = vec![false; eyes.len()];
    for k in 0..nz {
        for j in 0..ny {
            for i in 0..nx {
                let p = grid.centre(i, j, k);
                let at = i + nx * (j + ny * k);
                for (e, s) in eyes.iter().zip(&mut seen) {
                    *s = e.sees(&p, criteria.max_blur_px);
                }
                views[at] = u8::try_from(seen.iter().filter(|&&s| s).count()).unwrap_or(u8::MAX);
                let pairs = fans
                    .iter()
                    .filter(|f| seen[f.camera] && f.meets(&p))
                    .count();
                lit[at] = u8::try_from(pairs).unwrap_or(u8::MAX);
            }
        }
    }
    Ok(Coverage {
        grid: *grid,
        cameras: eyes.len(),
        pairs: fans.len(),
        views,
        lit,
    })
}
