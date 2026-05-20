//! Projecting the 3D laser stripe through the scene camera into pixel space.
//!
//! [`super::intersect`] gives the laser stripe as a 3D polyline on the target
//! with a per-sample physical cross-section width. This module pushes that
//! polyline through a [`CameraEntity`]'s projection chain to produce the
//! **simulated image** of the laser line: a pixel-space polyline whose
//! per-vertex thickness is the geometric cross-section width combined with the
//! defocus blur **in quadrature**.
//!
//! # The projection chain
//!
//! Each 3D stripe sample is in **world** coordinates. It is transformed into
//! the camera-local frame (`camera.pose.inverse()`), then projected to a pixel
//! by the `vision-calibration-core` camera model
//! ([`CameraParams::build`](vision_calibration_core::CameraParams::build) →
//! [`Camera::project_point_c`](vision_calibration_core::CameraModel::project_point_c)).
//! `project_point_c` rejects points with camera-frame `z ≤ 0` (behind the
//! camera); such a sample contributes no projected vertex.
//!
//! # Per-vertex pixel width: geometric ⊕ defocus, in quadrature
//!
//! The imaged line has a finite thickness from two independent causes, added
//! in quadrature (the standard combination of independent blur contributions):
//!
//! ```text
//! total_px = sqrt(geom_px² + defocus_px²)
//! ```
//!
//! - **`geom_px`** — the *physical* cross-section `w(d)` as the camera sees it.
//!   It is obtained projectively, not by a scale factor: two points offset
//!   from the stripe sample by `±w(d)/2`, **perpendicular to the stripe and to
//!   the camera ray**, are projected, and `geom_px` is their pixel separation.
//!   Doing it this way is robust to an obliquely-viewed target — foreshortening
//!   is captured exactly by the projection — and the offset direction is the
//!   one in which the stripe's thickness is most visible to the camera.
//! - **`defocus_px`** — the circle-of-confusion *diameter* in pixels at that
//!   3D point, from M4's [`ThickLens::coc_diameter_px`]. Reusing the M4 model
//!   here is the intended design (development plan, M5).
//!
//! # Output
//!
//! [`project_stripe`] returns a [`ProjectedStripe`] — a polyline of
//! [`ProjectedPoint`]s, each a pixel coordinate plus its `total_px` width (and
//! the `geom_px` / `defocus_px` components, kept for inspection and the UI
//! legend). Stripe samples that cannot be imaged are dropped; the projected
//! polyline therefore has between `0` and `stripe.len()` vertices.

use nalgebra::{Point2, Point3, Vector3};

use crate::laser::intersect::LaserStripe;
use crate::optics::ThickLens;
use crate::scene::CameraEntity;

/// Vectors shorter than this are treated as degenerate (no usable direction).
const MIN_NORM: f64 = 1e-12;

/// One projected vertex of the laser line in the simulated image.
///
/// All pixel quantities are in the camera's pixel coordinate system; widths
/// are full line **thicknesses** in pixels (matching the cross-section
/// *diameter* and the circle-of-confusion *diameter* they are built from).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectedPoint {
    /// The stripe sample's image coordinate, in pixels.
    pub pixel: Point2<f64>,
    /// The stripe sample's 3D position in **world** coordinates (metres). The
    /// M8 `resolution_mm_per_px` summary computes mm/px from polyline
    /// differences in this field and the pixel field; downstream consumers
    /// (the Scheimpflug solver, the simulated-image panel) can also read
    /// world position without re-running the projection.
    pub world_m: Point3<f64>,
    /// The geometric line width in pixels — the physical cross-section `w(d)`
    /// as seen by the camera (projective, obliquity-aware).
    pub geom_px: f64,
    /// The defocus line width in pixels — the circle-of-confusion diameter
    /// from the M4 [`ThickLens`] model at this 3D point.
    pub defocus_px: f64,
    /// The total imaged line width in pixels: `sqrt(geom_px² + defocus_px²)`.
    pub total_px: f64,
}

/// The projected laser line — the "simulated image" of the stripe.
///
/// Produced by [`project_stripe`]: a pixel-space polyline of
/// [`ProjectedPoint`]s. It has between `0` and the source stripe's sample
/// count vertices, since stripe samples that cannot be imaged (behind the
/// camera, or outside the projection chain) are dropped.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectedStripe {
    /// The projected polyline vertices, in stripe order. May be empty (the
    /// whole stripe is unimageable) or have a single vertex (a near-degenerate
    /// stripe); callers must not assume `>= 2`.
    points: Vec<ProjectedPoint>,
}

impl ProjectedStripe {
    /// The projected polyline vertices, in stripe order.
    #[must_use]
    pub fn points(&self) -> &[ProjectedPoint] {
        &self.points
    }

    /// The number of projected vertices.
    #[must_use]
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Whether the projected line has no vertices — the whole stripe was
    /// unimageable.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// The largest total line width over the projected vertices, in pixels,
    /// or `None` if the line has no vertices.
    ///
    /// Useful for scaling the simulated-image panel's thickness encoding.
    #[must_use]
    pub fn max_total_px(&self) -> Option<f64> {
        self.points
            .iter()
            .map(|p| p.total_px)
            .filter(|w| w.is_finite())
            .fold(None, |acc, w| Some(acc.map_or(w, |a: f64| a.max(w))))
    }

    /// Per-vertex stripe resolution, in **millimetres of object-space
    /// displacement per pixel of imaged-line displacement** (mm/px).
    ///
    /// At each vertex this is the local finite-difference Jacobian of the
    /// projection along the stripe direction: the world-space arc length
    /// between the vertex's polyline neighbours divided by the pixel-space
    /// arc length between the same neighbours. Endpoints use one-sided
    /// differences. Returns an empty `Vec` when the polyline has fewer than
    /// two vertices (no neighbours, so no resolution defined).
    ///
    /// A resolution of, say, 0.05 mm/px means **20 pixels per millimetre** of
    /// object space along the stripe — the standard metrology figure for a
    /// triangulation sensor. A larger mm/px means coarser resolution.
    #[must_use]
    pub fn resolution_mm_per_px(&self) -> Vec<f64> {
        let n = self.points.len();
        if n < 2 {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let lo = i.saturating_sub(1);
            let hi = (i + 1).min(n - 1);
            let dworld_m = (self.points[hi].world_m - self.points[lo].world_m).norm();
            let dpix = (self.points[hi].pixel - self.points[lo].pixel).norm();
            let res = if dpix > MIN_NORM {
                // metres → millimetres = ×1000.
                (dworld_m * 1.0e3) / dpix
            } else {
                f64::INFINITY
            };
            out.push(res);
        }
        out
    }

    /// `(min, median, max)` of [`resolution_mm_per_px`](Self::resolution_mm_per_px)
    /// across the stripe, in mm/px. `None` if the polyline has fewer than
    /// two vertices.
    ///
    /// The simulated-image panel reports these three numbers so the designer
    /// can see at a glance whether the stripe's metrology resolution is
    /// uniform (min ≈ median ≈ max) or varies — e.g., an obliquely viewed
    /// stripe will have a noticeable spread between the near and far ends.
    #[must_use]
    pub fn resolution_summary_mm_per_px(&self) -> Option<(f64, f64, f64)> {
        let mut res: Vec<f64> = self
            .resolution_mm_per_px()
            .into_iter()
            .filter(|r| r.is_finite())
            .collect();
        if res.is_empty() {
            return None;
        }
        res.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let n = res.len();
        let min = res[0];
        let max = res[n - 1];
        let median = if n.is_multiple_of(2) {
            0.5 * (res[n / 2 - 1] + res[n / 2])
        } else {
            res[n / 2]
        };
        Some((min, median, max))
    }
}

/// Project a 3D laser [`LaserStripe`] into the camera's pixel space.
///
/// Transforms each stripe sample world→camera-local, projects it to a pixel,
/// and computes the per-vertex line width as `geom_px ⊕ defocus_px` in
/// quadrature (see the module doc-comment). Stripe samples the camera cannot
/// image are silently dropped.
///
/// # Parameters
///
/// - `stripe`: the 3D laser stripe from [`stripe_on_target`](super::intersect::stripe_on_target).
/// - `camera`: the viewing camera; its `pose` places the camera in the world
///   and its [`thick_lens`](CameraEntity::thick_lens) supplies the defocus
///   model and the projection chain.
///
/// # Errors
///
/// Returns [`Error`](crate::Error) only if the camera's physical optics
/// parameters are inconsistent (propagated from
/// [`CameraEntity::thick_lens`]); this cannot happen for a camera built
/// through [`CameraEntity::new`]. A stripe sample that simply fails to image
/// is **not** an error — it is dropped from the output.
pub fn project_stripe(
    stripe: &LaserStripe,
    camera: &CameraEntity,
) -> crate::Result<ProjectedStripe> {
    // The defocus model (also carries the projection-chain CameraParams).
    let lens: ThickLens = camera.thick_lens()?;
    let camera_model = lens.params().build();
    let camera_from_world = camera.pose.inverse();

    let samples = stripe.samples();
    let mut points: Vec<ProjectedPoint> = Vec::with_capacity(samples.len());

    for (i, sample) in samples.iter().enumerate() {
        // World -> camera-local.
        let p_cam = camera_from_world * sample.point;

        // Project the sample centre. `project_point_c` rejects z <= 0.
        let Some(pixel) = camera_model.project_point_c(&p_cam.coords) else {
            continue;
        };

        // --- Defocus blur in pixels (M4 model) --------------------------
        // The CoC diameter at this 3D point. If the lens cannot image the
        // point (at/inside the focal point), there is no defocus figure;
        // treat it as zero rather than dropping the otherwise-projectable
        // vertex.
        let defocus_px = lens.coc_diameter_px(&p_cam).unwrap_or(0.0).max(0.0);

        // --- Geometric width in pixels (projective) ---------------------
        // The stripe tangent at this sample, from its polyline neighbours.
        let tangent = stripe_tangent(samples, i);
        let geom_px = geometric_width_px(
            sample.point,
            sample.width_m,
            tangent,
            &camera.pose,
            &camera_from_world,
            &camera_model,
            pixel,
        );

        // --- Quadrature -------------------------------------------------
        let total_px = (geom_px * geom_px + defocus_px * defocus_px).sqrt();

        points.push(ProjectedPoint {
            pixel,
            world_m: sample.point,
            geom_px,
            defocus_px,
            total_px,
        });
    }

    Ok(ProjectedStripe { points })
}

/// The unit tangent of the stripe polyline at sample `i`, in world space.
///
/// Uses a central difference between the neighbouring samples where possible,
/// a one-sided difference at the ends. Returns `+x` as a harmless fallback if
/// the neighbours coincide (a degenerate stripe), which only affects the
/// geometric-width offset direction — never the projected position.
fn stripe_tangent(samples: &[crate::laser::intersect::StripeSample], i: usize) -> Vector3<f64> {
    let n = samples.len();
    if n < 2 {
        return Vector3::x();
    }
    let lo = i.saturating_sub(1);
    let hi = (i + 1).min(n - 1);
    let delta = samples[hi].point - samples[lo].point;
    let norm = delta.norm();
    if norm < MIN_NORM {
        Vector3::x()
    } else {
        delta / norm
    }
}

/// The geometric line width in pixels at a stripe sample.
///
/// Offsets the 3D sample by `±width_m/2` along a direction that is
/// perpendicular to **both** the stripe tangent and the camera viewing ray —
/// the direction in which the stripe's physical thickness is most visible to
/// the camera — projects the two offset points, and returns their pixel
/// separation. Because the separation is measured *after* projection, target
/// obliquity and perspective foreshortening are captured exactly.
///
/// Falls back gracefully: if the perpendicular direction degenerates (the
/// stripe is viewed end-on, tangent parallel to the view ray) the tangent's
/// orthogonal complement within the image is used instead; if even the offset
/// points fail to image, the width is reported as `0`.
#[allow(clippy::too_many_arguments)]
fn geometric_width_px(
    point_world: Point3<f64>,
    width_m: f64,
    tangent_world: Vector3<f64>,
    camera_pose: &nalgebra::Isometry3<f64>,
    camera_from_world: &nalgebra::Isometry3<f64>,
    camera_model: &vision_calibration_core::CameraModel,
    pixel_centre: Point2<f64>,
) -> f64 {
    if !(width_m.is_finite() && width_m > 0.0) {
        return 0.0;
    }
    // The camera eye in world coordinates is the camera pose translation.
    let eye = camera_pose * Point3::origin();
    let view_ray = point_world - eye;
    let view_norm = view_ray.norm();

    // Offset direction: perpendicular to the stripe tangent and to the view
    // ray. This is the across-stripe direction whose extent the camera sees
    // most fully.
    let mut offset_dir = if view_norm >= MIN_NORM {
        tangent_world.cross(&(view_ray / view_norm))
    } else {
        Vector3::zeros()
    };
    if offset_dir.norm() < MIN_NORM {
        // The stripe is (nearly) viewed end-on. Fall back to any direction
        // perpendicular to the tangent.
        offset_dir = perpendicular_to(tangent_world);
    }
    let offset_dir_norm = offset_dir.norm();
    if offset_dir_norm < MIN_NORM {
        return 0.0;
    }
    let offset = offset_dir / offset_dir_norm * (0.5 * width_m);

    // Project the two offset points.
    let p_plus = camera_from_world * (point_world + offset);
    let p_minus = camera_from_world * (point_world - offset);
    let px_plus = camera_model.project_point_c(&p_plus.coords);
    let px_minus = camera_model.project_point_c(&p_minus.coords);

    match (px_plus, px_minus) {
        (Some(a), Some(b)) => (a - b).norm(),
        // Only one offset point images: estimate the full width as twice the
        // half-width from the imaged side to the centre.
        (Some(a), None) => 2.0 * (a - pixel_centre).norm(),
        (None, Some(b)) => 2.0 * (b - pixel_centre).norm(),
        // Neither offset images: no usable geometric width.
        (None, None) => 0.0,
    }
}

/// Any unit vector perpendicular to `v`.
///
/// Crosses `v` with whichever cardinal axis it is least aligned with, so the
/// cross product is well-conditioned. Returns `+x` if `v` is degenerate.
fn perpendicular_to(v: Vector3<f64>) -> Vector3<f64> {
    let n = v.norm();
    if n < MIN_NORM {
        return Vector3::x();
    }
    let u = v / n;
    // Pick the axis least parallel to u.
    let ax = u.x.abs();
    let ay = u.y.abs();
    let az = u.z.abs();
    let axis = if ax <= ay && ax <= az {
        Vector3::x()
    } else if ay <= az {
        Vector3::y()
    } else {
        Vector3::z()
    };
    let perp = u.cross(&axis);
    let pn = perp.norm();
    if pn < MIN_NORM {
        Vector3::x()
    } else {
        perp / pn
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::laser::intersect::stripe_on_target;
    use crate::laser::plane::LaserPlane;
    use crate::laser::width::GaussianBeamWidth;
    use approx::assert_relative_eq;
    use nalgebra::{Isometry3, Translation3, UnitQuaternion};
    use vision_calibration_core::{
        CameraParams, DistortionParams, FxFyCxCySkew, IntrinsicsParams, ProjectionParams,
        ScheimpflugParams, SensorParams,
    };

    // A realistic machine-vision lens, matching the optics-module tests.
    const F_M: f64 = 0.025;
    const FNUM: f64 = 4.0;
    const PITCH_M: f64 = 3.45e-6;

    /// A camera at `eye` looking at `target_pt`, with the standard test lens
    /// focused at `focus_m` and the given Scheimpflug tilt.
    fn camera_at(
        eye: Point3<f64>,
        target_pt: Point3<f64>,
        focus_m: f64,
        tau_x: f64,
    ) -> CameraEntity {
        let rot = nalgebra::Rotation3::face_towards(&(target_pt - eye), &(-Vector3::z()));
        let pose = Isometry3::from_parts(
            Translation3::from(eye.coords),
            UnitQuaternion::from_rotation_matrix(&rot),
        );
        let params = CameraParams {
            projection: ProjectionParams::Pinhole,
            distortion: DistortionParams::None,
            sensor: SensorParams::Scheimpflug {
                params: ScheimpflugParams {
                    tilt_x: tau_x,
                    tilt_y: 0.0,
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
            pose,
            params,
            (1280, 1024),
            F_M,
            FNUM,
            focus_m,
            0.0,
            PITCH_M,
            0.05,
            3.0,
        )
        .expect("valid camera")
    }

    /// A vertical fan at `(0, -standoff, 0)` aimed along world `+y`.
    fn laser_aimed_along_y(standoff: f64, half: f64, length: f64) -> LaserPlane {
        let rot = UnitQuaternion::rotation_between(&Vector3::z(), &Vector3::y()).unwrap();
        let pose = Isometry3::from_parts(Translation3::new(0.0, -standoff, 0.0), rot);
        let laser = crate::scene::LaserEntity::new(pose, half, length, 660.0, 0.25e-3).unwrap();
        LaserPlane::from_entity(&laser)
    }

    /// A target rectangle in the world `y = 0` plane facing the laser, with an
    /// optional tilt about world `x`.
    fn target_facing_y(width: f64, height: f64, tilt_x: f64) -> TargetEntity {
        let base = UnitQuaternion::rotation_between(&Vector3::z(), &Vector3::y()).unwrap();
        let tilt = UnitQuaternion::from_axis_angle(&Vector3::x_axis(), tilt_x);
        let pose = Isometry3::from_parts(Translation3::new(0.0, 0.0, 0.0), tilt * base);
        TargetEntity::new(pose, width, height).unwrap()
    }

    use crate::scene::TargetEntity;

    fn width_model() -> GaussianBeamWidth {
        GaussianBeamWidth::new(0.25e-3, 660.0e-9).unwrap()
    }

    #[test]
    fn projects_every_sample_of_a_visible_stripe() {
        // Laser straight at a fronto-parallel target, camera offset to the
        // side and aimed at the same target centre — the whole stripe is in
        // front of the camera, so every sample projects.
        let laser = laser_aimed_along_y(0.6, 0.3, 1.5);
        let target = target_facing_y(0.3, 0.25, 0.0);
        let stripe = stripe_on_target(&laser, &target, &width_model(), 15).unwrap();

        let camera = camera_at(
            Point3::new(0.25, -0.55, 0.0),
            Point3::new(0.0, 0.0, 0.0),
            0.6,
            0.0,
        );
        let projected = project_stripe(&stripe, &camera).unwrap();
        assert_eq!(projected.len(), stripe.len());
        for p in projected.points() {
            assert!(p.pixel.x.is_finite() && p.pixel.y.is_finite());
            assert!(p.geom_px >= 0.0);
            assert!(p.defocus_px >= 0.0);
            assert!(p.total_px >= 0.0);
        }
    }

    #[test]
    fn total_width_is_the_quadrature_of_the_components() {
        let laser = laser_aimed_along_y(0.6, 0.3, 1.5);
        let target = target_facing_y(0.3, 0.25, 0.0);
        let stripe = stripe_on_target(&laser, &target, &width_model(), 11).unwrap();
        let camera = camera_at(
            Point3::new(0.25, -0.55, 0.0),
            Point3::new(0.0, 0.0, 0.0),
            0.6,
            0.0,
        );
        let projected = project_stripe(&stripe, &camera).unwrap();
        for p in projected.points() {
            let expected = (p.geom_px * p.geom_px + p.defocus_px * p.defocus_px).sqrt();
            assert_relative_eq!(p.total_px, expected, epsilon = 1e-12);
            // Quadrature: the total is at least each component and at most
            // their sum.
            assert!(p.total_px >= p.geom_px - 1e-12);
            assert!(p.total_px >= p.defocus_px - 1e-12);
            assert!(p.total_px <= p.geom_px + p.defocus_px + 1e-9);
        }
    }

    #[test]
    fn an_in_focus_stripe_has_near_zero_defocus_width() {
        // Camera focused exactly on the target plane: a fronto-parallel target
        // at that focus distance is everywhere sharp, so defocus_px ≈ 0 and
        // the total width is essentially the geometric width alone.
        let laser = laser_aimed_along_y(0.6, 0.3, 1.5);
        let target = target_facing_y(0.3, 0.25, 0.0);
        let stripe = stripe_on_target(&laser, &target, &width_model(), 11).unwrap();
        // Camera on the optical axis straight in front of the target, focused
        // on it: every stripe point sits on the plane of best focus.
        let eye = Point3::new(0.0, -0.6, 0.0);
        let camera = camera_at(eye, Point3::new(0.0, 0.0, 0.0), 0.6, 0.0);
        let projected = project_stripe(&stripe, &camera).unwrap();
        for p in projected.points() {
            assert!(
                p.defocus_px < 1e-6,
                "an in-focus stripe sample should have ~0 defocus, got {}",
                p.defocus_px
            );
            assert_relative_eq!(p.total_px, p.geom_px, epsilon = 1e-6);
        }
    }

    #[test]
    fn the_line_broadens_out_of_focus() {
        // The M5 checkpoint: an oblique target leaves part of the stripe out
        // of the depth of field. With the camera focused on the near end, the
        // far end of the stripe is more defocused — defocus_px (and the total
        // width) grows along the stripe.
        let laser = laser_aimed_along_y(0.6, 0.45, 2.0);
        // A target tilted hard about world x: its depth varies strongly along
        // the (vertical) stripe.
        let target = target_facing_y(0.5, 0.6, 0.7);
        let stripe = stripe_on_target(&laser, &target, &width_model(), 31).unwrap();

        // Focus the camera on the *near end* of the stripe. The defocus model
        // measures depth along the optical axis (camera-frame z), so the
        // focus distance must be that near sample's camera-frame z, not its
        // slant distance from the eye.
        let eye = Point3::new(0.4, -0.55, 0.0);
        let camera_aim = Point3::new(0.0, 0.0, 0.0);
        // Build a provisional camera just to read the camera-frame depths.
        let probe = camera_at(eye, camera_aim, 0.6, 0.0);
        let cam_from_world = probe.pose.inverse();
        // Camera-frame z of every stripe sample.
        let depths: Vec<f64> = stripe
            .samples()
            .iter()
            .map(|s| (cam_from_world * s.point).z)
            .collect();
        let near_depth = depths.iter().copied().fold(f64::INFINITY, f64::min);
        let far_depth = depths.iter().copied().fold(0.0_f64, f64::max);
        // The target really is oblique — the depth genuinely varies.
        assert!(far_depth > near_depth + 0.05, "stripe depth must vary");

        // Focus exactly on the nearest sample's depth.
        let camera = camera_at(eye, camera_aim, near_depth, 0.0);
        let projected = project_stripe(&stripe, &camera).unwrap();
        assert!(projected.len() >= 3);

        // The defocus width spans a real range — sharp at the focused end,
        // blurred at the far end.
        let defocus: Vec<f64> = projected.points().iter().map(|p| p.defocus_px).collect();
        let min = defocus.iter().copied().fold(f64::INFINITY, f64::min);
        let max = defocus.iter().copied().fold(0.0_f64, f64::max);
        assert!(
            min < 1.0,
            "the focused end of the stripe should be near-sharp, min defocus {min}"
        );
        assert!(
            max > 2.0,
            "the far end of the stripe should be visibly blurred, max defocus {max}"
        );
        // The total width genuinely broadens: max exceeds min by a clear margin.
        let total_max = projected.max_total_px().unwrap();
        let total_min = projected
            .points()
            .iter()
            .map(|p| p.total_px)
            .fold(f64::INFINITY, f64::min);
        assert!(total_max > total_min + 1.0);
    }

    #[test]
    fn geometric_width_is_positive_for_a_visible_stripe() {
        // The geometric width is the physical beam cross-section as imaged: a
        // sub-pixel-but-positive figure for a 0.5 mm beam at ~0.6 m on a
        // 4637 px focal length.
        let laser = laser_aimed_along_y(0.6, 0.3, 1.5);
        let target = target_facing_y(0.3, 0.25, 0.0);
        let stripe = stripe_on_target(&laser, &target, &width_model(), 9).unwrap();
        let camera = camera_at(
            Point3::new(0.25, -0.55, 0.0),
            Point3::new(0.0, 0.0, 0.0),
            0.6,
            0.0,
        );
        let projected = project_stripe(&stripe, &camera).unwrap();
        for p in projected.points() {
            assert!(
                p.geom_px > 0.0,
                "a finite-width beam must image to a positive geometric width"
            );
        }
    }

    /// A constant-width [`WidthModel`] — a collimated beam. Lets a test fix
    /// the physical cross-section width directly, without the Gaussian-beam
    /// law's distance dependence.
    struct ConstantWidth(f64);
    impl crate::laser::width::WidthModel for ConstantWidth {
        fn width_at(&self, _distance_m: f64) -> f64 {
            self.0
        }
    }

    #[test]
    fn a_wider_beam_projects_to_a_wider_geometric_width() {
        // The geometric pixel width scales with the physical cross-section:
        // doubling the (constant) beam width doubles the imaged geom_px,
        // because the projection is locally linear in the small ± offset.
        let laser = laser_aimed_along_y(0.6, 0.3, 1.5);
        let target = target_facing_y(0.3, 0.25, 0.0);
        let stripe_narrow = stripe_on_target(&laser, &target, &ConstantWidth(0.5e-3), 9).unwrap();
        let stripe_wide = stripe_on_target(&laser, &target, &ConstantWidth(1.0e-3), 9).unwrap();
        let camera = camera_at(
            Point3::new(0.25, -0.55, 0.0),
            Point3::new(0.0, 0.0, 0.0),
            0.6,
            0.0,
        );
        let pn = project_stripe(&stripe_narrow, &camera).unwrap();
        let pw = project_stripe(&stripe_wide, &camera).unwrap();
        // Compare the middle vertex.
        let mid = pn.len() / 2;
        let gn = pn.points()[mid].geom_px;
        let gw = pw.points()[mid].geom_px;
        assert!(gn > 0.0 && gw > gn, "a wider beam must image wider");
        // A 2x physical width gives a 2x imaged width (locally linear).
        assert!(
            (gw / gn - 2.0).abs() < 0.02,
            "ratio {} should be ~2",
            gw / gn
        );
    }

    #[test]
    fn samples_behind_the_camera_are_dropped() {
        // Put the camera *between* the target and infinity, looking away —
        // the stripe is then behind the camera and nothing projects.
        let laser = laser_aimed_along_y(0.6, 0.3, 1.5);
        let target = target_facing_y(0.3, 0.25, 0.0);
        let stripe = stripe_on_target(&laser, &target, &width_model(), 11).unwrap();
        // Camera at the target, looking further along +y (away from the
        // stripe, which is at y = 0 i.e. behind this camera).
        let camera = camera_at(
            Point3::new(0.0, 0.5, 0.0),
            Point3::new(0.0, 5.0, 0.0),
            0.6,
            0.0,
        );
        let projected = project_stripe(&stripe, &camera).unwrap();
        assert!(
            projected.is_empty(),
            "a stripe behind the camera must project to no vertices"
        );
        assert!(projected.max_total_px().is_none());
    }

    #[test]
    fn projected_pixels_land_on_the_sensor_for_the_default_scene() {
        // The real default MVP scene: its camera frames the target, and the
        // laser stripe sits well inside it — so the projected line must land
        // on the 1280x1024 sensor.
        let scene = crate::scene::Scene::default_mvp();
        let camera = &scene.cameras[0];
        let laser = LaserPlane::from_entity(&scene.lasers[0]);
        let model = GaussianBeamWidth::from_laser(&scene.lasers[0]).unwrap();
        let stripe = stripe_on_target(&laser, &scene.targets[0], &model, 21)
            .expect("the default laser must strike the default target");

        let projected = project_stripe(&stripe, camera).unwrap();
        assert!(
            !projected.is_empty(),
            "the default laser stripe must be visible to the default camera"
        );
        // Every projected sample lands inside the sensor.
        let (res_x, res_y) = camera.resolution_f64();
        for p in projected.points() {
            assert!(
                (0.0..=res_x).contains(&p.pixel.x),
                "pixel x {} outside [0, {res_x}]",
                p.pixel.x
            );
            assert!(
                (0.0..=res_y).contains(&p.pixel.y),
                "pixel y {} outside [0, {res_y}]",
                p.pixel.y
            );
        }
    }

    #[test]
    fn resolution_mm_per_px_is_finite_and_positive_for_the_default_scene() {
        // Sanity: every projected vertex has a finite, strictly positive
        // mm/px figure for the standard scene, and the summary is non-empty.
        let scene = crate::scene::Scene::default_mvp();
        let camera = &scene.cameras[0];
        let laser = LaserPlane::from_entity(&scene.lasers[0]);
        let model = GaussianBeamWidth::from_laser(&scene.lasers[0]).unwrap();
        let stripe = stripe_on_target(&laser, &scene.targets[0], &model, 21)
            .expect("default laser strikes default target");
        let projected = project_stripe(&stripe, camera).unwrap();
        let res = projected.resolution_mm_per_px();
        assert_eq!(res.len(), projected.len());
        for &r in &res {
            assert!(r.is_finite() && r > 0.0, "every vertex needs a real mm/px");
        }
        let (mn, md, mx) = projected
            .resolution_summary_mm_per_px()
            .expect("non-empty summary");
        assert!(
            mn <= md && md <= mx,
            "summary must be ordered: {mn} {md} {mx}"
        );
    }

    #[test]
    fn resolution_summary_is_uniform_for_a_perpendicular_stripe() {
        // Camera on the optical axis looking straight at the target, focused
        // on it: the stripe is fronto-parallel, so mm/px is essentially
        // constant across its length (within a few percent due to perspective
        // and stripe-tangent variation).
        let laser = laser_aimed_along_y(0.6, 0.3, 1.5);
        let target = target_facing_y(0.5, 0.5, 0.0);
        let stripe = stripe_on_target(&laser, &target, &width_model(), 21).unwrap();
        let camera = camera_at(
            Point3::new(0.0, -0.6, 0.0),
            Point3::new(0.0, 0.0, 0.0),
            0.6,
            0.0,
        );
        let projected = project_stripe(&stripe, &camera).unwrap();
        let (mn, _md, mx) = projected.resolution_summary_mm_per_px().expect("non-empty");
        // A near-fronto-parallel view should show min/max agreement to better
        // than 5 % — generous to handle the polyline endpoints' one-sided
        // differences.
        assert!(
            (mx - mn) / mn < 0.05,
            "uniform stripe should have ≤5 % mm/px spread, got min={mn} max={mx}"
        );
    }

    #[test]
    fn resolution_summary_is_none_for_a_singleton_projection() {
        // A short stripe whose only sample projects gives a 1-vertex polyline
        // — no neighbours, no resolution defined.
        let laser = laser_aimed_along_y(0.6, 0.3, 1.5);
        let target = target_facing_y(0.3, 0.25, 0.0);
        let stripe = stripe_on_target(&laser, &target, &width_model(), 1).unwrap();
        let camera = camera_at(
            Point3::new(0.0, -0.6, 0.0),
            Point3::new(0.0, 0.0, 0.0),
            0.6,
            0.0,
        );
        let projected = project_stripe(&stripe, &camera).unwrap();
        if projected.len() < 2 {
            assert!(projected.resolution_summary_mm_per_px().is_none());
            assert!(projected.resolution_mm_per_px().is_empty());
        }
    }

    #[test]
    fn perpendicular_to_returns_an_orthogonal_unit_vector() {
        for v in [
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(1.0, 2.0, 3.0),
            Vector3::new(-0.4, 0.1, 0.9),
        ] {
            let p = perpendicular_to(v);
            assert_relative_eq!(p.norm(), 1.0, epsilon = 1e-12);
            assert_relative_eq!(p.dot(&v.normalize()), 0.0, epsilon = 1e-12);
        }
    }
}
