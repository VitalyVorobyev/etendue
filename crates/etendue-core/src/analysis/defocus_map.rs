//! Per-target defocus map: the circle of confusion sampled over a target.
//!
//! [`defocus_map`] takes a [`CameraEntity`] and a planar [`TargetEntity`] and
//! returns a [`DefocusMap`] — a regular grid of geometric circle-of-confusion
//! values, in pixels, sampled across the target's surface. It is the headless
//! M4a deliverable that M4b renders as a focus heatmap on the target quad.
//!
//! # What is sampled
//!
//! The target is a finite rectangle in its local `z = 0` plane (see
//! [`TargetEntity`]). The map samples it on a `cols × rows` lattice of points,
//! evenly spaced in the target's local `(x, y)`, spanning the *full* rectangle
//! from edge to edge. For each sample point:
//!
//! 1. the point is transformed target-local → world → camera-local;
//! 2. its geometric circle of confusion, in pixels, is evaluated with the
//!    camera's M4 [`ThickLens`] model
//!    ([`ThickLens::coc_diameter_px`](crate::optics::ThickLens::coc_diameter_px)).
//!
//! A sample whose point cannot be imaged — behind the camera, or at/inside the
//! lens's front focal point — is recorded as `None` rather than a number.
//!
//! Because the CoC model is an ideal-lens geometric model, with an untilted
//! sensor the map's values depend only on each point's depth (so the iso-CoC
//! contours are depth iso-lines); under Scheimpflug tilt the in-focus locus is
//! a tilted plane and the zero-CoC band rotates across the target. See
//! [`crate::optics::coc`].

use nalgebra::Point3;

use crate::optics::ThickLens;
use crate::scene::{CameraEntity, TargetEntity};

/// A regular grid of circle-of-confusion samples over a target's surface.
///
/// The grid is `rows × cols`; [`DefocusMap::coc_px`] is stored row-major
/// (`index = row * cols + col`). Each entry is the geometric CoC **in pixels**
/// at that target sample point, or `None` if the point could not be imaged
/// (behind the camera, or inside the lens's focal point).
///
/// Sample `(row, col)` sits at target-local coordinates
/// `(local_x(col), local_y(row), 0)` — see [`DefocusMap::local_x`] /
/// [`DefocusMap::local_y`] — which lets the renderer (M4b) place each value on
/// the target quad without re-deriving the lattice.
#[derive(Clone, Debug, PartialEq)]
pub struct DefocusMap {
    /// Number of sample columns (along the target's local `x`). `>= 2`.
    cols: usize,
    /// Number of sample rows (along the target's local `y`). `>= 2`.
    rows: usize,
    /// Target rectangle width (local `x` extent), in metres — copied from the
    /// [`TargetEntity`] so the renderer can scale the grid.
    target_width: f64,
    /// Target rectangle height (local `y` extent), in metres.
    target_height: f64,
    /// Circle-of-confusion samples in pixels, row-major (`row * cols + col`).
    /// `None` where the sample point could not be imaged. Length is
    /// `rows * cols`.
    coc_px: Vec<Option<f64>>,
}

impl DefocusMap {
    /// Number of sample columns (along the target's local `x`).
    #[must_use]
    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Number of sample rows (along the target's local `y`).
    #[must_use]
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Target rectangle width (local `x` extent), in metres.
    #[must_use]
    pub fn target_width(&self) -> f64 {
        self.target_width
    }

    /// Target rectangle height (local `y` extent), in metres.
    #[must_use]
    pub fn target_height(&self) -> f64 {
        self.target_height
    }

    /// The row-major slice of circle-of-confusion samples, in pixels.
    ///
    /// Entry `row * cols + col` is the CoC at sample `(row, col)`, or `None`
    /// if that point could not be imaged.
    #[must_use]
    pub fn coc_px(&self) -> &[Option<f64>] {
        &self.coc_px
    }

    /// The circle-of-confusion sample at `(row, col)`, in pixels.
    ///
    /// Returns `None` both when `(row, col)` is out of range and when the
    /// sample point could not be imaged — callers that must distinguish the
    /// two should bounds-check against [`DefocusMap::rows`] /
    /// [`DefocusMap::cols`] first.
    #[must_use]
    pub fn get(&self, row: usize, col: usize) -> Option<f64> {
        if row >= self.rows || col >= self.cols {
            return None;
        }
        self.coc_px[row * self.cols + col]
    }

    /// The target-local `x` coordinate of sample column `col`, in metres.
    ///
    /// Columns span the full rectangle, from `-width/2` at `col = 0` to
    /// `+width/2` at `col = cols - 1`.
    #[must_use]
    pub fn local_x(&self, col: usize) -> f64 {
        let t = col as f64 / (self.cols - 1) as f64;
        (t - 0.5) * self.target_width
    }

    /// The target-local `y` coordinate of sample row `row`, in metres.
    ///
    /// Rows span the full rectangle, from `-height/2` at `row = 0` to
    /// `+height/2` at `row = rows - 1`.
    #[must_use]
    pub fn local_y(&self, row: usize) -> f64 {
        let t = row as f64 / (self.rows - 1) as f64;
        (t - 0.5) * self.target_height
    }

    /// The largest finite circle-of-confusion value in the map, in pixels, or
    /// `None` if every sample failed to image.
    ///
    /// Useful for choosing a colour-map range when rendering the heatmap.
    #[must_use]
    pub fn max_coc_px(&self) -> Option<f64> {
        self.coc_px
            .iter()
            .filter_map(|&c| c)
            .filter(|c| c.is_finite())
            .fold(None, |acc, c| Some(acc.map_or(c, |a: f64| a.max(c))))
    }
}

/// Compute the per-target defocus map for a camera viewing a planar target.
///
/// Samples the `target` rectangle on a `cols × rows` lattice and evaluates the
/// geometric circle of confusion, in pixels, at each sample using the
/// `camera`'s M4 thick-lens model. See the module doc-comment for the sampling
/// scheme.
///
/// The `camera`'s [`ThickLens`] is built once and reused for every sample. A
/// sample point that the lens cannot image (behind the camera, or at/inside
/// the front focal point) is recorded as `None`; this never aborts the map.
///
/// # Parameters
///
/// - `camera`: the viewing camera; its physical optics parameters drive the
///   defocus model.
/// - `target`: the planar target whose surface is sampled.
/// - `cols`, `rows`: the sample-lattice resolution. Both must be `>= 2` so the
///   lattice spans the rectangle edge to edge.
///
/// # Errors
///
/// Returns [`Error::InvalidInput`](crate::Error::InvalidInput) if `cols` or
/// `rows` is less than 2. Propagates [`CameraEntity::thick_lens`]'s error if
/// the camera's physical optics parameters are inconsistent (cannot happen for
/// a camera built through [`CameraEntity::new`]).
pub fn defocus_map(
    camera: &CameraEntity,
    target: &TargetEntity,
    cols: usize,
    rows: usize,
) -> crate::Result<DefocusMap> {
    if cols < 2 || rows < 2 {
        return Err(crate::Error::InvalidInput {
            reason: format!("defocus-map sampling grid must be at least 2x2, got {cols}x{rows}"),
        });
    }

    let lens: ThickLens = camera.thick_lens()?;

    // Transform chain: target-local -> world -> camera-local.  The CoC model
    // takes a camera-local point, so we compose both isometries once.
    let world_from_target = target.pose;
    let camera_from_world = camera.pose.inverse();
    let camera_from_target = camera_from_world * world_from_target;

    let mut coc_px = Vec::with_capacity(rows * cols);
    for row in 0..rows {
        // Local y spans the full height, edge to edge.
        let ty = row as f64 / (rows - 1) as f64;
        let local_y = (ty - 0.5) * target.height;
        for col in 0..cols {
            let tx = col as f64 / (cols - 1) as f64;
            let local_x = (tx - 0.5) * target.width;

            // Target sample point, in the camera frame.
            let p_target = Point3::new(local_x, local_y, 0.0);
            let p_camera = camera_from_target * p_target;

            // A point the lens cannot image (behind the camera, inside the
            // focal point) yields no CoC — record None, do not abort.
            let value = lens.coc_diameter_px(&p_camera).ok();
            coc_px.push(value);
        }
    }

    Ok(DefocusMap {
        cols,
        rows,
        target_width: target.width,
        target_height: target.height,
        coc_px,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use nalgebra::{Isometry3, Translation3, UnitQuaternion};
    use vision_calibration_core::{
        CameraParams, DistortionParams, FxFyCxCySkew, IntrinsicsParams, ProjectionParams,
        ScheimpflugParams, SensorParams,
    };

    // The realistic machine-vision lens used throughout the optics tests:
    //   f = 25 mm, N = 4, s_o = 0.30 m, pixel pitch p = 3.45 um.
    const F_M: f64 = 0.025;
    const FNUM: f64 = 4.0;
    const FOCUS_M: f64 = 0.30;
    const GAP_M: f64 = 0.0;
    const PITCH_M: f64 = 3.45e-6;

    /// A camera at the world origin looking down `+z`, with the given sensor
    /// tilt and the standard test lens.
    fn camera(tau_x: f64, tau_y: f64) -> CameraEntity {
        let params = CameraParams {
            projection: ProjectionParams::Pinhole,
            distortion: DistortionParams::None,
            sensor: SensorParams::Scheimpflug {
                params: ScheimpflugParams {
                    tilt_x: tau_x,
                    tilt_y: tau_y,
                },
            },
            intrinsics: IntrinsicsParams::FxFyCxCySkew {
                params: FxFyCxCySkew {
                    fx: 0.0,
                    fy: 0.0,
                    cx: 640.0,
                    cy: 512.0,
                    skew: 0.0,
                },
            },
        };
        CameraEntity::new(
            Isometry3::identity(),
            params,
            (1280, 1024),
            F_M,
            FNUM,
            FOCUS_M,
            GAP_M,
            PITCH_M,
            0.05,
            2.0,
        )
        .expect("valid camera")
    }

    /// A fronto-parallel target of the given size, centred at camera-frame
    /// depth `z` (its local `z = 0` plane lies at world/camera `z = depth`,
    /// normal along `+z`).
    fn frontoparallel_target(width: f64, height: f64, depth: f64) -> TargetEntity {
        let pose = Isometry3::from_parts(
            Translation3::new(0.0, 0.0, depth),
            UnitQuaternion::identity(),
        );
        TargetEntity::new(pose, width, height).expect("valid target")
    }

    #[test]
    fn map_has_the_requested_dimensions() {
        let cam = camera(0.0, 0.0);
        let target = frontoparallel_target(0.1, 0.08, FOCUS_M);
        let map = defocus_map(&cam, &target, 9, 7).unwrap();
        assert_eq!(map.cols(), 9);
        assert_eq!(map.rows(), 7);
        assert_eq!(map.coc_px().len(), 63);
        assert_relative_eq!(map.target_width(), 0.1, epsilon = 1e-15);
        assert_relative_eq!(map.target_height(), 0.08, epsilon = 1e-15);
    }

    #[test]
    fn rejects_a_degenerate_grid() {
        let cam = camera(0.0, 0.0);
        let target = frontoparallel_target(0.1, 0.1, FOCUS_M);
        assert!(defocus_map(&cam, &target, 1, 5).is_err());
        assert!(defocus_map(&cam, &target, 5, 1).is_err());
    }

    #[test]
    fn target_on_the_focus_plane_is_everywhere_sharp() {
        // A fronto-parallel target placed exactly at the focus distance: every
        // sample sits on the (fronto-parallel) plane of best focus, so every
        // CoC is zero.
        let cam = camera(0.0, 0.0);
        let target = frontoparallel_target(0.12, 0.10, FOCUS_M);
        let map = defocus_map(&cam, &target, 11, 9).unwrap();
        for value in map.coc_px() {
            let c = value.expect("focus-plane sample must image");
            assert_relative_eq!(c, 0.0, epsilon = 1e-12);
        }
        assert_relative_eq!(map.max_coc_px().unwrap(), 0.0, epsilon = 1e-12);
    }

    #[test]
    fn untilted_sensor_gives_no_lateral_variation() {
        // A fronto-parallel target *off* the focus plane: it is uniformly
        // defocused.  With an untilted sensor and an ideal lens the CoC has no
        // lateral dependence, so every cell holds the SAME value.
        let cam = camera(0.0, 0.0);
        let depth = 0.33; // 30 mm beyond the 0.30 m focus
        let target = frontoparallel_target(0.12, 0.10, depth);
        let map = defocus_map(&cam, &target, 9, 9).unwrap();

        let first = map.get(0, 0).expect("sample must image");
        // The hand-computed CoC for this lens at z = 0.33 is 14.9719 px
        // (see optics/coc.rs regime (b)).
        assert_relative_eq!(first, 14.971_852_916_517_08, epsilon = 1e-9);
        for row in 0..map.rows() {
            for col in 0..map.cols() {
                let c = map.get(row, col).expect("sample must image");
                assert_relative_eq!(c, first, epsilon = 1e-12);
            }
        }
    }

    #[test]
    fn tilted_sensor_makes_the_map_vary_across_the_target() {
        // With a Scheimpflug-tilted sensor the in-focus locus is a tilted
        // plane.  A fronto-parallel target placed OFF the focus distance
        // crosses that tilted plane along one line, so the CoC must vary
        // across the target — and, because the crossing is off-centre, the
        // two opposite local-y edges differ.
        let tau_x = 5.0_f64.to_radians();
        let cam = camera(tau_x, 0.0);
        // Target at z = 0.34 m, 40 mm beyond the 0.30 m focus distance.
        let target = frontoparallel_target(0.10, 0.16, 0.34);
        let map = defocus_map(&cam, &target, 5, 9).unwrap();

        // Opposite local-y edges (rows 0 and last) have genuinely different
        // CoC — the tilted PoBF cuts the target asymmetrically.
        let top = map.get(0, 2).expect("sample must image");
        let bottom = map.get(map.rows() - 1, 2).expect("sample must image");
        assert!(
            (top - bottom).abs() > 1e-2,
            "a tilted sensor on an off-focus target must make the two local-y \
             edges differ; got top={top} px, bottom={bottom} px"
        );

        // Along a single row (fixed local y) the value is constant — a tau_x
        // tilt induces variation along the local y axis only, not along x.
        let r = map.rows() - 1;
        let c0 = map.get(r, 0).expect("sample must image");
        for col in 1..map.cols() {
            let c = map.get(r, col).expect("sample must image");
            assert_relative_eq!(c, c0, epsilon = 1e-9);
        }
    }

    #[test]
    fn tilted_sensor_keeps_one_band_of_the_target_sharper() {
        // A fronto-parallel target *at* the focus distance, tilted sensor:
        // the on-axis row (local y = 0) still lies on the tilted PoBF — that
        // band stays sharp — while the local-y edges leave focus and blur.
        let tau_x = 5.0_f64.to_radians();
        let cam = camera(tau_x, 0.0);
        let target = frontoparallel_target(0.10, 0.16, FOCUS_M);
        // Odd row count so a row lands exactly on local y = 0.
        let map = defocus_map(&cam, &target, 3, 9).unwrap();
        let mid_row = map.rows() / 2;
        assert_relative_eq!(map.local_y(mid_row), 0.0, epsilon = 1e-15);

        // The centre row is on the PoBF: sharp.
        let centre = map.get(mid_row, 1).expect("sample must image");
        assert_relative_eq!(centre, 0.0, epsilon = 1e-9);
        // An edge row is off the PoBF: blurred.
        let edge = map.get(0, 1).expect("sample must image");
        assert!(
            edge > 1.0,
            "the local-y edge of an at-focus target must blur under a tilted \
             sensor, got {edge} px"
        );
    }

    #[test]
    fn local_coordinate_helpers_span_the_rectangle() {
        let cam = camera(0.0, 0.0);
        let target = frontoparallel_target(0.20, 0.12, FOCUS_M);
        let map = defocus_map(&cam, &target, 5, 4).unwrap();
        // Columns span [-w/2, +w/2].
        assert_relative_eq!(map.local_x(0), -0.10, epsilon = 1e-15);
        assert_relative_eq!(map.local_x(4), 0.10, epsilon = 1e-15);
        assert_relative_eq!(map.local_x(2), 0.0, epsilon = 1e-15);
        // Rows span [-h/2, +h/2].
        assert_relative_eq!(map.local_y(0), -0.06, epsilon = 1e-15);
        assert_relative_eq!(map.local_y(3), 0.06, epsilon = 1e-15);
    }

    #[test]
    fn samples_behind_the_camera_are_recorded_as_none() {
        // A target placed *behind* the camera (negative camera-frame depth):
        // no sample can be imaged, so every cell is None and the map is still
        // returned (the computation never aborts).
        let cam = camera(0.0, 0.0);
        let target = frontoparallel_target(0.1, 0.1, -0.5);
        let map = defocus_map(&cam, &target, 4, 4).unwrap();
        assert!(map.coc_px().iter().all(Option::is_none));
        assert!(map.max_coc_px().is_none());
    }

    #[test]
    fn get_is_bounds_checked() {
        let cam = camera(0.0, 0.0);
        let target = frontoparallel_target(0.1, 0.1, FOCUS_M);
        let map = defocus_map(&cam, &target, 3, 3).unwrap();
        assert!(map.get(3, 0).is_none());
        assert!(map.get(0, 3).is_none());
        assert!(map.get(2, 2).is_some());
    }
}
