//! Intersecting the laser fan with a planar target — the laser *stripe*.
//!
//! Where a line laser's fan meets a planar inspection target, it paints a
//! bright stripe. This module computes that stripe geometrically: it
//! intersects a [`LaserPlane`] with a planar [`TargetEntity`], clips the
//! result to the fan's angular/radial extent **and** the target rectangle, and
//! samples the surviving segment into a [`LaserStripe`] polyline with a
//! per-sample cross-section width.
//!
//! # Why this is one straight segment (MVP scope)
//!
//! Two planes — the laser fan plane and the target plane — meet in a single
//! straight line. Restricting the MVP target to an analytic plane (development
//! plan, de-risking spike #3) makes the laser stripe **one straight 3D
//! segment**, with no polyline stitching across triangle boundaries. The
//! general triangle-mesh intersection (a piecewise-linear curve) is explicitly
//! post-MVP. The output type is still a polyline so the projection layer
//! ([`super::project`]) and a future mesh intersection share one interface.
//!
//! # The construction
//!
//! 1. **Two planes → a line.** The laser fan plane has normal `n_L`, the
//!    target plane normal `n_T`. The intersection line has direction
//!    `d = n_L × n_T`; a point on it is found by solving the two plane
//!    equations together. Parallel planes (`d ≈ 0`) yield no stripe.
//! 2. **Clip to the fan.** Along the line, the fan only illuminates the
//!    interval where the fan-angle stays within `±half_angle` and the radius
//!    stays within `length` (see [`LaserPlane::contains_in_extent`]). The
//!    angular bound is two half-plane cuts; the radial bound is a
//!    ray–circle-style cut. Behind-the-laser portions are excluded because the
//!    fan only opens forward.
//! 3. **Clip to the target rectangle.** The line is intersected with the four
//!    edges of the finite target rectangle (a 2D segment-vs-rectangle clip in
//!    the target's local frame).
//! 4. **Sample.** The surviving 3D segment is sampled into `N` evenly-spaced
//!    points; each records its distance from the laser origin and its
//!    cross-section width from a [`WidthModel`].

use nalgebra::{Point3, Vector3};

use crate::laser::plane::LaserPlane;
use crate::laser::width::WidthModel;
use crate::scene::TargetEntity;

/// A direction/normal whose norm is below this is treated as degenerate.
const MIN_NORM: f64 = 1e-12;

/// A single sample along the laser stripe — one vertex of the [`LaserStripe`]
/// polyline.
///
/// All quantities are in world coordinates / SI metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StripeSample {
    /// The sample point on the target surface, in world coordinates.
    pub point: Point3<f64>,
    /// Straight-line distance from the laser origin to [`StripeSample::point`],
    /// in metres. Feeds the [`WidthModel`] and the defocus model.
    pub distance_from_laser_m: f64,
    /// The laser's cross-section thickness at this sample, in metres — the
    /// full beam **diameter** from the [`WidthModel`].
    pub width_m: f64,
}

/// The laser stripe on a planar target: a sampled 3D polyline with a
/// per-sample cross-section width.
///
/// Produced by [`stripe_on_target`]. Because the MVP target is a single
/// analytic plane, the stripe is one straight segment; [`LaserStripe::samples`]
/// holds it as `>= 2` evenly-spaced [`StripeSample`]s so the projection layer
/// can treat it as a general polyline.
#[derive(Clone, Debug, PartialEq)]
pub struct LaserStripe {
    /// The ordered samples along the stripe, from one end to the other.
    /// Always at least two (the segment endpoints).
    samples: Vec<StripeSample>,
}

impl LaserStripe {
    /// The ordered stripe samples, from one endpoint to the other.
    #[must_use]
    pub fn samples(&self) -> &[StripeSample] {
        &self.samples
    }

    /// The number of samples (polyline vertices); always `>= 2`.
    #[must_use]
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Whether the stripe has no samples. Always `false` for a stripe returned
    /// by [`stripe_on_target`] (it yields `Some` only with `>= 2` samples), so
    /// this is purely a Clippy-mandated companion to [`LaserStripe::len`].
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// The two world-space endpoints of the stripe segment.
    #[must_use]
    pub fn endpoints(&self) -> (Point3<f64>, Point3<f64>) {
        (
            self.samples[0].point,
            self.samples[self.samples.len() - 1].point,
        )
    }

    /// The straight-line length of the stripe segment, in metres.
    #[must_use]
    pub fn length_m(&self) -> f64 {
        let (a, b) = self.endpoints();
        (b - a).norm()
    }
}

/// Compute the laser stripe where a fan illuminates a planar target.
///
/// Intersects the `laser` fan plane with the `target` rectangle, clips the
/// resulting line to the fan's angular/radial extent and to the target
/// rectangle, and samples the surviving 3D segment into a [`LaserStripe`] of
/// `samples` evenly-spaced points. Each sample's cross-section width comes
/// from `width_model`.
///
/// Returns `None` when there is **no stripe**: the fan plane and target plane
/// are parallel, or their intersection line never enters both the fan extent
/// and the target rectangle (the laser misses the target, or only grazes it
/// at a point).
///
/// # Parameters
///
/// - `laser`: the laser fan, as a [`LaserPlane`] in world coordinates.
/// - `target`: the planar inspection target.
/// - `width_model`: the cross-section width model (e.g.
///   [`GaussianBeamWidth`](crate::laser::GaussianBeamWidth)); any
///   [`WidthModel`] is accepted, supporting a per-component override.
/// - `samples`: the number of polyline vertices to emit; clamped up to a
///   minimum of 2 so the segment is always representable.
///
/// # Coordinate frames
///
/// The target rectangle clip is done in the target's **local** frame (where
/// the rectangle is axis-aligned and centred), then mapped back to the world;
/// the fan-extent clip is done with [`LaserPlane`]'s world-frame helpers. The
/// returned samples are in world coordinates.
#[must_use]
pub fn stripe_on_target(
    laser: &LaserPlane,
    target: &TargetEntity,
    width_model: &dyn WidthModel,
    samples: usize,
) -> Option<LaserStripe> {
    let n_samples = samples.max(2);

    // --- The target plane, in world coordinates -------------------------
    // A TargetEntity is a rectangle in its local z = 0 plane; the local +z
    // axis mapped to the world is the plane normal, the pose translation a
    // point on it.
    let target_origin = target.pose * Point3::origin();
    let target_normal = target.pose * Vector3::z();

    // --- Two planes -> a line -------------------------------------------
    // Direction of the intersection line: perpendicular to both normals.
    let line_dir = laser.normal().cross(&target_normal);
    let line_dir_norm = line_dir.norm();
    if line_dir_norm < MIN_NORM {
        // The fan plane and the target plane are parallel: no stripe line.
        return None;
    }
    let line_dir = line_dir / line_dir_norm;

    // A point on the intersection line. Both planes pass through their own
    // origins; we look for a point of the form
    //   p = laser_origin + a * n_L + b * n_T
    // that also lies in the target plane. Solving the 2x2 system in (a, b)
    // built from the two plane equations gives a particular line point.
    let line_point =
        line_through_planes(laser.origin(), laser.normal(), target_origin, target_normal)?;

    // --- Clip the line to the fan's angular + radial extent -------------
    // Parameterise the line as `line_point + t * line_dir`. The fan extent is
    // a convex region within the fan plane (an angular wedge intersected with
    // a disc), so its intersection with the line is a single t-interval.
    let fan_interval = clip_line_to_fan(laser, line_point, line_dir)?;

    // --- Clip the line to the finite target rectangle -------------------
    let rect_interval = clip_line_to_rectangle(target, line_point, line_dir)?;

    // --- Intersect the two t-intervals ----------------------------------
    let t_lo = fan_interval.0.max(rect_interval.0);
    let t_hi = fan_interval.1.min(rect_interval.1);
    // A non-positive span means the fan extent and the target rectangle do
    // not overlap along the line — the laser misses the target (or grazes it
    // at a single point, which is not a drawable stripe). `t_lo` / `t_hi` are
    // finite here (every clip stage returns finite bounds for a real stripe),
    // so a plain comparison is well-defined.
    if t_hi <= t_lo {
        return None;
    }

    // --- Sample the surviving 3D segment --------------------------------
    let mut stripe_samples = Vec::with_capacity(n_samples);
    for i in 0..n_samples {
        let s = i as f64 / (n_samples - 1) as f64;
        let t = t_lo + s * (t_hi - t_lo);
        let point = line_point + line_dir * t;
        let distance_from_laser_m = (point - laser.origin()).norm();
        let width_m = width_model.width_at(distance_from_laser_m);
        stripe_samples.push(StripeSample {
            point,
            distance_from_laser_m,
            width_m,
        });
    }

    Some(LaserStripe {
        samples: stripe_samples,
    })
}

/// A particular point on the line where two planes meet.
///
/// Plane A passes through `point_a` with normal `normal_a`; plane B likewise.
/// Looks for a point `point_a + a·normal_a + b·normal_b` that satisfies plane
/// B's equation as well, by solving the 2×2 linear system the two plane
/// equations form. Returns `None` if the system is singular (parallel planes,
/// already screened by the caller's direction-norm test, but re-guarded here).
fn line_through_planes(
    point_a: Point3<f64>,
    normal_a: Vector3<f64>,
    point_b: Point3<f64>,
    normal_b: Vector3<f64>,
) -> Option<Point3<f64>> {
    // Plane A: nA · (p - pA) = 0. Plane B: nB · (p - pB) = 0.
    // Substitute p = pA + a·nA + b·nB. Plane A is satisfied for any a, b
    // *only* if we instead anchor at a point on plane A — so solve directly:
    //   nA · p = nA · pA = dA
    //   nB · p = nB · pB = dB
    // with p = pA + a·nA + b·nB. Then
    //   nA·p = dA + a (nA·nA) + b (nA·nB)
    //   nB·p = nB·pA + a (nB·nA) + b (nB·nB)
    // Setting nA·p = dA gives a (nA·nA) + b (nA·nB) = 0.
    // Setting nB·p = dB gives a (nB·nA) + b (nB·nB) = dB - nB·pA.
    let aa = normal_a.dot(&normal_a);
    let ab = normal_a.dot(&normal_b);
    let bb = normal_b.dot(&normal_b);
    let rhs0 = 0.0;
    let rhs1 = normal_b.dot(&point_b.coords) - normal_b.dot(&point_a.coords);

    let det = aa * bb - ab * ab;
    if det.abs() < MIN_NORM {
        return None;
    }
    let a = (rhs0 * bb - ab * rhs1) / det;
    let b = (aa * rhs1 - rhs0 * ab) / det;
    Some(point_a + normal_a * a + normal_b * b)
}

/// Clip the parametric line `base + t·dir` to the laser fan's extent.
///
/// The fan illuminates the convex region of its plane bounded by the angular
/// wedge `±half_angle` and the radius `≤ length`. The wedge contributes two
/// half-plane cuts; the disc a quadratic cut. Their intersection with the line
/// is one `t`-interval, returned as `(t_lo, t_hi)` with `t_lo < t_hi`, or
/// `None` if the line never enters the fan.
fn clip_line_to_fan(
    laser: &LaserPlane,
    base: Point3<f64>,
    dir: Vector3<f64>,
) -> Option<(f64, f64)> {
    // Express the line in the fan's in-plane (central, in_plane) basis,
    // anchored at the apex. The out-of-plane component is irrelevant — the
    // line already lies in the fan plane (it is the plane∩plane line).
    let central = laser.central();
    let in_plane = laser.in_plane();
    let rel = base - laser.origin();
    // Line in 2D fan coordinates: u(t) = u0 + t·du, v(t) = v0 + t·dv,
    // where u is along `central`, v along `in_plane`.
    let u0 = rel.dot(&central);
    let v0 = rel.dot(&in_plane);
    let du = dir.dot(&central);
    let dv = dir.dot(&in_plane);

    let half = laser.half_angle();
    let length = laser.length();

    // The fan wedge boundary rays are at phi = ±half_angle. The wedge interior
    // satisfies, for the +edge: the point lies on the central side of the
    // line through the apex along the +edge direction; likewise for -edge.
    //
    // Edge normals (pointing *into* the wedge) in (u, v) coordinates:
    //   +edge ray direction e+ = (cos h,  sin h); inward normal ( sin h, -cos h)
    //   -edge ray direction e- = (cos h, -sin h); inward normal (-sin h, -cos h)
    // Wait — derive carefully below to get the inward sense right.
    let (sh, ch) = half.sin_cos();
    // The wedge is { (u,v) : v ≤ u·tan(h)  AND  v ≥ -u·tan(h)  AND  u ≥ 0 }.
    // Equivalent half-planes (multiply through by cos h ≥ 0 to stay linear):
    //   (1)  u·sin h - v·cos h ≥ 0      (the +edge: v ≤ u·tan h)
    //   (2)  u·sin h + v·cos h ≥ 0      (the -edge: v ≥ -u·tan h)
    // and the forward condition u ≥ 0 is implied by (1)+(2) for h < π/2.

    // Start with the whole line, then cut by each constraint.
    let mut t_lo = f64::NEG_INFINITY;
    let mut t_hi = f64::INFINITY;

    // Half-plane (1): f(t) = (u0 + t·du)·sin h - (v0 + t·dv)·cos h ≥ 0.
    {
        let c0 = u0 * sh - v0 * ch;
        let cd = du * sh - dv * ch;
        cut_half_plane(c0, cd, &mut t_lo, &mut t_hi)?;
    }
    // Half-plane (2): g(t) = (u0 + t·du)·sin h + (v0 + t·dv)·cos h ≥ 0.
    {
        let c0 = u0 * sh + v0 * ch;
        let cd = du * sh + dv * ch;
        cut_half_plane(c0, cd, &mut t_lo, &mut t_hi)?;
    }

    // The radial bound: u² + v² ≤ length². Quadratic in t.
    {
        // (u0+t·du)² + (v0+t·dv)² ≤ L²
        // => (du²+dv²) t² + 2(u0·du + v0·dv) t + (u0²+v0² - L²) ≤ 0
        let qa = du * du + dv * dv;
        let qb = 2.0 * (u0 * du + v0 * dv);
        let qc = u0 * u0 + v0 * v0 - length * length;
        let (r_lo, r_hi) = solve_quadratic_interval(qa, qb, qc)?;
        t_lo = t_lo.max(r_lo);
        t_hi = t_hi.min(r_hi);
    }

    if t_hi > t_lo {
        Some((t_lo, t_hi))
    } else {
        None
    }
}

/// Tighten `[t_lo, t_hi]` by the constraint `c0 + cd·t ≥ 0`.
///
/// `None` if the constraint is `c0 < 0` with `cd ≈ 0` (the line is wholly
/// outside this half-plane).
fn cut_half_plane(c0: f64, cd: f64, t_lo: &mut f64, t_hi: &mut f64) -> Option<()> {
    if cd.abs() < MIN_NORM {
        // Constant constraint: the whole line is in (c0 ≥ 0) or out.
        if c0 < 0.0 {
            return None;
        }
        return Some(());
    }
    // c0 + cd·t ≥ 0  =>  t ≥ -c0/cd  (cd > 0)  or  t ≤ -c0/cd  (cd < 0).
    let t_bound = -c0 / cd;
    if cd > 0.0 {
        *t_lo = t_lo.max(t_bound);
    } else {
        *t_hi = t_hi.min(t_bound);
    }
    Some(())
}

/// Solve `qa·t² + qb·t + qc ≤ 0` for the `t`-interval `[t_lo, t_hi]`.
///
/// Handles the degenerate linear case (`qa ≈ 0`). Returns `None` when the
/// inequality has no solution (the quadratic is strictly positive everywhere).
fn solve_quadratic_interval(qa: f64, qb: f64, qc: f64) -> Option<(f64, f64)> {
    if qa.abs() < MIN_NORM {
        // Linear: qb·t + qc ≤ 0.
        if qb.abs() < MIN_NORM {
            // Constant: qc ≤ 0 admits all t, qc > 0 admits none.
            return if qc <= 0.0 {
                Some((f64::NEG_INFINITY, f64::INFINITY))
            } else {
                None
            };
        }
        let bound = -qc / qb;
        return if qb > 0.0 {
            Some((f64::NEG_INFINITY, bound))
        } else {
            Some((bound, f64::INFINITY))
        };
    }
    // Genuine quadratic. With qa > 0 the parabola opens upward and the region
    // ≤ 0 is the closed interval between the two roots (if real).
    let disc = qb * qb - 4.0 * qa * qc;
    if disc < 0.0 {
        // No real roots: the quadratic never changes sign.
        return if qa > 0.0 {
            None
        } else {
            Some((f64::NEG_INFINITY, f64::INFINITY))
        };
    }
    let sqrt_disc = disc.sqrt();
    let r0 = (-qb - sqrt_disc) / (2.0 * qa);
    let r1 = (-qb + sqrt_disc) / (2.0 * qa);
    let (lo, hi) = if r0 <= r1 { (r0, r1) } else { (r1, r0) };
    if qa > 0.0 {
        // Upward parabola: ≤ 0 between the roots.
        Some((lo, hi))
    } else {
        // Downward parabola: ≤ 0 outside the roots — not a single interval.
        // qa < 0 cannot occur here (qa = du²+dv² ≥ 0), so this branch is
        // unreachable in practice; treat it as "all t" to stay total.
        Some((f64::NEG_INFINITY, f64::INFINITY))
    }
}

/// Clip the parametric line `base + t·dir` to the finite target rectangle.
///
/// The rectangle is the target's local `[-w/2, w/2] × [-h/2, h/2]` patch in
/// its `z = 0` plane. The line is mapped into that local frame and clipped to
/// the axis-aligned rectangle with a Liang–Barsky-style slab test. Returns the
/// surviving `t`-interval, or `None` if the line misses the rectangle.
fn clip_line_to_rectangle(
    target: &TargetEntity,
    base: Point3<f64>,
    dir: Vector3<f64>,
) -> Option<(f64, f64)> {
    // World -> target-local.
    let world_from_target = target.pose;
    let target_from_world = world_from_target.inverse();
    let base_l = target_from_world * base;
    let dir_l = target_from_world * dir;

    let hw = 0.5 * target.width;
    let hh = 0.5 * target.height;

    let mut t_lo = f64::NEG_INFINITY;
    let mut t_hi = f64::INFINITY;

    // Slab test on local x against [-hw, hw], then local y against [-hh, hh].
    clip_slab(base_l.x, dir_l.x, -hw, hw, &mut t_lo, &mut t_hi)?;
    clip_slab(base_l.y, dir_l.y, -hh, hh, &mut t_lo, &mut t_hi)?;

    if t_hi > t_lo {
        Some((t_lo, t_hi))
    } else {
        None
    }
}

/// Tighten `[t_lo, t_hi]` so the coordinate `p0 + d·t` stays in `[lo, hi]`.
///
/// One axis of a Liang–Barsky slab clip. `None` when the line is parallel to
/// the slab and outside it.
fn clip_slab(p0: f64, d: f64, lo: f64, hi: f64, t_lo: &mut f64, t_hi: &mut f64) -> Option<()> {
    if d.abs() < MIN_NORM {
        // Parallel to this slab: in only if p0 already lies within it.
        if p0 < lo || p0 > hi {
            return None;
        }
        return Some(());
    }
    let inv = 1.0 / d;
    let mut t0 = (lo - p0) * inv;
    let mut t1 = (hi - p0) * inv;
    if t0 > t1 {
        std::mem::swap(&mut t0, &mut t1);
    }
    *t_lo = t_lo.max(t0);
    *t_hi = t_hi.min(t1);
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::laser::width::GaussianBeamWidth;
    use approx::assert_relative_eq;
    use nalgebra::{Isometry3, Translation3, UnitQuaternion};
    use std::f64::consts::FRAC_PI_2;

    /// A laser at `(0, -d, 0)` aimed along world `+y` at the origin, fan in
    /// the world `x = 0` plane (vertical fan), opening about `+y`.
    fn laser_aimed_along_y(standoff: f64, half: f64, length: f64) -> LaserPlane {
        // Local +z must map to world +y: rotate +z onto +y.
        let rot = UnitQuaternion::rotation_between(&Vector3::z(), &Vector3::y())
            .expect("z to y rotation");
        let pose = Isometry3::from_parts(Translation3::new(0.0, -standoff, 0.0), rot);
        let laser = crate::scene::LaserEntity::new(pose, half, length, 660.0, 0.25e-3).unwrap();
        LaserPlane::from_entity(&laser)
    }

    /// A fronto-parallel target rectangle centred at world `(0, 0, 0)` in the
    /// `y = 0` plane (its normal along world `+y`, facing the laser).
    fn target_facing_y(width: f64, height: f64) -> TargetEntity {
        // Local +z onto world +y, so the rectangle lies in the world y = 0
        // plane and its (local x, local y) span world (x, z).
        let rot = UnitQuaternion::rotation_between(&Vector3::z(), &Vector3::y())
            .expect("z to y rotation");
        let pose = Isometry3::from_parts(Translation3::new(0.0, 0.0, 0.0), rot);
        TargetEntity::new(pose, width, height).unwrap()
    }

    fn default_width() -> GaussianBeamWidth {
        GaussianBeamWidth::new(0.25e-3, 660.0e-9).unwrap()
    }

    #[test]
    fn fan_meets_a_facing_target_in_a_vertical_segment() {
        // A vertical fan aimed along +y at a target in the y = 0 plane:
        // the stripe is a vertical segment (varying world z) at world x = 0.
        let laser = laser_aimed_along_y(0.6, 0.3, 1.5);
        let target = target_facing_y(0.4, 0.3);
        let stripe =
            stripe_on_target(&laser, &target, &default_width(), 21).expect("stripe exists");

        assert!(stripe.len() == 21);
        for s in stripe.samples() {
            // The stripe lies in the fan plane (world x = 0).
            assert_relative_eq!(s.point.x, 0.0, epsilon = 1e-9);
            // And on the target plane (world y = 0).
            assert_relative_eq!(s.point.y, 0.0, epsilon = 1e-9);
            // Distance from the laser is finite and positive.
            assert!(s.distance_from_laser_m > 0.0);
            // Width is a positive cross-section diameter.
            assert!(s.width_m > 0.0);
        }
        // The segment spans a range of world z (it is the vertical stripe).
        let (a, b) = stripe.endpoints();
        assert!((a.z - b.z).abs() > 0.01, "stripe should span world z");
    }

    #[test]
    fn stripe_is_clipped_to_the_target_height() {
        // A wide fan would paint a long line, but the target is only 0.2 m
        // tall: the stripe length is bounded by the target height.
        let laser = laser_aimed_along_y(0.6, 0.8, 2.0); // wide fan
        let target = target_facing_y(0.4, 0.2); // short target
        let stripe = stripe_on_target(&laser, &target, &default_width(), 11).unwrap();
        // The stripe runs vertically; clipped to the 0.2 m target height.
        assert!(
            stripe.length_m() <= 0.2 + 1e-6,
            "stripe length {} must not exceed the target height",
            stripe.length_m()
        );
        // It should actually reach close to the full height (the fan is wide
        // enough to over-fill it).
        assert!(stripe.length_m() > 0.19);
    }

    #[test]
    fn stripe_is_clipped_to_the_fan_half_angle() {
        // A narrow fan on a tall target: now the fan, not the target, limits
        // the stripe. Half-angle 0.1 rad at 0.6 m standoff -> the stripe
        // half-length is ~0.6 * tan(0.1) ≈ 0.0602 m, full length ≈ 0.1203 m.
        let laser = laser_aimed_along_y(0.6, 0.1, 2.0); // narrow fan
        let target = target_facing_y(0.4, 1.0); // tall target
        let stripe = stripe_on_target(&laser, &target, &default_width(), 11).unwrap();
        let expected = 2.0 * 0.6 * 0.1_f64.tan();
        assert_relative_eq!(stripe.length_m(), expected, epsilon = 1e-6);
    }

    #[test]
    fn parallel_planes_yield_no_stripe() {
        // Lay the target in a plane parallel to the fan plane (the fan plane
        // is world x = 0; a target also in an x = const plane is parallel).
        let laser = laser_aimed_along_y(0.6, 0.3, 1.5);
        // Target with normal along world +x — parallel to the fan plane.
        let pose = Isometry3::from_parts(
            Translation3::new(0.5, 0.0, 0.0),
            UnitQuaternion::identity(), // local +z = world +z is wrong; need +x
        );
        // Rotate local +z onto world +x so the target plane normal is +x.
        let rot = UnitQuaternion::rotation_between(&Vector3::z(), &Vector3::x()).unwrap();
        let pose = Isometry3::from_parts(pose.translation, rot);
        let target = TargetEntity::new(pose, 0.4, 0.3).unwrap();
        assert!(stripe_on_target(&laser, &target, &default_width(), 11).is_none());
    }

    #[test]
    fn fan_missing_the_target_yields_no_stripe() {
        // Aim the laser along +y but place a small target far off to the side
        // in world x — the vertical fan (x = 0 plane) never reaches it.
        let laser = laser_aimed_along_y(0.6, 0.2, 1.5);
        let rot = UnitQuaternion::rotation_between(&Vector3::z(), &Vector3::y()).unwrap();
        // Target centred at world x = 5 m: far outside the x = 0 fan plane's
        // reach within the rectangle.
        let pose = Isometry3::from_parts(Translation3::new(5.0, 0.0, 0.0), rot);
        let target = TargetEntity::new(pose, 0.2, 0.2).unwrap();
        assert!(stripe_on_target(&laser, &target, &default_width(), 11).is_none());
    }

    #[test]
    fn samples_are_evenly_spaced_along_the_segment() {
        let laser = laser_aimed_along_y(0.6, 0.3, 1.5);
        let target = target_facing_y(0.4, 0.3);
        let stripe = stripe_on_target(&laser, &target, &default_width(), 9).unwrap();
        let s = stripe.samples();
        // Consecutive gaps are equal (the segment is sampled uniformly).
        let gap0 = (s[1].point - s[0].point).norm();
        for i in 1..s.len() - 1 {
            let gap = (s[i + 1].point - s[i].point).norm();
            assert_relative_eq!(gap, gap0, epsilon = 1e-9);
        }
        // Total length is the endpoint distance.
        assert_relative_eq!(
            stripe.length_m(),
            gap0 * (s.len() - 1) as f64,
            epsilon = 1e-9
        );
    }

    #[test]
    fn width_grows_with_distance_from_the_laser() {
        // On an oblique target the two ends of the stripe are at different
        // distances from the laser, so the Gaussian-beam width differs.
        // Tilt the target about world z so its plane is oblique to the fan.
        let laser = laser_aimed_along_y(0.6, 0.5, 2.0);
        let tilt = UnitQuaternion::from_axis_angle(&Vector3::x_axis(), 0.5);
        let base_rot = UnitQuaternion::rotation_between(&Vector3::z(), &Vector3::y()).unwrap();
        let pose = Isometry3::from_parts(Translation3::new(0.0, 0.0, 0.0), tilt * base_rot);
        let target = TargetEntity::new(pose, 0.6, 0.6).unwrap();
        let stripe = stripe_on_target(&laser, &target, &default_width(), 21).unwrap();
        let s = stripe.samples();
        // The sample farthest from the laser must be at least as wide as the
        // nearest one (Gaussian-beam width is monotone in distance).
        let nearest = s
            .iter()
            .min_by(|a, b| a.distance_from_laser_m.total_cmp(&b.distance_from_laser_m))
            .unwrap();
        let farthest = s
            .iter()
            .max_by(|a, b| a.distance_from_laser_m.total_cmp(&b.distance_from_laser_m))
            .unwrap();
        assert!(farthest.width_m >= nearest.width_m);
        // And they genuinely differ — the target is oblique, not fronto-parallel.
        assert!(farthest.distance_from_laser_m > nearest.distance_from_laser_m + 1e-3);
    }

    #[test]
    fn sample_count_is_clamped_up_to_two() {
        let laser = laser_aimed_along_y(0.6, 0.3, 1.5);
        let target = target_facing_y(0.4, 0.3);
        // Asking for 0 or 1 samples still yields a representable segment.
        let stripe = stripe_on_target(&laser, &target, &default_width(), 0).unwrap();
        assert_eq!(stripe.len(), 2);
        let stripe = stripe_on_target(&laser, &target, &default_width(), 1).unwrap();
        assert_eq!(stripe.len(), 2);
    }

    #[test]
    fn line_through_planes_lands_on_both_planes() {
        // Two non-parallel planes: the returned point satisfies both plane
        // equations.
        let pa = Point3::new(1.0, 0.0, 0.0);
        let na = Vector3::new(1.0, 0.0, 0.0);
        let pb = Point3::new(0.0, 2.0, 0.0);
        let nb = Vector3::new(0.0, 1.0, 0.0);
        let p = line_through_planes(pa, na, pb, nb).expect("planes meet");
        assert_relative_eq!(na.dot(&(p - pa)), 0.0, epsilon = 1e-12);
        assert_relative_eq!(nb.dot(&(p - pb)), 0.0, epsilon = 1e-12);
    }

    #[test]
    fn an_oblique_target_still_produces_a_clean_segment() {
        // The M5 checkpoint geometry: a tilted target. The stripe must still
        // be a single straight segment with monotone arc length.
        let laser = laser_aimed_along_y(0.6, 0.4, 2.0);
        let tilt = UnitQuaternion::from_axis_angle(&Vector3::x_axis(), 0.3);
        let base_rot = UnitQuaternion::rotation_between(&Vector3::z(), &Vector3::y()).unwrap();
        let pose = Isometry3::from_parts(Translation3::new(0.0, 0.0, 0.05), tilt * base_rot);
        let target = TargetEntity::new(pose, 0.4, 0.4).unwrap();
        let stripe = stripe_on_target(&laser, &target, &default_width(), 15).unwrap();
        // Collinear: every interior sample lies on the segment between the
        // endpoints.
        let (a, b) = stripe.endpoints();
        let seg = b - a;
        let seg_len2 = seg.norm_squared();
        for s in stripe.samples() {
            let proj = (s.point - a).dot(&seg) / seg_len2;
            let on_line = a + seg * proj;
            assert_relative_eq!(s.point, on_line, epsilon = 1e-9);
        }
    }

    #[test]
    fn solve_quadratic_interval_handles_the_linear_case() {
        // qa = 0, linear qb·t + qc ≤ 0 with qb > 0 -> t ≤ -qc/qb.
        let (lo, hi) = solve_quadratic_interval(0.0, 2.0, -4.0).unwrap();
        assert!(lo.is_infinite() && lo < 0.0);
        assert_relative_eq!(hi, 2.0, epsilon = 1e-12);
    }

    #[test]
    fn a_wide_fan_at_grazing_incidence_is_capped_by_the_radius() {
        // Sanity: even with an arbitrarily wide fan the radial extent caps the
        // stripe. A π/2-ε fan with length 0.5 on a large target — the stripe
        // cannot be longer than the diameter 2·length implied by the disc.
        let laser = laser_aimed_along_y(0.3, FRAC_PI_2 - 0.05, 0.5);
        let target = target_facing_y(2.0, 2.0);
        let stripe = stripe_on_target(&laser, &target, &default_width(), 11).unwrap();
        assert!(stripe.length_m() <= 2.0 * 0.5 + 1e-6);
    }
}
