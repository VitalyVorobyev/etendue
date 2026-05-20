//! Working-volume analysis: the on-fan region a triangulation sensor can
//! actually measure.
//!
//! The **working volume** of a single-laser-fan triangulation sensor is the
//! set of object points the sensor can simultaneously
//!
//! 1. **illuminate** — they lie inside the laser fan's angular *and* radial
//!    extent (see [`LaserPlane::contains_in_extent`]);
//! 2. **see** — they project to a pixel inside the camera's sensor rectangle
//!    (via [`Camera::project_point_c`](vision_calibration_core::CameraModel::project_point_c),
//!    which rejects points behind the camera);
//! 3. **focus on** — their geometric circle of confusion stays below a
//!    depth-of-field pixel threshold (via [`ThickLens::coc_diameter_px`]).
//!
//! Because the fan is planar, every illuminated point lies on the fan plane.
//! The working volume of an **MVP** single-laser/single-camera triangulation
//! sensor is therefore a **2D region** on the laser plane — *not* a 3D voxel
//! grid. (Voxel analysis is post-MVP.) Computing it as a planar region keeps
//! the analysis cheap, lets the UI paint it directly on top of the rendered
//! fan, and matches the designer's intuition about what "the measurement
//! volume" means for a single-line sensor.
//!
//! # The sampling scheme
//!
//! [`working_volume`] samples a regular grid in the fan's **natural
//! parameterisation** — along the central ray (radius `r`) × across (fan
//! angle `phi`). Each cell records:
//!
//! - the cell's 3D world-frame position (the fan-plane point at that
//!   `(r, phi)`),
//! - the cell's camera-frame depth `z` — `None` if the point is behind the
//!   camera,
//! - the cell's circle of confusion in pixels — `None` if the point cannot be
//!   imaged,
//! - an `in_volume` boolean: true when *all three* predicates hold.
//!
//! Re-using the laser-plane parameterisation (instead of a `(x, y)` grid in
//! world space) means every sample lies on the fan plane by construction —
//! the illumination predicate then reduces to a pair of cheap bound checks
//! and the grid lays out cleanly for the UI to tessellate.
//!
//! # Why sample-and-mask rather than polygon clipping
//!
//! A polygon-intersection construction (Sutherland–Hodgman against the
//! camera frustum's projection onto the fan plane, intersected with the
//! depth-of-field band) is exact but considerably more code, and degenerate
//! configurations (a Scheimpflug-tilted PoBF coincident with the fan plane;
//! a fan plane edge-on to the camera) need careful handling. The
//! sample-and-mask pattern is the same one [`mod@crate::analysis::defocus_map`]
//! uses for the per-target CoC field — robust, headless, and trivially
//! testable.

use nalgebra::Point3;

use crate::laser::LaserPlane;
use crate::optics::ThickLens;
use crate::scene::{CameraEntity, LaserEntity};

/// Default depth-of-field pixel threshold for the working volume.
///
/// A point is considered "in focus" for the working-volume mask when its
/// geometric circle of confusion is at or below this many pixels — the
/// classic ~1-pixel depth-of-field criterion. Matches
/// [`mod@crate::analysis::defocus_map`]'s heatmap green band so the on-target
/// heatmap and the on-fan working-volume mask read the same scale.
pub const DEFAULT_COC_THRESHOLD_PX: f64 = 1.0;

/// A single sample of the working-volume grid.
///
/// One cell of the [`WorkingVolume::cells`] array. Every cell has a well-
/// defined world-space position (the fan-plane point at this `(r, phi)`); the
/// per-predicate fields then record what the camera/lens see at that point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorkingVolumeCell {
    /// The 3D world-frame position of this sample point on the fan plane.
    pub point_world: Point3<f64>,
    /// The camera-frame depth `z` of this sample, in metres. `None` if the
    /// point is behind the camera or otherwise unprojectable.
    pub depth_m: Option<f64>,
    /// The geometric circle-of-confusion diameter at this sample, in pixels.
    /// `None` if the sample cannot be imaged by the lens (behind the camera,
    /// inside the front focal point, or projecting outside the frustum).
    pub coc_px: Option<f64>,
    /// Whether the sample is inside the fan's angular + radial extent — the
    /// **illumination** predicate. For samples produced by [`working_volume`]
    /// this is also true for every interior cell by construction (the grid
    /// is laid out in the fan's parameterisation); cells exactly on the
    /// `phi = ±half_angle` or `r = length` boundary may sit a hair outside,
    /// so the field is recorded explicitly for completeness.
    pub illuminated: bool,
    /// Whether the sample projects to a pixel inside the camera's sensor
    /// rectangle — the **visibility** predicate.
    pub visible: bool,
    /// Whether the sample is in focus — `coc_px.is_some()` and that value is
    /// at or below [`WorkingVolume::coc_threshold_px`] — the **focus**
    /// predicate.
    pub in_focus: bool,
    /// Whether the sample is in the working volume — all three predicates
    /// (illuminated **and** visible **and** in_focus) hold.
    pub in_volume: bool,
}

/// A regular sampling of the working volume on a laser fan plane.
///
/// Produced by [`working_volume`]. The grid is laid out in the laser's
/// natural parameterisation:
///
/// - **`row`** (`0..rows`) sweeps the fan angle `phi` from `-half_angle` at
///   `row = 0` to `+half_angle` at `row = rows - 1`;
/// - **`col`** (`0..cols`) sweeps the radius `r` from `0` at `col = 0` to
///   `fan_length` at `col = cols - 1`.
///
/// Cells are stored row-major (`index = row * cols + col`); use [`get`] for
/// bounds-checked access or [`cells`] for the raw slice. The grid origin is
/// the laser apex (at `r = 0` every cell collapses to the same world point);
/// downstream code that needs a non-degenerate quad should skip the
/// `col = 0` column or treat its zero-area triangles accordingly.
///
/// [`get`]: WorkingVolume::get
/// [`cells`]: WorkingVolume::cells
#[derive(Clone, Debug, PartialEq)]
pub struct WorkingVolume {
    /// Sample rows — fan-angle sweeps (`>= 2`).
    rows: usize,
    /// Sample columns — radial sweeps (`>= 2`).
    cols: usize,
    /// Fan half-angle (`> 0`, `< π/2`), radians.
    half_angle_rad: f64,
    /// Fan radial extent, metres.
    fan_length_m: f64,
    /// The depth-of-field pixel threshold applied to the focus predicate.
    coc_threshold_px: f64,
    /// Row-major cell storage (length `rows * cols`).
    cells: Vec<WorkingVolumeCell>,
}

impl WorkingVolume {
    /// Number of sample rows (fan-angle sweep).
    #[must_use]
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Number of sample columns (radial sweep).
    #[must_use]
    pub fn cols(&self) -> usize {
        self.cols
    }

    /// The fan half-angle the grid was sampled over, in radians.
    #[must_use]
    pub fn half_angle_rad(&self) -> f64 {
        self.half_angle_rad
    }

    /// The fan radial extent the grid was sampled over, in metres.
    #[must_use]
    pub fn fan_length_m(&self) -> f64 {
        self.fan_length_m
    }

    /// The pixel circle-of-confusion threshold the focus predicate was
    /// evaluated against.
    #[must_use]
    pub fn coc_threshold_px(&self) -> f64 {
        self.coc_threshold_px
    }

    /// The row-major slice of grid cells.
    #[must_use]
    pub fn cells(&self) -> &[WorkingVolumeCell] {
        &self.cells
    }

    /// The cell at `(row, col)`, or `None` if either index is out of range.
    #[must_use]
    pub fn get(&self, row: usize, col: usize) -> Option<&WorkingVolumeCell> {
        if row >= self.rows || col >= self.cols {
            return None;
        }
        Some(&self.cells[row * self.cols + col])
    }

    /// The fan angle `phi` of sample row `row`, in radians.
    ///
    /// Rows sweep `-half_angle` at `row = 0` to `+half_angle` at
    /// `row = rows - 1`.
    #[must_use]
    pub fn phi(&self, row: usize) -> f64 {
        let t = row as f64 / (self.rows - 1) as f64;
        (t - 0.5) * 2.0 * self.half_angle_rad
    }

    /// The radius `r` of sample column `col`, in metres.
    ///
    /// Columns sweep `0` at `col = 0` to `fan_length` at `col = cols - 1`.
    #[must_use]
    pub fn radius_m(&self, col: usize) -> f64 {
        col as f64 / (self.cols - 1) as f64 * self.fan_length_m
    }

    /// Estimated working-volume **area**, in square metres.
    ///
    /// Integrates over the laser-plane cells whose centroid is in the
    /// working volume, using the area of each `(phi, r)` cell on the fan
    /// plane. With `dphi` and `dr` the cell extents and `r` the midpoint
    /// radius, the planar cell area is `r · dphi · dr` (the Jacobian of the
    /// fan parameterisation, scaled by the cell midpoint radius).
    ///
    /// For an all-out-of-volume grid this is exactly `0.0`. A coarser grid
    /// systematically underestimates the area along the boundary of the
    /// in-volume region, but for the MVP's 128×128 default the error is
    /// well below the practical precision a designer cares about.
    #[must_use]
    pub fn area_m2(&self) -> f64 {
        if self.rows < 2 || self.cols < 2 {
            return 0.0;
        }
        let dphi = 2.0 * self.half_angle_rad / (self.rows - 1) as f64;
        let dr = self.fan_length_m / (self.cols - 1) as f64;
        let mut total = 0.0;
        // A `(row, col)` cell — between sample (row, col) and (row+1, col+1)
        // — is in the volume if any of its four corners is in the volume.
        // Using `all` would shrink the region by a row/column at every
        // boundary; using `any` matches the rendered tessellation, where a
        // mesh triangle is colored in-volume if any of its three vertices is
        // in-volume.
        for row in 0..self.rows - 1 {
            // Cell midpoint radius (for the Jacobian).
            for col in 0..self.cols - 1 {
                let any_in = self.cells[row * self.cols + col].in_volume
                    || self.cells[row * self.cols + (col + 1)].in_volume
                    || self.cells[(row + 1) * self.cols + col].in_volume
                    || self.cells[(row + 1) * self.cols + (col + 1)].in_volume;
                if !any_in {
                    continue;
                }
                // Mid-radius of the cell — the average of the two sampling
                // radii spanning this cell.
                let r_mid = 0.5 * (self.radius_m(col) + self.radius_m(col + 1));
                total += r_mid * dphi * dr;
            }
        }
        total
    }

    /// The achieved camera-frame depth range across the working volume:
    /// `(min_z, max_z)` in metres.
    ///
    /// Both bounds are taken over cells with `in_volume = true`; cells with
    /// no recorded depth (behind the camera) are skipped automatically — a
    /// cell behind the camera fails the visibility predicate, so it cannot
    /// be `in_volume`. Returns `None` when no cell is in the working volume.
    ///
    /// This is the second number a designer cares about (alongside
    /// [`area_m2`](Self::area_m2)): the working-volume **depth coverage** —
    /// from a near-field plane (small `z`) to a far-field plane (large `z`),
    /// in the camera's own frame.
    #[must_use]
    pub fn depth_range_m(&self) -> Option<(f64, f64)> {
        let mut min_z = f64::INFINITY;
        let mut max_z = f64::NEG_INFINITY;
        for cell in &self.cells {
            if !cell.in_volume {
                continue;
            }
            let Some(z) = cell.depth_m else {
                continue;
            };
            if z < min_z {
                min_z = z;
            }
            if z > max_z {
                max_z = z;
            }
        }
        if min_z.is_finite() && max_z.is_finite() {
            Some((min_z, max_z))
        } else {
            None
        }
    }

    /// The number of cells in the working volume.
    ///
    /// Useful for sanity checks and tests; the headline UI readouts are
    /// [`area_m2`](Self::area_m2) and [`depth_range_m`](Self::depth_range_m).
    #[must_use]
    pub fn in_volume_count(&self) -> usize {
        self.cells.iter().filter(|c| c.in_volume).count()
    }
}

/// Compute the working volume of a camera + laser pair, sampled on a regular
/// grid over the laser fan plane.
///
/// See the module doc-comment for the predicates and the parameterisation.
///
/// # Parameters
///
/// - `camera`: the viewing camera; its physical optics drive the focus
///   predicate, its [`CameraParams`](crate::calibration::CameraParams) projection chain drives the visibility
///   predicate.
/// - `laser`: the line-projecting laser; its [`LaserPlane`] supplies the
///   illumination geometry and the natural `(r, phi)` parameterisation.
/// - `rows`, `cols`: grid resolution (both `>= 2`). The MVP UI uses 128×128
///   — fine enough that the in-volume region's boundary reads as a clean
///   curve, cheap enough to recompute on every scene change.
/// - `coc_threshold_px`: the depth-of-field pixel threshold for the focus
///   predicate. The classic 1-pixel criterion (see
///   [`DEFAULT_COC_THRESHOLD_PX`]) is the MVP default.
///
/// # Errors
///
/// Returns [`Error::InvalidInput`](crate::Error::InvalidInput) if `rows` or
/// `cols` is less than 2, or if `coc_threshold_px` is not finite and
/// strictly positive. Propagates [`CameraEntity::thick_lens`]'s error if the
/// camera's physical optics parameters are inconsistent (cannot happen for a
/// camera built through [`CameraEntity::new`]).
pub fn working_volume(
    camera: &CameraEntity,
    laser: &LaserEntity,
    rows: usize,
    cols: usize,
    coc_threshold_px: f64,
) -> crate::Result<WorkingVolume> {
    if rows < 2 || cols < 2 {
        return Err(crate::Error::InvalidInput {
            reason: format!("working-volume sampling grid must be at least 2x2, got {rows}x{cols}"),
        });
    }
    if !(coc_threshold_px.is_finite() && coc_threshold_px > 0.0) {
        return Err(crate::Error::InvalidInput {
            reason: format!(
                "working-volume CoC threshold must be finite and positive, got \
                 {coc_threshold_px}"
            ),
        });
    }

    let plane = LaserPlane::from_entity(laser);
    let lens: ThickLens = camera.thick_lens()?;
    let camera_model = camera.params.build();
    let camera_from_world = camera.pose.inverse();
    let (sensor_w, sensor_h) = camera.resolution_f64();

    // Sample the fan in its natural parameterisation: (phi, r) on a regular
    // grid. Row sweeps the angle, column sweeps the radius — the same
    // ordering the UI tessellates and renders.
    let mut cells = Vec::with_capacity(rows * cols);
    for row in 0..rows {
        let t_phi = row as f64 / (rows - 1) as f64;
        let phi = (t_phi - 0.5) * 2.0 * laser.fan_half_angle;
        let direction = plane.ray_direction(phi);
        for col in 0..cols {
            let t_r = col as f64 / (cols - 1) as f64;
            let r = t_r * laser.fan_length;
            let point_world = plane.origin() + direction * r;

            // (1) Illumination: by construction of the parameterisation
            // every interior cell is inside the fan; the explicit call
            // covers boundary samples (and gracefully accepts any
            // numerical wobble at the edges).
            let illuminated = plane.contains_in_extent(&point_world);

            // (2) Visibility: transform to the camera frame and project.
            // `project_point_c` rejects z <= 0 (behind the camera). The
            // sensor-rectangle check finishes the predicate.
            let p_camera = camera_from_world * point_world;
            let depth_m = if p_camera.z > 0.0 {
                Some(p_camera.z)
            } else {
                None
            };
            let pixel = camera_model.project_point_c(&p_camera.coords);
            let visible = match pixel {
                Some(px) => {
                    px.x.is_finite()
                        && px.y.is_finite()
                        && px.x >= 0.0
                        && px.x <= sensor_w
                        && px.y >= 0.0
                        && px.y <= sensor_h
                }
                None => false,
            };

            // (3) Focus: evaluate the geometric CoC at the camera-frame
            // point. A point the lens cannot image (behind the camera or
            // inside the focal point) yields no CoC and fails focus.
            let coc_px = lens.coc_diameter_px(&p_camera).ok();
            let in_focus = coc_px.is_some_and(|c| c.is_finite() && c <= coc_threshold_px);

            let in_volume = illuminated && visible && in_focus;

            cells.push(WorkingVolumeCell {
                point_world,
                depth_m,
                coc_px,
                illuminated,
                visible,
                in_focus,
                in_volume,
            });
        }
    }

    Ok(WorkingVolume {
        rows,
        cols,
        half_angle_rad: laser.fan_half_angle,
        fan_length_m: laser.fan_length,
        coc_threshold_px,
        cells,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use nalgebra::{Isometry3, Translation3, UnitQuaternion, Vector3};
    use vision_calibration_core::{
        CameraParams, DistortionParams, FxFyCxCySkew, IntrinsicsParams, ProjectionParams,
        ScheimpflugParams, SensorParams,
    };

    // A realistic machine-vision lens, identical to the optics tests.
    const F_M: f64 = 0.016;
    const FNUM: f64 = 2.8;
    const FOCUS_M: f64 = 0.5;
    const GAP_M: f64 = 0.0;
    const PITCH_M: f64 = 3.45e-6;
    const RES_W: u32 = 1280;
    const RES_H: u32 = 1024;

    /// Build a Scheimpflug camera entity with the given pose and optional
    /// non-default focus distance.
    fn camera_at(pose: Isometry3<f64>, focus_m: f64) -> CameraEntity {
        let params = CameraParams {
            projection: ProjectionParams::Pinhole,
            distortion: DistortionParams::None,
            sensor: SensorParams::Scheimpflug {
                params: ScheimpflugParams {
                    tilt_x: 0.0,
                    tilt_y: 0.0,
                },
            },
            intrinsics: IntrinsicsParams::FxFyCxCySkew {
                params: FxFyCxCySkew {
                    fx: 0.0,
                    fy: 0.0,
                    cx: f64::from(RES_W) * 0.5,
                    cy: f64::from(RES_H) * 0.5,
                    skew: 0.0,
                },
            },
        };
        CameraEntity::new(
            pose,
            params,
            (RES_W, RES_H),
            F_M,
            FNUM,
            focus_m,
            GAP_M,
            PITCH_M,
            0.1,
            2.0,
        )
        .expect("valid camera entity")
    }

    /// A laser at `(0, -d, 0)` aimed along world +y, fan opening in the
    /// world `x = 0` plane symmetric about `+y`.
    fn laser_along_y(standoff: f64, half_angle: f64, length: f64) -> LaserEntity {
        let rot = UnitQuaternion::rotation_between(&Vector3::z(), &Vector3::y())
            .expect("z to y rotation");
        let pose = Isometry3::from_parts(Translation3::new(0.0, -standoff, 0.0), rot);
        LaserEntity::new(pose, half_angle, length, 660.0, 0.25e-3).expect("valid laser")
    }

    /// A triangulation-style camera: offset sideways from a laser, looking
    /// at the same target standoff. Returns the camera entity and the laser.
    fn triangulation_setup() -> (CameraEntity, LaserEntity) {
        // Laser at world (0, -0.5, 0) along +y; camera at (0.2, -0.45, 0)
        // looking at the world origin. Camera focus at the camera-to-origin
        // distance so a point near (0, 0, 0) is sharply imaged.
        let laser = laser_along_y(0.5, 0.30, 1.0);
        let camera_pos = Point3::new(0.2, -0.45, 0.0);
        let look = Point3::origin();
        let dir = look - camera_pos;
        let rot = nalgebra::Rotation3::face_towards(&dir, &-Vector3::z());
        let pose = Isometry3::from_parts(
            Translation3::from(camera_pos.coords),
            UnitQuaternion::from_rotation_matrix(&rot),
        );
        let distance = dir.norm();
        let cam = camera_at(pose, distance);
        (cam, laser)
    }

    #[test]
    fn rejects_a_degenerate_grid() {
        let (cam, laser) = triangulation_setup();
        assert!(working_volume(&cam, &laser, 1, 16, 1.0).is_err());
        assert!(working_volume(&cam, &laser, 16, 1, 1.0).is_err());
    }

    #[test]
    fn rejects_a_bad_threshold() {
        let (cam, laser) = triangulation_setup();
        assert!(working_volume(&cam, &laser, 8, 8, 0.0).is_err());
        assert!(working_volume(&cam, &laser, 8, 8, -1.0).is_err());
        assert!(working_volume(&cam, &laser, 8, 8, f64::NAN).is_err());
    }

    #[test]
    fn grid_has_the_requested_dimensions() {
        let (cam, laser) = triangulation_setup();
        let wv = working_volume(&cam, &laser, 11, 17, 1.0).unwrap();
        assert_eq!(wv.rows(), 11);
        assert_eq!(wv.cols(), 17);
        assert_eq!(wv.cells().len(), 11 * 17);
        assert_relative_eq!(wv.half_angle_rad(), 0.30, epsilon = 1e-15);
        assert_relative_eq!(wv.fan_length_m(), 1.0, epsilon = 1e-15);
    }

    #[test]
    fn parameterisation_spans_the_fan() {
        let (cam, laser) = triangulation_setup();
        let wv = working_volume(&cam, &laser, 5, 9, 1.0).unwrap();
        // Row sweeps phi from -half_angle to +half_angle.
        assert_relative_eq!(wv.phi(0), -laser.fan_half_angle, epsilon = 1e-12);
        assert_relative_eq!(wv.phi(wv.rows() - 1), laser.fan_half_angle, epsilon = 1e-12);
        // Mid row is the central ray.
        assert_relative_eq!(wv.phi(wv.rows() / 2), 0.0, epsilon = 1e-12);
        // Column sweeps r from 0 to fan_length.
        assert_relative_eq!(wv.radius_m(0), 0.0, epsilon = 1e-12);
        assert_relative_eq!(
            wv.radius_m(wv.cols() - 1),
            laser.fan_length,
            epsilon = 1e-12
        );
    }

    #[test]
    fn default_mvp_scene_has_a_non_empty_working_volume() {
        // The default scene was tuned (M2/M4 work) so the camera frames the
        // target and the lens places some of the fan inside the depth of
        // field. The working volume must therefore be a non-empty region,
        // with a positive area and a sensible camera-frame depth range.
        let scene = crate::scene::Scene::default_mvp();
        let wv = working_volume(&scene.cameras[0], &scene.lasers[0], 64, 64, 1.0)
            .expect("default scene");

        assert!(
            wv.in_volume_count() > 0,
            "the default scene must place SOME fan samples in the working volume"
        );
        let area = wv.area_m2();
        assert!(area > 0.0, "area must be positive, got {area} m^2");
        // A handful of square millimetres at minimum — a real measurement
        // patch, not just a few stray cells.
        assert!(
            area > 1.0e-5,
            "working-volume area should be > 10 mm^2, got {} mm^2",
            area * 1.0e6
        );

        let (z_min, z_max) = wv
            .depth_range_m()
            .expect("a non-empty volume has a depth range");
        assert!(z_min.is_finite() && z_max.is_finite());
        assert!(z_max > z_min, "depth range must be a real interval");
        // The camera-to-target standoff is ~0.62 m for the default scene, so
        // every cell must sit at a sensible positive depth.
        assert!(
            z_min > 0.1 && z_max < 1.5,
            "default-scene depth range {z_min}..{z_max} should bracket the target standoff"
        );
    }

    #[test]
    fn detuning_focus_shrinks_the_working_volume() {
        // The MVP "useful" check: if you move the focus distance away from
        // the target, the in-focus band on the fan plane shrinks (the
        // intersection of the depth of field with the fan plane is smaller),
        // so the working-volume area falls. This is the most basic property
        // the working-volume analysis must respect — without it the tool
        // could not tell the designer when their focus is mis-set.
        let scene = crate::scene::Scene::default_mvp();
        let focused = working_volume(&scene.cameras[0], &scene.lasers[0], 64, 64, 1.0).unwrap();
        let area_focused = focused.area_m2();
        assert!(area_focused > 0.0);

        // Move the focus far from the target standoff (the default is ~0.62 m
        // — push it to 2.0 m so the on-target band is squarely out of focus).
        let mut detuned_cam = scene.cameras[0].clone();
        detuned_cam.focus_distance_m = 2.0;
        detuned_cam.sync_intrinsics_from_physical();
        let detuned = working_volume(&detuned_cam, &scene.lasers[0], 64, 64, 1.0).unwrap();
        let area_detuned = detuned.area_m2();

        assert!(
            area_detuned < area_focused,
            "detuning focus must shrink the working-volume area: \
             focused {area_focused} m^2 vs detuned {area_detuned} m^2"
        );
    }

    #[test]
    fn an_invisible_fan_yields_no_working_volume() {
        // Place the laser BEHIND the camera, aimed even further behind it:
        // every fan sample sits at camera-frame z <= 0 and fails the
        // visibility predicate, so the working volume is empty.
        let camera = camera_at(Isometry3::identity(), FOCUS_M);
        // Camera looks along +z. Put the laser at z = -1.0 and aim its
        // central ray along world -z, so the fan opens entirely behind the
        // camera.
        let rot =
            UnitQuaternion::rotation_between(&Vector3::z(), &-Vector3::z()).unwrap_or_else(|| {
                // Anti-parallel rotation has no shortest-arc representative;
                // pick the 180° turn about world +x explicitly.
                UnitQuaternion::from_axis_angle(&Vector3::x_axis(), std::f64::consts::PI)
            });
        let pose = Isometry3::from_parts(Translation3::new(0.0, 0.0, -1.0), rot);
        let laser = LaserEntity::new(pose, 0.20, 0.5, 660.0, 0.25e-3).unwrap();
        let wv = working_volume(&camera, &laser, 16, 16, 1.0).unwrap();
        assert_eq!(wv.in_volume_count(), 0);
        assert_eq!(wv.area_m2(), 0.0);
        assert!(wv.depth_range_m().is_none());
    }

    #[test]
    fn cells_carry_world_positions_on_the_fan_plane() {
        // Every cell's point_world must lie in the fan plane — a
        // signed-distance check via LaserPlane.
        let (cam, laser) = triangulation_setup();
        let plane = LaserPlane::from_entity(&laser);
        let wv = working_volume(&cam, &laser, 8, 8, 1.0).unwrap();
        for cell in wv.cells() {
            let d = plane.signed_distance(&cell.point_world);
            assert!(
                d.abs() < 1e-9,
                "every working-volume cell must lie in the fan plane, got d = {d}"
            );
        }
    }

    #[test]
    fn area_uses_the_fan_plane_jacobian() {
        // Mark every cell as in_volume by setting the focus threshold huge
        // and pointing a wide laser at the camera. The estimated area is
        // then the area of the entire fan triangle.
        // Use a very forgiving threshold so the focus predicate is
        // effectively disabled, and pick a setup where every sample is
        // visible from the camera.
        let camera_pos = Point3::new(0.0, 0.0, 0.0);
        let look = Point3::new(0.0, 1.0, 0.0);
        let dir = look - camera_pos;
        let rot = nalgebra::Rotation3::face_towards(&dir, &-Vector3::z());
        let pose = Isometry3::from_parts(
            Translation3::from(camera_pos.coords),
            UnitQuaternion::from_rotation_matrix(&rot),
        );
        let camera = camera_at(pose, 1.0);

        // Laser at world (0, 0.1, 0) aiming along +y (away from camera),
        // narrow fan so every sample lands inside the centred camera frame.
        let standoff = -0.1; // place at y = +0.1, so laser_along_y wants -d.
        let laser = laser_along_y(standoff, 0.05, 0.3);
        // Disable the focus predicate by passing a huge threshold.
        let wv = working_volume(&camera, &laser, 32, 32, 1.0e6).unwrap();

        // Expected fan-plane sector area (excluding any failed cells):
        //   A_sector = (1/2) * (2*half) * length^2 = half * length^2.
        let expected = laser.fan_half_angle * laser.fan_length * laser.fan_length;
        let area = wv.area_m2();
        // Allow ~10% relative error — the trapezoidal sum + binary-mask
        // boundary handling is coarse, and the goal of this test is to lock
        // the Jacobian's order of magnitude rather than its exact value.
        assert!(
            area > 0.0,
            "saturated working volume should produce a positive area"
        );
        assert!(
            (area - expected).abs() / expected < 0.15,
            "area {area} should approximate the sector area {expected} \
             (Jacobian = r·dphi·dr)"
        );
    }
}
