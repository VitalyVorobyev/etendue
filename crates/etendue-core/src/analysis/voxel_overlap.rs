//! Voxelised N-view overlap analysis — the 3D coverage of a multi-pair rig.
//!
//! Where [`mod@crate::analysis::working_volume`] computes the 2D working
//! region of a **single** (camera, laser) pair on its fan plane, this module
//! computes the **3D** overlap of an **N-pair** rig: for each voxel in a
//! caller-specified bounding box, count how many of the scene's
//! (camera, laser) pairs can simultaneously **see**, **illuminate**, and
//! **focus on** that voxel. The result is a 3D scalar field — the
//! "agreement" of the rig — that the M10 industrial-inspection use case
//! cares about (how many pairs reach every point of the inspection volume).
//!
//! # The three predicates per pair
//!
//! For a voxel centre `p` (world coordinates) and a pair `(camera_i, laser_i)`:
//!
//! 1. **See** — `p` is in front of the camera (`(camera_from_world · p).z > 0`)
//!    and its pinhole projection lands inside the sensor rectangle. The
//!    pinhole approximation is used rather than the camera model's full
//!    Scheimpflug-homography projection because the latter depends on the
//!    `tau_x`/`tau_y` design parameters and would couple the overlap field
//!    to optimisation variables that should not affect *visibility* in the
//!    headline coverage metric.
//! 2. **Illuminate** — `p` is close enough to the laser fan plane (within
//!    `illum_thickness_m / 2`) **and** inside the fan's angular extent
//!    `±half_angle` **and** its radial reach `length`. The thickness band
//!    matches the laser's physical fan thickness; without it any voxel that
//!    doesn't fall *exactly* on the (zero-thickness) plane would fail.
//! 3. **Focus** — `p`'s circle of confusion under the camera's lens is
//!    below `coc_threshold_px` (the standard depth-of-field gate).
//!
//! A voxel is "covered by pair i" iff all three predicates hold. The per-voxel
//! integer count is the *overlap*; voxels covered by every pair (`count == N`)
//! are the rig's "full-agreement" working volume.
//!
//! # Why store counts, not booleans
//!
//! Storing a count keeps the analysis composable: a viewer can colour-code
//! by count (1 → blue, N → red), threshold to any minimum agreement (e.g.
//! "≥ 4 pairs see this voxel"), or compute statistics (the fraction of the
//! bounding box covered by at least one pair). A boolean "covered by every
//! pair" mask is just the `count >= N` threshold.

use nalgebra::Point3;
use serde::{Deserialize, Serialize};

use crate::laser::LaserPlane;
use crate::optics::ThickLens;
use crate::scene::Scene;

/// A 3D axis-aligned bounding box in world coordinates — the volume the
/// voxel grid samples.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct VoxelBox {
    /// Minimum corner `(x, y, z)` in metres.
    pub min: Point3<f64>,
    /// Maximum corner `(x, y, z)` in metres, with `max.* >= min.*`.
    pub max: Point3<f64>,
}

impl VoxelBox {
    /// Edge lengths `(dx, dy, dz)` of the box, in metres.
    #[must_use]
    pub fn size(&self) -> (f64, f64, f64) {
        (
            self.max.x - self.min.x,
            self.max.y - self.min.y,
            self.max.z - self.min.z,
        )
    }
}

/// Voxel-grid resolution along each world axis. All three counts must be
/// `>= 1`; one voxel along an axis collapses that axis to a single sample.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoxelResolution {
    /// Voxel count along world `+x`.
    pub nx: usize,
    /// Voxel count along world `+y`.
    pub ny: usize,
    /// Voxel count along world `+z`.
    pub nz: usize,
}

/// The voxelised N-view overlap field.
///
/// Stored row-major in `(nz, ny, nx)` order: the flat index for voxel
/// `(ix, iy, iz)` is `iz · (nx · ny) + iy · nx + ix`. The integer at each
/// cell is the number of (camera, laser) pairs that simultaneously see,
/// illuminate, and focus on that voxel's centre.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VoxelOverlap {
    /// The bounding box the field samples.
    bounds: VoxelBox,
    /// The grid resolution.
    resolution: VoxelResolution,
    /// Number of (camera, laser) pairs the field was computed against. The
    /// per-voxel count is in `0..=n_pairs`.
    n_pairs: usize,
    /// Per-voxel overlap count, row-major (z, y, x).
    counts: Vec<u32>,
}

impl VoxelOverlap {
    /// The sampled bounding box.
    #[must_use]
    pub fn bounds(&self) -> &VoxelBox {
        &self.bounds
    }

    /// The grid resolution.
    #[must_use]
    pub fn resolution(&self) -> &VoxelResolution {
        &self.resolution
    }

    /// Number of (camera, laser) pairs in the analysis.
    #[must_use]
    pub fn n_pairs(&self) -> usize {
        self.n_pairs
    }

    /// Row-major slice of per-voxel overlap counts. Indexed
    /// `iz · nx · ny + iy · nx + ix`.
    #[must_use]
    pub fn counts(&self) -> &[u32] {
        &self.counts
    }

    /// World-space centre of voxel `(ix, iy, iz)`.
    #[must_use]
    pub fn voxel_centre(&self, ix: usize, iy: usize, iz: usize) -> Point3<f64> {
        let (sx, sy, sz) = self.bounds.size();
        let dx = sx / self.resolution.nx as f64;
        let dy = sy / self.resolution.ny as f64;
        let dz = sz / self.resolution.nz as f64;
        Point3::new(
            self.bounds.min.x + (ix as f64 + 0.5) * dx,
            self.bounds.min.y + (iy as f64 + 0.5) * dy,
            self.bounds.min.z + (iz as f64 + 0.5) * dz,
        )
    }

    /// Total voxel count `nx · ny · nz`.
    #[must_use]
    pub fn voxel_count(&self) -> usize {
        self.counts.len()
    }

    /// Number of voxels with `overlap >= min_count`.
    ///
    /// `min_count = n_pairs` returns the "full agreement" count — voxels every
    /// pair simultaneously covers. `min_count = 1` returns voxels covered by
    /// any pair.
    #[must_use]
    pub fn count_at_least(&self, min_count: u32) -> usize {
        self.counts.iter().filter(|&&c| c >= min_count).count()
    }

    /// Maximum overlap count observed in the field, in `0..=n_pairs`.
    #[must_use]
    pub fn max_overlap(&self) -> u32 {
        self.counts.iter().copied().max().unwrap_or(0)
    }
}

/// Compute the voxelised N-view overlap field over `bounds` at `resolution`.
///
/// See the module doc-comment for the predicate definitions and the
/// "see / illuminate / focus" decomposition. Iterates the scene's pairs:
/// pair `i` is `(scene.cameras[i], scene.lasers[i])` for `i ∈ 0..min(N_cam, N_las)`.
/// Targets are not consulted (the overlap is a property of the rig + the
/// inspection volume, not of any specific target inside it).
///
/// # Parameters
///
/// - `scene`: the rig; pairs are read as `(cameras[i], lasers[i])`.
/// - `bounds`: the world-space bounding box to voxelise.
/// - `resolution`: the per-axis voxel count.
/// - `coc_threshold_px`: the focus predicate's CoC threshold in pixels.
/// - `illum_thickness_m`: the half-thickness of the "near the fan plane"
///   illumination band. For a tight line laser ~1 mm is generous; for a
///   diffuse fan it can be the typical beam diameter at the working
///   distance.
///
/// # Errors
///
/// Returns [`Error::InvalidInput`](crate::Error::InvalidInput) if the bounding
/// box is degenerate, the resolution is zero on any axis, or the thresholds
/// are non-finite / non-positive. Propagates
/// [`CameraEntity::thick_lens`](crate::scene::CameraEntity::thick_lens)'s
/// errors per pair (cannot fire for entities built through
/// [`CameraEntity::new`](crate::scene::CameraEntity::new)).
pub fn voxelized_overlap(
    scene: &Scene,
    bounds: VoxelBox,
    resolution: VoxelResolution,
    coc_threshold_px: f64,
    illum_thickness_m: f64,
) -> crate::Result<VoxelOverlap> {
    validate(&bounds, &resolution, coc_threshold_px, illum_thickness_m)?;
    let n_pairs = scene.cameras.len().min(scene.lasers.len());

    // Pre-compute per-pair derived data so the inner loop is just the
    // three predicate checks per voxel.
    let mut per_pair: Vec<PairContext> = Vec::with_capacity(n_pairs);
    for i in 0..n_pairs {
        let camera = &scene.cameras[i];
        let laser = &scene.lasers[i];
        let lens: ThickLens = camera.thick_lens()?;
        let camera_model = camera.params.build();
        let (cx, cy) = match &camera.params.intrinsics {
            vision_calibration_core::IntrinsicsParams::FxFyCxCySkew { params } => {
                (params.cx, params.cy)
            }
        };
        per_pair.push(PairContext {
            camera_from_world: camera.pose.inverse(),
            sensor_extent: camera.resolution_f64(),
            principal_point: (cx, cy),
            focal_px: camera.focal_length_px(),
            camera_model,
            lens,
            laser_plane: LaserPlane::from_entity(laser),
            laser_length: laser.fan_length,
            laser_half_angle: laser.fan_half_angle,
        });
    }

    let (sx, sy, sz) = bounds.size();
    let dx = sx / resolution.nx as f64;
    let dy = sy / resolution.ny as f64;
    let dz = sz / resolution.nz as f64;
    let total = resolution.nx * resolution.ny * resolution.nz;
    let mut counts = vec![0u32; total];

    for iz in 0..resolution.nz {
        for iy in 0..resolution.ny {
            for ix in 0..resolution.nx {
                let p = Point3::new(
                    bounds.min.x + (ix as f64 + 0.5) * dx,
                    bounds.min.y + (iy as f64 + 0.5) * dy,
                    bounds.min.z + (iz as f64 + 0.5) * dz,
                );
                let mut overlap: u32 = 0;
                for ctx in &per_pair {
                    if ctx.covers(&p, coc_threshold_px, illum_thickness_m) {
                        overlap += 1;
                    }
                }
                let idx = iz * resolution.nx * resolution.ny + iy * resolution.nx + ix;
                counts[idx] = overlap;
            }
        }
    }

    Ok(VoxelOverlap {
        bounds,
        resolution,
        n_pairs,
        counts,
    })
}

fn validate(
    bounds: &VoxelBox,
    resolution: &VoxelResolution,
    coc_threshold_px: f64,
    illum_thickness_m: f64,
) -> crate::Result<()> {
    if !(bounds.min.coords.iter().all(|c| c.is_finite())
        && bounds.max.coords.iter().all(|c| c.is_finite()))
    {
        return Err(crate::Error::InvalidInput {
            reason: format!("voxel bounding box must be finite, got {bounds:?}"),
        });
    }
    if bounds.max.x <= bounds.min.x || bounds.max.y <= bounds.min.y || bounds.max.z <= bounds.min.z
    {
        return Err(crate::Error::InvalidInput {
            reason: format!("voxel bounding box must have positive size, got {bounds:?}"),
        });
    }
    if resolution.nx == 0 || resolution.ny == 0 || resolution.nz == 0 {
        return Err(crate::Error::InvalidInput {
            reason: format!(
                "voxel resolution must be >= 1 on all axes, got {}x{}x{}",
                resolution.nx, resolution.ny, resolution.nz,
            ),
        });
    }
    if !(coc_threshold_px.is_finite() && coc_threshold_px > 0.0) {
        return Err(crate::Error::InvalidInput {
            reason: format!("CoC threshold must be finite and positive, got {coc_threshold_px}"),
        });
    }
    if !(illum_thickness_m.is_finite() && illum_thickness_m > 0.0) {
        return Err(crate::Error::InvalidInput {
            reason: format!(
                "illumination thickness must be finite and positive, got {illum_thickness_m}"
            ),
        });
    }
    Ok(())
}

/// Per-pair derived data: everything the inner loop reads, pre-computed
/// once so the hot path is just three predicate checks per voxel.
struct PairContext {
    camera_from_world: nalgebra::Isometry3<f64>,
    sensor_extent: (f64, f64),
    principal_point: (f64, f64),
    focal_px: f64,
    camera_model: vision_calibration_core::CameraModel,
    lens: ThickLens,
    laser_plane: LaserPlane,
    laser_length: f64,
    laser_half_angle: f64,
}

impl PairContext {
    fn covers(&self, p: &Point3<f64>, coc_threshold_px: f64, illum_thickness_m: f64) -> bool {
        // (1) Visibility: in front of the camera + pinhole projection inside
        // the sensor rectangle.
        let p_cam = self.camera_from_world * p;
        if !(p_cam.z.is_finite() && p_cam.z > 0.0) {
            return false;
        }
        let (cx, cy) = self.principal_point;
        let u = self.focal_px * p_cam.x / p_cam.z + cx;
        let v = self.focal_px * p_cam.y / p_cam.z + cy;
        let (sw, sh) = self.sensor_extent;
        if !(u.is_finite() && v.is_finite() && u >= 0.0 && u <= sw && v >= 0.0 && v <= sh) {
            return false;
        }
        // The full camera-model projection is also checked so distortion +
        // Scheimpflug-homography sensors agree with the pinhole gate (the
        // pinhole is the dominant filter; the full projection catches
        // distortion-induced rejections at the field's edge).
        if self.camera_model.project_point_c(&p_cam.coords).is_none() {
            return false;
        }
        // (2) Illumination: close to the fan plane and inside the fan's
        // angular + radial extent.
        if self.laser_plane.signed_distance(p).abs() > 0.5 * illum_thickness_m {
            return false;
        }
        // Check angular + radial extent: project p onto the fan plane (it's
        // already close to it) and test using the plane's (central, in_plane)
        // basis.
        let rel = p - self.laser_plane.origin();
        let u_fan = rel.dot(&self.laser_plane.central());
        let v_fan = rel.dot(&self.laser_plane.in_plane());
        let r = (u_fan * u_fan + v_fan * v_fan).sqrt();
        if u_fan <= 0.0 || r > self.laser_length {
            return false;
        }
        // Wedge: |atan2(v, u)| <= half_angle  ⇔  v² <= u² · tan²(half)
        // (with u > 0 already guaranteed). Tangent grows monotonically on
        // (0, π/2), and half_angle is constrained there at construction.
        let tan_h = self.laser_half_angle.tan();
        if v_fan * v_fan > u_fan * u_fan * tan_h * tan_h {
            return false;
        }
        // (3) Focus: CoC at this 3D point under the camera's lens.
        let Ok(coc_px) = self.lens.coc_diameter_px(&p_cam) else {
            return false;
        };
        coc_px.is_finite() && coc_px <= coc_threshold_px
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Scene;

    /// A small, easily-debugged voxel grid spanning the default-MVP scene's
    /// inspection volume: a cube ~30 cm on a side centred on the target.
    fn default_scene_voxel_box() -> VoxelBox {
        // Default-MVP target sits at world (0, 0, 0.30); the working volume
        // is a small patch in front of it. Use a 0.30 m cube centred there.
        VoxelBox {
            min: Point3::new(-0.15, -0.15, 0.15),
            max: Point3::new(0.15, 0.15, 0.45),
        }
    }

    #[test]
    fn rejects_invalid_inputs() {
        let scene = Scene::default_mvp();
        let res = VoxelResolution {
            nx: 4,
            ny: 4,
            nz: 4,
        };
        // Inverted box.
        let bad_box = VoxelBox {
            min: Point3::new(1.0, 0.0, 0.0),
            max: Point3::new(0.0, 1.0, 1.0),
        };
        assert!(voxelized_overlap(&scene, bad_box, res, 1.0, 1e-3).is_err());
        // Zero resolution.
        let bad_res = VoxelResolution {
            nx: 0,
            ny: 4,
            nz: 4,
        };
        assert!(voxelized_overlap(&scene, default_scene_voxel_box(), bad_res, 1.0, 1e-3).is_err());
        // Non-positive CoC threshold.
        assert!(voxelized_overlap(&scene, default_scene_voxel_box(), res, 0.0, 1e-3,).is_err());
        // Non-positive illumination thickness.
        assert!(voxelized_overlap(&scene, default_scene_voxel_box(), res, 1.0, -1.0,).is_err());
    }

    #[test]
    fn dimensions_and_voxel_count_match_resolution() {
        let scene = Scene::default_mvp();
        let res = VoxelResolution {
            nx: 8,
            ny: 6,
            nz: 4,
        };
        let v = voxelized_overlap(&scene, default_scene_voxel_box(), res, 1.0, 5e-3).unwrap();
        assert_eq!(v.resolution(), &res);
        assert_eq!(v.voxel_count(), 8 * 6 * 4);
        assert_eq!(v.n_pairs(), 1);
    }

    #[test]
    fn default_scene_has_some_covered_voxels() {
        // The default MVP scene's single pair must cover at least one voxel
        // of a box centred on the target. Otherwise the working-volume
        // analysis would also show empty, which other tests already
        // disprove.
        let scene = Scene::default_mvp();
        let res = VoxelResolution {
            nx: 16,
            ny: 16,
            nz: 16,
        };
        // Use a thicker illumination band so a 16³ grid catches the
        // (zero-thickness) fan plane reliably.
        let v = voxelized_overlap(&scene, default_scene_voxel_box(), res, 1.0, 2e-2).unwrap();
        assert!(
            v.count_at_least(1) > 0,
            "default-scene rig must cover at least one voxel"
        );
        assert_eq!(v.max_overlap(), 1, "single pair → max overlap is 1");
    }

    #[test]
    fn ring_increases_overlap_count() {
        // A 6-pair triangulation ring around the target should cover some
        // voxels with overlap > 1 (the central region viewed by multiple
        // pairs simultaneously). max_overlap should rise above the
        // single-pair case for at least some voxels near the target.
        let scene = Scene::default_mvp();
        let centre = scene.targets[0].pose * Point3::origin();
        let (cams, lasers) = Scene::triangulation_ring(
            6,
            nalgebra::Vector3::z(),
            centre,
            &scene.cameras[0],
            &scene.lasers[0],
        )
        .unwrap();
        let mut ring_scene = scene.clone();
        ring_scene.cameras = cams;
        ring_scene.lasers = lasers;
        let res = VoxelResolution {
            nx: 16,
            ny: 16,
            nz: 16,
        };
        let v = voxelized_overlap(&ring_scene, default_scene_voxel_box(), res, 2.0, 2e-2).unwrap();
        assert_eq!(v.n_pairs(), 6);
        assert!(
            v.max_overlap() >= 2,
            "a 6-pair ring should overlap on at least one voxel, max = {}",
            v.max_overlap()
        );
    }

    #[test]
    fn voxel_centre_is_at_the_grid_position() {
        let scene = Scene::default_mvp();
        let bounds = VoxelBox {
            min: Point3::new(0.0, 0.0, 0.0),
            max: Point3::new(1.0, 1.0, 1.0),
        };
        let res = VoxelResolution {
            nx: 4,
            ny: 4,
            nz: 4,
        };
        let v = voxelized_overlap(&scene, bounds, res, 1.0, 1e-3).unwrap();
        // First voxel (0,0,0): centre at (0.125, 0.125, 0.125).
        let c = v.voxel_centre(0, 0, 0);
        assert!((c.x - 0.125).abs() < 1e-12);
        assert!((c.y - 0.125).abs() < 1e-12);
        assert!((c.z - 0.125).abs() < 1e-12);
        // Last voxel (3,3,3): centre at (0.875, 0.875, 0.875).
        let c = v.voxel_centre(3, 3, 3);
        assert!((c.x - 0.875).abs() < 1e-12);
    }
}
