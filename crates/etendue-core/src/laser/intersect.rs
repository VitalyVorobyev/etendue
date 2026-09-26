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

use nalgebra::{Isometry3, Point3, Vector3};

use crate::geom::TriMesh;
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

/// Compute the laser stripe(s) where a fan plane cuts a triangle mesh.
///
/// Where [`stripe_on_target`] handles a single analytic plane, this handles
/// a general triangle mesh — the per-triangle intersections of the fan
/// plane with every triangle are collected, clipped to the fan extent, and
/// returned as a vector of [`LaserStripe`]s, one per surviving segment.
///
/// # Why a vector rather than a single polyline
///
/// The fan plane can cross a non-convex mesh in **multiple disconnected
/// arcs** (think of a fan crossing a ring-shaped object: the cut produces
/// two arcs, one on each side of the hole). Concatenating them into a
/// single polyline would draw fake edges across the gaps. A `Vec<LaserStripe>`
/// preserves the topology: each surviving segment is its own piecewise-linear
/// run, and the renderer / projection layer can draw or sample each
/// independently. For a convex watertight mesh the typical result is a
/// single-element vector — equivalent to the planar case.
///
/// # The construction
///
/// 1. For every triangle, transform vertices to world (`world_from_mesh`)
///    and compute their signed distances to the fan plane.
/// 2. If the three signed distances are all strictly the same sign, the
///    triangle is wholly on one side of the plane — skip it.
/// 3. Otherwise the plane crosses exactly two edges of the triangle (for a
///    non-degenerate triangle and a non-grazing plane); compute the two
///    edge-crossing points by linear interpolation along each edge.
/// 4. Treat those two points as the endpoints of a finite segment; clip
///    that segment to the fan's angular wedge + radial disc (the same clip
///    [`stripe_on_target`] uses), and sample any surviving sub-segment
///    into `samples_per_segment` evenly-spaced points.
///
/// Per-sample `width_m` comes from `width_model`, as in the planar case.
///
/// # Coordinate frames
///
/// `mesh` is in its own local frame; `world_from_mesh` maps it into world
/// coordinates, where the fan plane lives. The returned samples are in
/// world coordinates.
///
/// # Parameters
///
/// - `laser`: the laser fan, as a [`LaserPlane`] in world coordinates.
/// - `mesh`: the inspection mesh, in its local frame.
/// - `world_from_mesh`: the mesh's pose (`world ← mesh-local`).
/// - `width_model`: cross-section width versus distance (e.g.
///   [`GaussianBeamWidth`](crate::laser::GaussianBeamWidth)).
/// - `samples_per_segment`: samples per surviving sub-segment; clamped up
///   to 2 so each is representable.
#[must_use]
pub fn stripe_segments_on_mesh(
    laser: &LaserPlane,
    mesh: &TriMesh,
    world_from_mesh: &Isometry3<f64>,
    width_model: &dyn WidthModel,
    samples_per_segment: usize,
) -> Vec<LaserStripe> {
    let n_samples = samples_per_segment.max(2);
    let plane_origin = laser.origin();
    let plane_normal = laser.normal();
    let vertices = mesh.vertices();
    let mut stripes = Vec::new();

    for [i0, i1, i2] in mesh.indices().iter().copied() {
        // World-space vertices.
        let v0 = world_from_mesh * vertices[i0 as usize];
        let v1 = world_from_mesh * vertices[i1 as usize];
        let v2 = world_from_mesh * vertices[i2 as usize];

        // Signed distances of each vertex to the fan plane.
        let d0 = plane_normal.dot(&(v0 - plane_origin));
        let d1 = plane_normal.dot(&(v1 - plane_origin));
        let d2 = plane_normal.dot(&(v2 - plane_origin));

        let Some((a, b)) = plane_vs_triangle_segment((v0, d0), (v1, d1), (v2, d2)) else {
            continue;
        };
        if (b - a).norm() < MIN_NORM {
            // Grazing intersection (a single point or numerically degenerate).
            continue;
        }

        // The triangle's intersection segment is a piece of the plane-plane
        // line shared by `stripe_on_target`. Parameterise as base = a,
        // dir = unit(b - a), t ∈ [0, seg_len] for the finite segment.
        let segment = b - a;
        let seg_len = segment.norm();
        let dir = segment / seg_len;

        // Clip the infinite parametric line to the fan extent — angular
        // wedge + radial disc — then intersect with [0, seg_len].
        let Some((fan_lo, fan_hi)) = clip_line_to_fan(laser, a, dir) else {
            continue;
        };
        let t_lo = fan_lo.max(0.0);
        let t_hi = fan_hi.min(seg_len);
        if t_hi <= t_lo {
            continue;
        }

        // Sample this surviving sub-segment.
        let mut samples = Vec::with_capacity(n_samples);
        for i in 0..n_samples {
            let s = i as f64 / (n_samples - 1) as f64;
            let t = t_lo + s * (t_hi - t_lo);
            let point = a + dir * t;
            let distance_from_laser_m = (point - laser.origin()).norm();
            let width_m = width_model.width_at(distance_from_laser_m);
            samples.push(StripeSample {
                point,
                distance_from_laser_m,
                width_m,
            });
        }
        stripes.push(LaserStripe { samples });
    }

    stripes
}

/// Compute the two endpoints of the segment where a plane cuts a triangle,
/// given each vertex's **signed distance** to the plane.
///
/// The plane crosses exactly two edges of a non-degenerate triangle iff the
/// three signed distances do not all share a sign. The crossings are linear
/// interpolations along the edges that straddle the plane.
///
/// Returns `None` when all three signed distances are strictly the same sign
/// (the triangle is wholly on one side of the plane), or when the resulting
/// pair has fewer than two crossings (a grazing case).
fn plane_vs_triangle_segment(
    (v0, d0): (Point3<f64>, f64),
    (v1, d1): (Point3<f64>, f64),
    (v2, d2): (Point3<f64>, f64),
) -> Option<(Point3<f64>, Point3<f64>)> {
    // All three on the same strict side → no crossing.
    if (d0 > 0.0 && d1 > 0.0 && d2 > 0.0) || (d0 < 0.0 && d1 < 0.0 && d2 < 0.0) {
        return None;
    }
    let edges = [
        ((v0, d0), (v1, d1)),
        ((v1, d1), (v2, d2)),
        ((v2, d2), (v0, d0)),
    ];
    let mut crossings: [Option<Point3<f64>>; 2] = [None, None];
    let mut found = 0usize;
    for ((va, da), (vb, db)) in edges.iter().copied() {
        // An edge is crossed if the two signed distances have opposite signs.
        // Equality at exactly one endpoint counts as a crossing at that
        // endpoint; equality at both means the whole edge is on the plane
        // (degenerate — skip, the other two edges' crossings handle it).
        if (da > 0.0 && db < 0.0) || (da < 0.0 && db > 0.0) {
            // Strict sign change — linear interp on the edge.
            let t = da / (da - db);
            let p = va + (vb - va) * t;
            if found < 2 {
                crossings[found] = Some(p);
                found += 1;
            }
        } else if da == 0.0 && db != 0.0 {
            // Edge starts exactly on the plane.
            if found < 2 {
                crossings[found] = Some(va);
                found += 1;
            }
        }
        // An edge with db == 0.0 will be handled by the next edge's da == 0.0.
    }
    if found < 2 {
        return None;
    }
    Some((crossings[0]?, crossings[1]?))
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

    /// Build a unit `TriMesh` in the plane `z = 0` covering
    /// `[-0.5, 0.5] × [-0.5, 0.5]`, split into 2 triangles.
    fn unit_quad_mesh() -> TriMesh {
        use nalgebra::Point3 as P;
        let v = vec![
            P::new(-0.5, -0.5, 0.0),
            P::new(0.5, -0.5, 0.0),
            P::new(0.5, 0.5, 0.0),
            P::new(-0.5, 0.5, 0.0),
        ];
        let n = vec![Vector3::z(); 4];
        let i = vec![[0, 1, 2], [0, 2, 3]];
        TriMesh::new(v, n, i).expect("non-degenerate quad")
    }

    #[test]
    fn mesh_intersection_total_length_matches_planar_target_for_a_flat_quad() {
        // A flat 2-triangle quad-mesh at world y = 0 produces **two** stripes
        // (one per triangle) whose concatenated length equals the analytic-
        // plane target's single stripe length. This is the contract: the API
        // returns per-triangle stripes; the consumer treats them as a piecewise
        // line.
        let laser = laser_aimed_along_y(0.6, 0.3, 1.5);
        let target = target_facing_y(1.0, 1.0); // 1×1 m planar target
        let planar =
            stripe_on_target(&laser, &target, &default_width(), 11).expect("planar stripe exists");

        let mesh = unit_quad_mesh();
        let mesh_pose = target.pose;
        let segments = stripe_segments_on_mesh(&laser, &mesh, &mesh_pose, &default_width(), 11);
        // Both triangles of the quad are cut by the fan plane.
        assert_eq!(segments.len(), 2);
        let mesh_total: f64 = segments.iter().map(LaserStripe::length_m).sum();
        assert_relative_eq!(mesh_total, planar.length_m(), epsilon = 1e-9);
    }

    #[test]
    fn mesh_intersection_endpoints_lie_on_the_fan_plane() {
        // Every sample of every per-triangle stripe must lie on the fan
        // plane (signed distance ~ 0): triangle-plane intersection is exact
        // up to float error.
        let laser = laser_aimed_along_y(0.6, 0.3, 1.5);
        let target = target_facing_y(1.0, 1.0);
        let mesh = unit_quad_mesh();
        let segments = stripe_segments_on_mesh(&laser, &mesh, &target.pose, &default_width(), 5);
        assert!(!segments.is_empty());
        for stripe in &segments {
            for sample in stripe.samples() {
                let d = laser.normal().dot(&(sample.point - laser.origin()));
                assert!(
                    d.abs() < 1e-9,
                    "sample must lie on the fan plane, signed dist = {d}"
                );
            }
        }
    }

    #[test]
    fn mesh_intersection_yields_no_stripes_when_the_mesh_is_behind_the_laser() {
        // Mesh sits at world y = -1.5 (behind the laser which faces +y from
        // y = -0.6). Every triangle is entirely on the negative-y side of
        // the fan plane (the fan plane normal is +x — actually no, the fan
        // opens about +y so its normal is +x in this test setup). Let me
        // instead place the mesh far off-axis where the radial-disc clip
        // throws it out.
        let laser = laser_aimed_along_y(0.6, 0.2, 1.5);
        let mesh = unit_quad_mesh();
        // Mesh at world x = 5 m, facing the laser plane: still in the fan
        // plane (the fan plane is world x = 0), so the cut is a full quad
        // segment — but it's outside the fan's radial extent.
        let rot = UnitQuaternion::rotation_between(&Vector3::z(), &Vector3::y()).unwrap();
        let pose = Isometry3::from_parts(Translation3::new(5.0, 0.0, 0.0), rot);
        let segments = stripe_segments_on_mesh(&laser, &mesh, &pose, &default_width(), 11);
        assert!(
            segments.is_empty(),
            "mesh outside fan extent yields no stripes"
        );
    }

    #[test]
    fn mesh_intersection_skips_a_mesh_parallel_to_the_fan_plane() {
        // A mesh entirely in a plane parallel to the fan plane: every triangle
        // has all 3 vertices at the same signed distance, so no crossings.
        let laser = laser_aimed_along_y(0.6, 0.3, 1.5);
        // Mesh whose normal is also world +x (same as fan plane normal),
        // placed off the fan plane.
        let mesh = unit_quad_mesh();
        let rot = UnitQuaternion::rotation_between(&Vector3::z(), &Vector3::x()).unwrap();
        let pose = Isometry3::from_parts(Translation3::new(0.5, 0.0, 0.0), rot);
        let segments = stripe_segments_on_mesh(&laser, &mesh, &pose, &default_width(), 11);
        assert!(segments.is_empty());
    }

    #[test]
    fn mesh_intersection_cuts_a_single_tilted_triangle() {
        // A single tilted triangle that straddles the fan plane in world x:
        // two vertices at x > 0 and one at x < 0, so the plane crosses two
        // edges. The result is exactly one stripe whose endpoints lie on
        // those edges' intersections.
        use nalgebra::Point3 as P;
        let v = vec![
            P::new(-0.2, 0.0, 0.0),
            P::new(0.2, 0.0, 0.1),
            P::new(0.2, 0.0, -0.1),
        ];
        let n = vec![Vector3::y(); 3];
        let i = vec![[0, 1, 2]];
        let mesh = TriMesh::new(v, n, i).expect("non-degenerate triangle");

        let laser = laser_aimed_along_y(0.5, 0.5, 1.0);
        let pose = Isometry3::identity();
        let segments = stripe_segments_on_mesh(&laser, &mesh, &pose, &default_width(), 11);
        assert_eq!(segments.len(), 1, "exactly one cut for a single triangle");
        let stripe = &segments[0];
        // Both endpoints sit at world x = 0 (on the fan plane).
        let (a, b) = stripe.endpoints();
        assert_relative_eq!(a.x, 0.0, epsilon = 1e-9);
        assert_relative_eq!(b.x, 0.0, epsilon = 1e-9);
        // And both endpoints sit on the triangle's edges:
        // Edge 0→1 from (-0.2, 0, 0) to (0.2, 0, 0.1): at x = 0, z = 0.05.
        // Edge 0→2 from (-0.2, 0, 0) to (0.2, 0, -0.1): at x = 0, z = -0.05.
        // The polyline endpoints are these two points (in some order).
        let zs: Vec<f64> = [a.z, b.z].into_iter().collect();
        let mut zs_sorted = zs.clone();
        zs_sorted.sort_by(|x, y| x.partial_cmp(y).unwrap());
        assert_relative_eq!(zs_sorted[0], -0.05, epsilon = 1e-9);
        assert_relative_eq!(zs_sorted[1], 0.05, epsilon = 1e-9);
    }

    #[test]
    fn mesh_intersection_clamps_sample_count_up_to_two() {
        let laser = laser_aimed_along_y(0.6, 0.3, 1.5);
        let target = target_facing_y(1.0, 1.0);
        let mesh = unit_quad_mesh();
        for samples in [0, 1, 2] {
            let stripes =
                stripe_segments_on_mesh(&laser, &mesh, &target.pose, &default_width(), samples);
            assert!(!stripes.is_empty());
            for s in &stripes {
                assert!(s.len() >= 2, "each stripe must have at least 2 samples");
            }
        }
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
