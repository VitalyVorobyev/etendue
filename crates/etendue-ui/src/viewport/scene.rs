//! Turning `etendue-core` scene entities into renderable geometry.
//!
//! This module is the bridge between the kernel's [`Scene`] — posed cameras,
//! lasers, and targets — and the viewport's GPU primitives. It holds the pure,
//! GPU-free geometry helpers:
//!
//! - [`camera_frustum_edges`] — a [`CameraEntity`]'s view frustum as a list of
//!   line segments;
//! - [`laser_fan_mesh`] — a [`LaserEntity`]'s fan as a thin triangle wedge;
//! - [`target_quad_mesh`] — a [`TargetEntity`]'s plane as a shaded quad;
//! - [`laser_stripe_segments`] — the M5 laser stripe where the fan strikes the
//!   target, as world-space line segments;
//! - [`isometry_to_matrix4`] — the `f64` `Isometry3` → `f32` `Matrix4` cast
//!   used to feed an entity pose to the renderer as a model matrix.
//!
//! Every *entity* geometry helper returns its result in the **entity-local**
//! frame; the caller hands the entity's `pose` to the renderer as the model
//! matrix, so the GPU does the local→world transform. This keeps the entity
//! pose in one place and gives M3 a cheap re-pose path
//! (`Drawable::set_transform`). The laser stripe is the exception — it is a
//! *derived* curve spanning two entities, so [`laser_stripe_segments`] returns
//! it directly in **world** coordinates (drawn with an identity model matrix,
//! like the ground grid).
//!
//! `renderer.rs` consumes these helpers in `build_scene`, wrapping each result
//! in a `Drawable`. The actual GPU upload and the (private) `Drawable` type
//! stay in `renderer.rs`.

use etendue_core::analysis::WorkingVolume;
use etendue_core::geom::TriMesh;
use etendue_core::laser::{GaussianBeamWidth, LaserPlane, WidthModel, stripe_on_target};
use etendue_core::optics::sensor_distance;
use etendue_core::scene::{CameraEntity, LaserEntity, Scene, TargetEntity};
use nalgebra::{Isometry3, Matrix4, Point2, Point3, Vector3};

/// Number of polyline samples for the rendered 3D laser stripe.
///
/// The MVP stripe is a single straight segment, so two points would suffice
/// geometrically; a handful more keeps the line-list builder identical to the
/// general (post-MVP) piecewise-linear case at negligible cost.
const STRIPE_SAMPLES: usize = 24;

/// How far the rendered 3D stripe is lifted off the target surface, in metres.
///
/// The stripe is geometrically *coplanar* with the target quad (and the M4
/// heatmap grid). Drawn at exactly equal depth it would z-fight the surface
/// under the renderer's `Less` depth test. A sub-millimetre lift along the
/// target's outward normal — toward the laser/camera — keeps the line
/// reliably in front without visibly detaching it from the surface.
const STRIPE_SURFACE_LIFT_M: f64 = 5.0e-4;

/// How far the M6 working-volume patch is lifted off the laser fan plane,
/// in metres.
///
/// The working-volume mesh is geometrically coplanar with the rendered laser
/// fan: both lie in the fan plane (the local `x = 0` plane of the laser).
/// The fan is drawn translucent (depth-test on, depth-write off), and the
/// working-volume mesh is also translucent — at exactly equal depth, the
/// blend order between two depth-coincident translucent surfaces is unstable
/// and the patch may flicker behind the fan. A sub-millimetre lift along the
/// fan plane's normal **toward the camera** places the working-volume patch
/// reliably in front of the fan from the orbit camera's viewpoint without
/// visibly detaching it from the fan surface — the same pattern M5
/// established for the laser stripe.
const WORKING_VOLUME_LIFT_M: f64 = 5.0e-4;

/// Convert an `f64` [`Isometry3`] pose into the `f32` [`Matrix4`] the renderer
/// uses as a model matrix.
///
/// The kernel is `f64`; GPU model matrices are `f32`. This is the one place
/// that narrowing happens for entity poses — the analogue of `mesh.rs`'s
/// vertex narrowing. The conversion goes through `Isometry3::to_homogeneous`,
/// so a pure rotation+translation stays a well-formed affine matrix.
#[must_use]
pub fn isometry_to_matrix4(pose: &Isometry3<f64>) -> Matrix4<f32> {
    pose.to_homogeneous().cast::<f32>()
}

/// The 8 corners of a [`CameraEntity`]'s view frustum, in **camera-local**
/// coordinates.
///
/// The camera looks down local `+z`. For each of the four image-corner pixels
/// — `(0,0)`, `(w,0)`, `(w,h)`, `(0,h)` — the camera model is back-projected
/// to a ray; `Camera::backproject_pixel` returns that ray as a point on the
/// local `z = 1` plane. Scaling that point by the frustum `near` and `far`
/// distances gives the near-plane and far-plane corner. The result is ordered
/// `[near tl, near tr, near br, near bl, far tl, far tr, far br, far bl]`.
///
/// Returns `None` if any back-projected ray does not have a positive `z`
/// component (it cannot be scaled onto a `z = const` plane in front of the
/// camera) — e.g. an extreme distortion that folds a corner behind the lens.
fn frustum_corners(camera: &CameraEntity) -> Option<[Point3<f64>; 8]> {
    let model = camera.params.build();
    let (w, h) = camera.resolution_f64();

    // Image corners, traversed tl → tr → br → bl so the edge lists below wind
    // consistently. Pixel (0,0) is the top-left corner with image-y-down.
    let corner_px = [
        Point2::new(0.0, 0.0),
        Point2::new(w, 0.0),
        Point2::new(w, h),
        Point2::new(0.0, h),
    ];

    let mut near = [Point3::origin(); 4];
    let mut far = [Point3::origin(); 4];
    for (i, px) in corner_px.iter().enumerate() {
        // `Ray.point` is the corner direction expressed on the z = 1 plane.
        let on_z1 = model.backproject_pixel(px).point;
        // It must lie in front of the camera to scale onto near/far planes.
        if !(on_z1.z.is_finite() && on_z1.z > 0.0) {
            return None;
        }
        // The point already has z = 1, so multiplying by a distance places it
        // on the z = distance plane while keeping the same view direction.
        near[i] = Point3::from(on_z1 * camera.frustum_near);
        far[i] = Point3::from(on_z1 * camera.frustum_far);
    }

    Some([
        near[0], near[1], near[2], near[3], far[0], far[1], far[2], far[3],
    ])
}

/// A [`CameraEntity`]'s view frustum as colored line segments in
/// **camera-local** coordinates.
///
/// Twelve edges: the near rectangle, the far rectangle, and the four side
/// edges joining them. Apply the camera's `pose` (as a model matrix) to place
/// them in the world.
///
/// Returns an empty `Vec` if the frustum is degenerate (see
/// [`frustum_corners`]); the camera then contributes no drawn geometry rather
/// than panicking.
#[must_use]
pub fn camera_frustum_edges(
    camera: &CameraEntity,
    color: [f32; 3],
) -> Vec<(Point3<f64>, Point3<f64>, [f32; 3])> {
    let Some(c) = frustum_corners(camera) else {
        return Vec::new();
    };
    // Corner indices: 0..4 near rectangle (tl,tr,br,bl), 4..8 far rectangle.
    let edges: [(usize, usize); 12] = [
        // Near rectangle.
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        // Far rectangle.
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        // Side edges near→far.
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ];
    edges.iter().map(|&(a, b)| (c[a], c[b], color)).collect()
}

/// A double-sided rectangular sheet at a given local `z`, in **camera-local**
/// coordinates.
///
/// Used by the M8 camera-anatomy helpers ([`camera_imager_mesh`],
/// [`camera_principal_plane_meshes`]) to render the sensor plane and the two
/// principal planes as flat translucent quads. Two coplanar triangles per face,
/// one face normal `+z` (object side), the other `-z` (sensor side), so the
/// sheet shades consistently whichever side the orbit camera is on (the
/// renderer disables back-face culling, but a single normal would still leave
/// one face dark under the directional light).
fn double_sided_rect_mesh(half_w: f64, half_h: f64, z: f64) -> TriMesh {
    let tl = Point3::new(-half_w, half_h, z);
    let tr = Point3::new(half_w, half_h, z);
    let br = Point3::new(half_w, -half_h, z);
    let bl = Point3::new(-half_w, -half_h, z);
    let plus_z = Vector3::z();
    let minus_z = -plus_z;
    // Two distinct vertex blocks (one per face) so each carries the correct
    // per-vertex normal — `TriMesh` is a per-vertex-normal mesh, so sharing
    // vertices between front and back would average to a zero normal.
    let vertices = vec![bl, br, tr, tl, bl, tl, tr, br];
    let normals = vec![
        plus_z, plus_z, plus_z, plus_z, minus_z, minus_z, minus_z, minus_z,
    ];
    let indices = vec![[0, 1, 2], [0, 2, 3], [4, 5, 6], [4, 6, 7]];
    TriMesh::new(vertices, normals, indices)
        .expect("double-sided rect is non-degenerate for positive half-extents")
}

/// The image sensor plane as a translucent rectangle, in **camera-local**
/// coordinates.
///
/// Placed at `z = -v0` where `v0 = sensor_distance(s_o, g, f)` is the on-axis
/// image-side conjugate of the focus plane (see
/// [`etendue_core::optics::sensor_distance`]). The rectangle is sized to the
/// physical sensor extent (`resolution * pixel_pitch_m`).
///
/// Currently rendered **fronto-parallel** — the Scheimpflug tilt is not
/// applied to the visualization. The tilt is captured by the projection model
/// and the M4 plane-of-best-focus; rendering a tilted sensor here is a
/// follow-up.
///
/// Returns `None` when `sensor_distance` cannot be computed (focus distance
/// at/inside the focal point); the caller then adds no sensor drawable.
#[must_use]
pub fn camera_imager_mesh(camera: &CameraEntity) -> Option<TriMesh> {
    let v0 = sensor_distance(
        camera.focus_distance_m,
        camera.principal_gap_m,
        camera.effective_focal_length_m,
    )
    .ok()?;
    let (rx, ry) = camera.resolution_f64();
    let half_w = 0.5 * rx * camera.pixel_pitch_m;
    let half_h = 0.5 * ry * camera.pixel_pitch_m;
    Some(double_sided_rect_mesh(half_w, half_h, -v0))
}

/// The two principal planes `H` and `H'` as translucent squares, in
/// **camera-local** coordinates.
///
/// `H'` sits at the camera-local origin (`z = 0`); `H` at `z = -principal_gap_m`
/// — the convention adopted by the M4 CoC physics (an object at camera depth
/// `Z` is at object distance `Z + g` from `H`, see
/// [`etendue_core::optics::coc::coc_diameter_at_sensor`]). Returns the pair as
/// `(h_mesh, h_prime_mesh)`.
///
/// Each plane is sized as a square `1.4 × aperture_diameter` on a side so the
/// aperture ring sits visually inside it. For a thin lens (`g = 0`) the two
/// planes coincide; the renderer is free to draw both — only one will be
/// visible.
#[must_use]
pub fn camera_principal_plane_meshes(camera: &CameraEntity) -> (TriMesh, TriMesh) {
    let aperture_diam = camera.effective_focal_length_m / camera.f_number;
    let half_side = 0.5 * 1.4 * aperture_diam;
    let g = camera.principal_gap_m;
    let h = double_sided_rect_mesh(half_side, half_side, -g);
    let h_prime = double_sided_rect_mesh(half_side, half_side, 0.0);
    (h, h_prime)
}

/// The lens aperture as a colored line-list ring, in **camera-local**
/// coordinates.
///
/// Drawn as a 48-segment polygon approximating a circle of diameter
/// `effective_focal_length_m / f_number` (the geometric aperture diameter).
/// Centred on the optical axis at `z = -principal_gap_m / 2` — midway between
/// `H` (`z = -g`) and `H'` (`z = 0`), where the physical aperture stop of a
/// thick lens conventionally sits. For a thin lens (`g = 0`) the ring lies in
/// the plane of the coincident principal planes.
#[must_use]
pub fn camera_aperture_ring(
    camera: &CameraEntity,
    color: [f32; 3],
) -> Vec<(Point3<f64>, Point3<f64>, [f32; 3])> {
    const SEGMENTS: usize = 48;
    let radius = 0.5 * camera.effective_focal_length_m / camera.f_number;
    let z = -0.5 * camera.principal_gap_m;
    (0..SEGMENTS)
        .map(|i| {
            let t0 = (i as f64 / SEGMENTS as f64) * std::f64::consts::TAU;
            let t1 = ((i + 1) as f64 / SEGMENTS as f64) * std::f64::consts::TAU;
            (
                Point3::new(radius * t0.cos(), radius * t0.sin(), z),
                Point3::new(radius * t1.cos(), radius * t1.sin(), z),
                color,
            )
        })
        .collect()
}

/// Number of axial samples (rings) used to tessellate the M8 laser-fan ribbon.
///
/// Each ring contributes two vertices per sheet (at the negative-y and
/// positive-y edge of the fan); 24 rings give the Gaussian thickness a smooth,
/// visible curve at typical scene scales without producing a heavy mesh
/// (≈ 4·N − 2 triangles per fan = 94 triangles).
const LASER_FAN_AXIAL_SAMPLES: usize = 24;

/// A [`LaserEntity`]'s fan as a thickness-varying ribbon, in **laser-local**
/// coordinates.
///
/// The fan opens about the laser-local `+z` axis within the `y–z` plane: the
/// apex sits at the local origin, and the two far corners at
/// `±fan_half_angle` from `+z`, a `fan_length` along each edge ray. The
/// rendered ribbon extrudes that fan triangle along the laser-local `±x` axis
/// with a half-thickness `t(s) = GaussianBeamWidth::width_at(s) / 2`, where
/// `s` is the radial distance from the apex (the laser waist sits at the
/// apex). The thickness reflects the depth-varying beam width: the ribbon is
/// narrowest at the apex (where `t = w₀`) and widens with distance.
///
/// Topology: two parallel sheets, one at `x = +t(s)` (normal `+x`) and one at
/// `x = -t(s)` (normal `-x`). Each sheet is a triangle fan from the apex to
/// `LASER_FAN_AXIAL_SAMPLES` axial rings, so adjacent rings are connected by
/// two triangles each (and the apex by one), producing
/// `2 · (2N − 1)` triangles total. The two sheets do not connect at the side
/// edges — the ribbon is open along its rim — so the orbit camera sees the
/// divergence as two slightly diverging films.
///
/// # Panics
///
/// Does not panic for a valid [`LaserEntity`]: `LaserEntity::new` guarantees a
/// finite half-angle in `(0, π/2)`, a finite positive length, a positive
/// wavelength, and a positive beam waist, which together make the ribbon
/// non-degenerate and satisfy `TriMesh::new`'s invariants.
#[must_use]
pub fn laser_fan_mesh(laser: &LaserEntity) -> TriMesh {
    let width = GaussianBeamWidth::from_laser(laser)
        .expect("GaussianBeamWidth cannot fail for a valid LaserEntity");
    let n = LASER_FAN_AXIAL_SAMPLES;
    let (sin_a, cos_a) = laser.fan_half_angle.sin_cos();
    let l = laser.fan_length;

    let s_of = |i: usize| (i as f64 / n as f64) * l;
    let t_of = |s: f64| 0.5 * width.width_at(s);

    let plus_x = Vector3::x();
    let minus_x = -plus_x;
    let mut vertices: Vec<Point3<f64>> = Vec::with_capacity(4 * n + 2);
    let mut normals: Vec<Vector3<f64>> = Vec::with_capacity(4 * n + 2);
    let mut indices: Vec<[u32; 3]> = Vec::with_capacity(4 * n - 2);

    // Build one sheet (front or back) at x = sign · t(s), with the given
    // per-vertex shading normal. Returns the vertex indices of the rings so
    // the caller can stitch the apex and trapezoid triangles.
    let mut emit_sheet = |sign: f64, normal: Vector3<f64>| {
        let apex_idx = vertices.len() as u32;
        vertices.push(Point3::new(sign * t_of(0.0), 0.0, 0.0));
        normals.push(normal);
        let mut rings: Vec<(u32, u32)> = Vec::with_capacity(n);
        for i in 1..=n {
            let s = s_of(i);
            let t = t_of(s);
            let y_ext = s * sin_a;
            let z = s * cos_a;
            let neg = vertices.len() as u32;
            vertices.push(Point3::new(sign * t, -y_ext, z));
            normals.push(normal);
            let pos = vertices.len() as u32;
            vertices.push(Point3::new(sign * t, y_ext, z));
            normals.push(normal);
            rings.push((neg, pos));
        }
        (apex_idx, rings)
    };

    // Front sheet (normal +x).
    let (front_apex, front_rings) = emit_sheet(1.0, plus_x);
    indices.push([front_apex, front_rings[0].0, front_rings[0].1]);
    for i in 0..(n - 1) {
        let (a_neg, a_pos) = front_rings[i];
        let (b_neg, b_pos) = front_rings[i + 1];
        indices.push([a_neg, a_pos, b_pos]);
        indices.push([a_neg, b_pos, b_neg]);
    }

    // Back sheet (normal -x), reversed winding.
    let (back_apex, back_rings) = emit_sheet(-1.0, minus_x);
    indices.push([back_apex, back_rings[0].1, back_rings[0].0]);
    for i in 0..(n - 1) {
        let (a_neg, a_pos) = back_rings[i];
        let (b_neg, b_pos) = back_rings[i + 1];
        indices.push([a_neg, b_pos, a_pos]);
        indices.push([a_neg, b_neg, b_pos]);
    }

    TriMesh::new(vertices, normals, indices)
        .expect("laser fan ribbon is non-degenerate for a valid LaserEntity")
}

/// A [`TargetEntity`]'s plane as a shaded quad, in **target-local**
/// coordinates.
///
/// The target rectangle already lives in the target-local `z = 0` plane with
/// its normal along local `+z` — exactly [`TriMesh::quad`]'s convention — so
/// this is a thin wrapper that sizes the quad by the entity's extent. Apply
/// the target's `pose` (as a model matrix) to place it in the world.
///
/// # Panics
///
/// Does not panic for a valid [`TargetEntity`]: `TargetEntity::new` guarantees
/// a finite positive width and height, which is exactly what `TriMesh::quad`
/// requires.
#[must_use]
pub fn target_quad_mesh(target: &TargetEntity) -> TriMesh {
    TriMesh::quad(target.width, target.height)
}

/// All M5 laser stripes — every laser × every target intersection — as
/// colored line segments in **world** coordinates.
///
/// For an M10 multi-pair rig, each laser independently strikes the shared
/// target(s), so the rendered geometry is the *union* of all stripe
/// polylines. For a single-pair scene this reduces to the original M5
/// behaviour (one stripe). Missing intersections, missing entities, and
/// invalid beam-width models are silently skipped — each pair contributes
/// segments only if its stripe exists.
///
/// Unlike the entity-local helpers, the result is already in world
/// coordinates — each stripe spans the laser *and* the target, so it cannot
/// live in a single entity's local frame. The caller draws the whole vec
/// with an identity model matrix.
#[must_use]
pub fn laser_stripe_segments(
    scene: &Scene,
    color: [f32; 3],
) -> Vec<(Point3<f64>, Point3<f64>, [f32; 3])> {
    let mut out = Vec::new();
    for laser in &scene.lasers {
        let plane = LaserPlane::from_entity(laser);
        let Ok(width_model) = GaussianBeamWidth::from_laser(laser) else {
            continue;
        };
        for target in &scene.targets {
            let Some(stripe) = stripe_on_target(&plane, target, &width_model, STRIPE_SAMPLES)
            else {
                continue;
            };
            // Lift the stripe a hair off the target surface so it does not
            // z-fight the coplanar target quad / heatmap grid. The lift is
            // along the target's outward normal toward the laser side (the
            // laser illuminates the front face, so the dot-product sign
            // picks the correct direction).
            let mut surface_normal = target.pose * Vector3::z();
            let to_laser = (plane.origin() - (target.pose * Point3::origin())).normalize();
            if surface_normal.dot(&to_laser) < 0.0 {
                surface_normal = -surface_normal;
            }
            let lift = surface_normal * STRIPE_SURFACE_LIFT_M;

            let samples = stripe.samples();
            out.extend(
                samples
                    .windows(2)
                    .map(|w| (w[0].point + lift, w[1].point + lift, color)),
            );
        }
    }
    out
}

/// RGB color of the working-volume patch — a saturated cyan/teal.
///
/// Deliberately distinct from both the laser fan red and the heatmap
/// green/yellow/red so a designer can tell at a glance whether they are
/// looking at the laser fan, the on-target focus heatmap, or the
/// working-volume patch on the fan plane.
pub const WORKING_VOLUME_COLOR: [f32; 3] = [0.20, 0.85, 0.95];

/// Alpha of the translucent working-volume patch.
///
/// On the same scale as the laser fan's alpha so the patch and the fan blend
/// coherently on the (effectively coplanar) fan plane — the patch reads as a
/// stronger highlight on top of the fan rather than a solid overlay.
pub const WORKING_VOLUME_ALPHA: f32 = 0.45;

/// The M6 working volume as a triangle mesh on the laser fan plane, in
/// **world** coordinates.
///
/// Builds one mesh covering exactly the in-volume cells of `wv`. Each
/// in-volume grid cell contributes two CCW-wound triangles; cells outside
/// the working volume contribute no triangles, so the irregular shape of
/// the patch is the patch's mesh footprint — there is no per-vertex masking.
///
/// The mesh sits a sub-millimetre off the fan plane along the plane's
/// **camera-side** normal (see [`WORKING_VOLUME_LIFT_M`]): both the fan and
/// the working-volume patch are translucent and lie in the same plane, so
/// lifting the patch toward the orbit camera breaks the depth tie and keeps
/// the patch visible in front of the fan.
///
/// The mesh is laid out so the renderer can use it through the existing
/// `Mesh::Translucent` path: a single flat colour ([`WORKING_VOLUME_COLOR`])
/// with the per-draw tint alpha set to [`WORKING_VOLUME_ALPHA`]. Two
/// triangles per cell are added with **opposite** winding/normals so the
/// patch shades from either side of the fan (the orbit camera can be on
/// either side).
///
/// Returns `None` when there are no in-volume cells — the caller then simply
/// adds no patch drawable.
///
/// `camera_position` is used purely to pick which side of the fan plane to
/// lift the patch toward; the renderer does not see it directly.
#[must_use]
pub fn working_volume_mesh(
    wv: &WorkingVolume,
    laser: &LaserEntity,
    camera_position: Point3<f64>,
) -> Option<TriMesh> {
    let rows = wv.rows();
    let cols = wv.cols();
    if rows < 2 || cols < 2 {
        return None;
    }

    let plane = LaserPlane::from_entity(laser);
    let to_camera = camera_position - plane.origin();
    let lift_sign = if plane.normal().dot(&to_camera) >= 0.0 {
        1.0
    } else {
        -1.0
    };
    let lift = plane.normal() * (WORKING_VOLUME_LIFT_M * lift_sign);
    // Normals for the two-sided patch: one face along +lift, one along -lift,
    // so the patch shades consistently whichever side the orbit camera is
    // on (the renderer disables back-face culling, but a single normal would
    // still leave one face dark under the directional light).
    let front_normal = plane.normal() * lift_sign;
    let back_normal = -front_normal;

    // Build the mesh: for each in-volume cell add two pairs of CCW/CW
    // triangles with opposite normals. The vertex list is built fresh
    // (rather than indexing into a shared per-cell vertex list) so the
    // triangle's normal is consistent with the vertex's normal — `TriMesh`
    // is a per-vertex normal mesh, and sharing vertices between front and
    // back faces would average the normals.
    let mut vertices: Vec<Point3<f64>> = Vec::new();
    let mut normals: Vec<Vector3<f64>> = Vec::new();
    let mut indices: Vec<[u32; 3]> = Vec::new();

    // Helper: push a single triangle with a given face normal.
    let mut emit_tri = |a: Point3<f64>, b: Point3<f64>, c: Point3<f64>, n: Vector3<f64>| {
        let base = vertices.len() as u32;
        vertices.push(a);
        vertices.push(b);
        vertices.push(c);
        normals.push(n);
        normals.push(n);
        normals.push(n);
        indices.push([base, base + 1, base + 2]);
    };

    // Direct slice indexing: all (row, col) pairs with row < rows-1, col < cols-1
    // are provably in bounds, so we skip the Option round-trip from wv.get().
    let cells = wv.cells();
    let at =
        |r: usize, c: usize| -> &etendue_core::analysis::WorkingVolumeCell { &cells[r * cols + c] };

    for row in 0..rows - 1 {
        for col in 0..cols - 1 {
            let c00 = at(row, col).in_volume;
            let c01 = at(row, col + 1).in_volume;
            let c10 = at(row + 1, col).in_volume;
            let c11 = at(row + 1, col + 1).in_volume;
            // Skip cells with no in-volume corner. The patch's silhouette is
            // a step function on the grid; corners-as-OR keeps the patch's
            // visible footprint slightly **larger** than its strictly
            // interior cells, which matches the area-estimator's "any
            // corner is enough" convention (see `WorkingVolume::area_m2`).
            if !(c00 || c01 || c10 || c11) {
                continue;
            }
            let tl = at(row, col).point_world + lift;
            let tr = at(row, col + 1).point_world + lift;
            let bl = at(row + 1, col).point_world + lift;
            let br = at(row + 1, col + 1).point_world + lift;
            // Front face (CCW from front_normal side): (tl, tr, br), (tl, br, bl).
            emit_tri(tl, tr, br, front_normal);
            emit_tri(tl, br, bl, front_normal);
            // Back face (reverse winding, opposite normal).
            emit_tri(tl, br, tr, back_normal);
            emit_tri(tl, bl, br, back_normal);
        }
    }

    if indices.is_empty() {
        return None;
    }

    TriMesh::new(vertices, normals, indices).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use etendue_core::scene::Scene;
    // The calibration kernel is reached through `etendue-core`'s re-export
    // (`etendue_core::calibration`): `etendue-ui` does not depend on
    // `vision-calibration-core` directly.
    use etendue_core::calibration::{
        CameraParams, DistortionParams, FxFyCxCySkew, IntrinsicsParams, ProjectionParams,
        SensorParams,
    };

    /// A pinhole camera entity with a centred principal point: its frustum is
    /// then symmetric, which makes the corner geometry easy to assert.
    ///
    /// The physical optics parameters are chosen so the derived focal length
    /// is exactly `fx = fy = focal / pitch = 0.005 / 5e-6 = 1000` px — the
    /// value the frustum-corner assertions below were written against.
    fn centred_camera() -> CameraEntity {
        let params = CameraParams {
            projection: ProjectionParams::Pinhole,
            distortion: DistortionParams::None,
            sensor: SensorParams::Identity,
            intrinsics: IntrinsicsParams::FxFyCxCySkew {
                params: FxFyCxCySkew {
                    // Overwritten by `CameraEntity::new` from focal/pitch.
                    fx: 0.0,
                    fy: 0.0,
                    cx: 640.0,
                    cy: 360.0,
                    skew: 0.0,
                },
            },
        };
        // f = 5 mm, f/4, focused at 0.5 m, thin lens, 5 um pixel pitch
        // => fx = fy = 0.005 / 5e-6 = 1000 px.
        CameraEntity::new(
            Isometry3::identity(),
            params,
            (1280, 720),
            0.005,
            4.0,
            0.5,
            0.0,
            5e-6,
            0.2,
            1.0,
        )
        .expect("valid camera entity")
    }

    #[test]
    fn isometry_to_matrix4_preserves_translation() {
        let pose = Isometry3::translation(1.0, -2.0, 3.0);
        let m = isometry_to_matrix4(&pose);
        // The translation lands in the fourth column of a homogeneous matrix.
        assert_relative_eq!(m[(0, 3)], 1.0, epsilon = 1e-6);
        assert_relative_eq!(m[(1, 3)], -2.0, epsilon = 1e-6);
        assert_relative_eq!(m[(2, 3)], 3.0, epsilon = 1e-6);
        assert_relative_eq!(m[(3, 3)], 1.0, epsilon = 1e-6);
    }

    #[test]
    fn frustum_has_twelve_edges() {
        let edges = camera_frustum_edges(&centred_camera(), [1.0, 1.0, 1.0]);
        assert_eq!(edges.len(), 12);
    }

    #[test]
    fn frustum_corners_sit_on_the_near_and_far_planes() {
        let cam = centred_camera();
        let corners = frustum_corners(&cam).expect("frustum corners");
        // First four corners on the near plane, last four on the far plane.
        for near in &corners[0..4] {
            assert_relative_eq!(near.z, cam.frustum_near, epsilon = 1e-9);
        }
        for far in &corners[4..8] {
            assert_relative_eq!(far.z, cam.frustum_far, epsilon = 1e-9);
        }
    }

    #[test]
    fn frustum_principal_point_is_centred_for_a_centred_camera() {
        // With cx,cy at the image centre, opposite frustum corners are mirror
        // images through the optical axis: their x and y sum to zero.
        let corners = frustum_corners(&centred_camera()).expect("frustum corners");
        // Near top-left (0) and near bottom-right (2) are diagonally opposite.
        assert_relative_eq!(corners[0].x + corners[2].x, 0.0, epsilon = 1e-9);
        assert_relative_eq!(corners[0].y + corners[2].y, 0.0, epsilon = 1e-9);
    }

    #[test]
    fn frustum_far_rectangle_is_wider_than_the_near_rectangle() {
        // A perspective frustum widens with depth: the far corners are
        // farther off-axis than the near corners.
        let corners = frustum_corners(&centred_camera()).expect("frustum corners");
        let near_extent = corners[0].x.abs();
        let far_extent = corners[4].x.abs();
        assert!(
            far_extent > near_extent,
            "far extent {far_extent} should exceed near extent {near_extent}"
        );
    }

    #[test]
    fn camera_imager_lies_at_minus_sensor_distance() {
        // With g=0 and a far-field focus, v0 ≈ f to better than 0.1 %, so the
        // imager rectangle sits at z ≈ -f. We just assert it matches the
        // exact `sensor_distance` value to within float epsilon.
        let cam = centred_camera();
        let mesh = camera_imager_mesh(&cam).expect("imager mesh for a valid lens");
        let v0 = etendue_core::optics::sensor_distance(
            cam.focus_distance_m,
            cam.principal_gap_m,
            cam.effective_focal_length_m,
        )
        .expect("sensor_distance for a valid lens");
        for v in mesh.vertices() {
            assert_relative_eq!(v.z, -v0, epsilon = 1e-12);
        }
    }

    #[test]
    fn camera_imager_size_matches_resolution_times_pixel_pitch() {
        let cam = centred_camera();
        let mesh = camera_imager_mesh(&cam).expect("imager mesh");
        let (rx, ry) = cam.resolution_f64();
        let half_w = 0.5 * rx * cam.pixel_pitch_m;
        let half_h = 0.5 * ry * cam.pixel_pitch_m;
        for v in mesh.vertices() {
            assert_relative_eq!(v.x.abs(), half_w, epsilon = 1e-12);
            assert_relative_eq!(v.y.abs(), half_h, epsilon = 1e-12);
        }
    }

    #[test]
    fn principal_plane_meshes_are_separated_by_the_inter_principal_gap() {
        // For a thick-lens camera with g > 0, the H mesh sits at z = -g and the
        // H' mesh at z = 0. Build a camera with non-zero gap and check.
        let g = 5e-3; // 5 mm gap
        let params = CameraParams {
            projection: ProjectionParams::Pinhole,
            distortion: DistortionParams::None,
            sensor: SensorParams::Identity,
            intrinsics: IntrinsicsParams::FxFyCxCySkew {
                params: FxFyCxCySkew {
                    fx: 0.0,
                    fy: 0.0,
                    cx: 640.0,
                    cy: 360.0,
                    skew: 0.0,
                },
            },
        };
        let cam = CameraEntity::new(
            Isometry3::identity(),
            params,
            (1280, 720),
            0.025,
            4.0,
            0.3,
            g,
            5e-6,
            0.1,
            1.0,
        )
        .expect("valid thick-lens camera");
        let (h, h_prime) = camera_principal_plane_meshes(&cam);
        for v in h.vertices() {
            assert_relative_eq!(v.z, -g, epsilon = 1e-12);
        }
        for v in h_prime.vertices() {
            assert_relative_eq!(v.z, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn aperture_ring_has_radius_half_f_over_n() {
        let cam = centred_camera();
        let segments = camera_aperture_ring(&cam, [1.0, 1.0, 0.0]);
        let expected_radius = 0.5 * cam.effective_focal_length_m / cam.f_number;
        // 48 segments → 48 line segments.
        assert_eq!(segments.len(), 48);
        for (a, b, _) in &segments {
            let ra = (a.x * a.x + a.y * a.y).sqrt();
            let rb = (b.x * b.x + b.y * b.y).sqrt();
            assert_relative_eq!(ra, expected_radius, epsilon = 1e-12);
            assert_relative_eq!(rb, expected_radius, epsilon = 1e-12);
        }
    }

    #[test]
    fn aperture_ring_z_is_midway_between_principal_planes() {
        // For a thick lens, the aperture sits at z = -g/2.
        let g = 4e-3;
        let params = CameraParams {
            projection: ProjectionParams::Pinhole,
            distortion: DistortionParams::None,
            sensor: SensorParams::Identity,
            intrinsics: IntrinsicsParams::FxFyCxCySkew {
                params: FxFyCxCySkew {
                    fx: 0.0,
                    fy: 0.0,
                    cx: 640.0,
                    cy: 360.0,
                    skew: 0.0,
                },
            },
        };
        let cam = CameraEntity::new(
            Isometry3::identity(),
            params,
            (1280, 720),
            0.025,
            4.0,
            0.3,
            g,
            5e-6,
            0.1,
            1.0,
        )
        .expect("valid thick-lens camera");
        let segments = camera_aperture_ring(&cam, [1.0; 3]);
        for (a, _, _) in &segments {
            assert_relative_eq!(a.z, -0.5 * g, epsilon = 1e-12);
        }
    }

    #[test]
    fn aperture_ring_closes_on_itself() {
        // The last segment's endpoint must coincide with the first segment's
        // start — the ring is a closed polygon.
        let cam = centred_camera();
        let segments = camera_aperture_ring(&cam, [1.0; 3]);
        let first_start = segments.first().expect("ring is non-empty").0;
        let last_end = segments.last().expect("ring is non-empty").1;
        assert_relative_eq!(first_start.x, last_end.x, epsilon = 1e-12);
        assert_relative_eq!(first_start.y, last_end.y, epsilon = 1e-12);
    }

    #[test]
    fn laser_fan_mesh_is_a_thick_ribbon() {
        // The M8 ribbon has `2 · (2N − 1)` triangles where N is the axial
        // sample count; with N = 24 that's 94 triangles.
        let laser = LaserEntity::new(Isometry3::identity(), 0.25, 1.2, 660.0, 0.25e-3).unwrap();
        let mesh = laser_fan_mesh(&laser);
        let expected = 2 * (2 * super::LASER_FAN_AXIAL_SAMPLES as i32 - 1);
        assert_eq!(mesh.triangle_count() as i32, expected);
        assert!(
            mesh.vertices().len() > 6,
            "ribbon must have more vertices than the old 6-vertex flat fan, got {}",
            mesh.vertices().len()
        );
    }

    #[test]
    fn laser_fan_apex_thickness_is_the_beam_waist_diameter() {
        // At the apex (s = 0) the half-thickness is `width_at(0) / 2 = w₀`,
        // so the apex straddles `x = ±w₀`. Every vertex with `y = z = 0` is
        // an apex vertex.
        let w0 = 0.25e-3;
        let laser = LaserEntity::new(Isometry3::identity(), 0.25, 1.2, 660.0, w0).unwrap();
        let mesh = laser_fan_mesh(&laser);
        let apex_vertices: Vec<_> = mesh
            .vertices()
            .iter()
            .filter(|v| v.y.abs() < 1e-12 && v.z.abs() < 1e-12)
            .collect();
        assert_eq!(apex_vertices.len(), 2, "front and back apex");
        for v in apex_vertices {
            assert_relative_eq!(v.x.abs(), w0, epsilon = 1e-12);
        }
    }

    #[test]
    fn laser_fan_thickness_grows_with_axial_distance() {
        // The Gaussian beam diverges, so a vertex on the front sheet farther
        // from the apex must have a larger `|x|` than a vertex closer in.
        let laser = LaserEntity::new(Isometry3::identity(), 0.25, 1.2, 660.0, 0.25e-3).unwrap();
        let mesh = laser_fan_mesh(&laser);
        // Front-sheet vertex closest to the apex: the smallest non-zero z.
        let mut near = f64::INFINITY;
        let mut near_x = 0.0;
        let mut far = 0.0_f64;
        let mut far_x = 0.0;
        for v in mesh.vertices() {
            if v.x > 0.0 && v.z > 0.0 {
                if v.z < near {
                    near = v.z;
                    near_x = v.x;
                }
                if v.z > far {
                    far = v.z;
                    far_x = v.x;
                }
            }
        }
        assert!(far_x > near_x, "thickness must grow with distance");
    }

    #[test]
    fn laser_fan_far_corners_match_the_half_angle() {
        // The far corners (at the last ring) still subtend exactly the
        // configured half-angle from the local +z axis. Their y/z component
        // is independent of the extruded x thickness.
        let half_angle = 0.3;
        let length = 2.0;
        let laser =
            LaserEntity::new(Isometry3::identity(), half_angle, length, 660.0, 0.25e-3).unwrap();
        let mesh = laser_fan_mesh(&laser);
        // Find the vertex with the largest z (a far corner).
        let far = mesh
            .vertices()
            .iter()
            .max_by(|a, b| a.z.partial_cmp(&b.z).unwrap())
            .unwrap();
        let angle = far.y.abs().atan2(far.z);
        assert_relative_eq!(angle, half_angle, epsilon = 1e-9);
        // And the far corner's (y, z) component is `length` from the apex
        // along its edge ray; only the extruded x is added on top of that.
        let yz_norm = (far.y * far.y + far.z * far.z).sqrt();
        assert_relative_eq!(yz_norm, length, epsilon = 1e-9);
    }

    #[test]
    fn laser_fan_sheets_have_opposite_normals() {
        // The two sheets carry opposite +x / -x normals so the translucent
        // ribbon shades from both sides.
        let laser = LaserEntity::new(Isometry3::identity(), 0.25, 1.2, 660.0, 0.25e-3).unwrap();
        let mesh = laser_fan_mesh(&laser);
        let plus = mesh.normals().iter().find(|n| n.x > 0.5).copied();
        let minus = mesh.normals().iter().find(|n| n.x < -0.5).copied();
        let plus = plus.expect("front sheet must have +x normals");
        let minus = minus.expect("back sheet must have -x normals");
        assert_relative_eq!(plus.dot(&minus), -1.0, epsilon = 1e-12);
    }

    #[test]
    fn target_quad_matches_the_entity_extent() {
        let target = TargetEntity::new(Isometry3::identity(), 0.4, 0.25).unwrap();
        let mesh = target_quad_mesh(&target);
        assert_eq!(mesh.triangle_count(), 2);
        // Quad spans ±width/2 × ±height/2 in the z = 0 plane.
        for v in mesh.vertices() {
            assert_relative_eq!(v.x.abs(), 0.2, epsilon = 1e-12);
            assert_relative_eq!(v.y.abs(), 0.125, epsilon = 1e-12);
            assert_relative_eq!(v.z, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn default_mvp_scene_yields_geometry_for_every_entity() {
        // An integration-style check that the helpers accept the real default
        // scene without panicking and produce non-empty geometry.
        let scene = Scene::default_mvp();
        assert_eq!(camera_frustum_edges(&scene.cameras[0], [1.0; 3]).len(), 12);
        // The M8 ribbon fan has more than the old 2 triangles; just confirm
        // a non-empty, well-formed mesh.
        assert!(laser_fan_mesh(&scene.lasers[0]).triangle_count() > 2);
        assert_eq!(target_quad_mesh(&scene.targets[0]).triangle_count(), 2);
    }

    #[test]
    fn default_mvp_laser_stripe_lands_on_the_target() {
        // The M5 laser stripe: the default scene's laser must paint a stripe
        // on its target — a non-empty run of connected world-space segments.
        let scene = Scene::default_mvp();
        let segs = laser_stripe_segments(&scene, [1.0, 0.0, 0.0]);
        assert!(
            !segs.is_empty(),
            "the default laser fan must strike the default target"
        );
        // Consecutive segments are connected end-to-start (one polyline).
        for w in segs.windows(2) {
            assert_relative_eq!(w[0].1, w[1].0, epsilon = 1e-9);
        }
        // Every stripe vertex sits just off the target plane — the rendered
        // stripe is lifted by STRIPE_SURFACE_LIFT_M along the target normal so
        // it does not z-fight the coplanar quad. The lift sign depends on the
        // laser-side direction; only its magnitude is part of the contract.
        // Each vertex stays well within the target rectangle in x / y.
        let target = &scene.targets[0];
        let target_from_world = target.pose.inverse();
        let hw = 0.5 * target.width;
        let hh = 0.5 * target.height;
        for &(a, _b, _) in &segs {
            let local = target_from_world * a;
            assert_relative_eq!(local.z.abs(), STRIPE_SURFACE_LIFT_M, epsilon = 1e-9);
            assert!(local.x.abs() <= hw + 1e-9 && local.y.abs() <= hh + 1e-9);
        }
    }

    #[test]
    fn laser_stripe_is_empty_when_the_fan_misses() {
        // A laser aimed away from the target produces no stripe — the helper
        // returns an empty Vec rather than panicking.
        let mut scene = Scene::default_mvp();
        // Re-pose the laser to point straight up, away from the upright
        // target: the fan plane no longer crosses the target rectangle.
        let up = nalgebra::UnitQuaternion::rotation_between(&Vector3::z(), &Vector3::z())
            .unwrap_or_else(nalgebra::UnitQuaternion::identity);
        scene.lasers[0].pose =
            Isometry3::from_parts(nalgebra::Translation3::new(0.0, -5.0, 5.0), up);
        let segs = laser_stripe_segments(&scene, [1.0, 0.0, 0.0]);
        assert!(segs.is_empty());
    }

    #[test]
    fn working_volume_mesh_is_built_for_the_default_scene() {
        // The M6 mesh helper: the default scene must place at least one
        // in-volume cell on the fan plane and the helper must produce a
        // non-empty `TriMesh`. Every triangle must lie on (or very close to)
        // the fan plane: the lift is sub-millimetre.
        let scene = Scene::default_mvp();
        let wv = etendue_core::analysis::working_volume(
            &scene.cameras[0],
            &scene.lasers[0],
            64,
            64,
            1.0,
        )
        .expect("working volume for the default scene");
        assert!(wv.in_volume_count() > 0);

        let camera_pos = scene.cameras[0].pose * Point3::origin();
        let mesh = super::working_volume_mesh(&wv, &scene.lasers[0], camera_pos)
            .expect("working-volume mesh should be non-empty for the default scene");
        // Two front + two back triangles per in-volume cell.
        assert!(mesh.triangle_count() > 0);

        // Every vertex sits within `WORKING_VOLUME_LIFT_M` of the fan plane.
        let plane = LaserPlane::from_entity(&scene.lasers[0]);
        for v in mesh.vertices() {
            let d = plane.signed_distance(v).abs();
            assert!(
                d <= WORKING_VOLUME_LIFT_M + 1e-9,
                "vertex must sit within one lift step of the fan plane, got {d}"
            );
        }
    }

    #[test]
    fn working_volume_mesh_is_none_for_an_empty_volume() {
        // If no cell is in the working volume the helper returns `None` so
        // the renderer skips the drawable rather than emitting an empty mesh.
        let scene = Scene::default_mvp();
        // Detune the focus far enough that nothing on the fan is in focus.
        // We construct a working volume with a tight (and ignored) threshold,
        // then sanity-check that an all-out-of-volume case yields `None`.
        // A simpler route: build a WorkingVolume from an "invisible" setup
        // (laser entirely behind the camera) and confirm `None`.
        let mut detuned = scene.clone();
        // Laser pose pointing the fan straight up away from any camera /
        // target — every sample fails the visibility predicate.
        let pose = Isometry3::from_parts(
            nalgebra::Translation3::new(0.0, -5.0, 5.0),
            nalgebra::UnitQuaternion::identity(),
        );
        detuned.lasers[0].pose = pose;
        let wv = etendue_core::analysis::working_volume(
            &detuned.cameras[0],
            &detuned.lasers[0],
            32,
            32,
            1.0,
        )
        .unwrap();
        assert_eq!(wv.in_volume_count(), 0);
        let camera_pos = detuned.cameras[0].pose * Point3::origin();
        let mesh = super::working_volume_mesh(&wv, &detuned.lasers[0], camera_pos);
        assert!(mesh.is_none(), "empty volume must give no mesh");
    }
}
