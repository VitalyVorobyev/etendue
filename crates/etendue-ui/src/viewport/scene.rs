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
use etendue_core::laser::{GaussianBeamWidth, LaserPlane, stripe_on_target};
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

/// A [`LaserEntity`]'s fan as a thin triangle wedge, in **laser-local**
/// coordinates.
///
/// The fan lies in the laser-local `x = 0` plane and opens symmetrically about
/// the local `+z` axis: the apex is at the local origin, and the two far
/// corners are at `±fan_half_angle` from `+z`, a `fan_length` away along each
/// edge ray. The wedge is built as two triangles with **opposite** winding and
/// normals — one facing local `+x`, one facing local `-x` — so the translucent
/// fan shades consistently whichever side the viewer is on (the renderer
/// disables back-face culling, but a single normal would still leave one face
/// dark).
///
/// # Panics
///
/// Does not panic for a valid [`LaserEntity`]: `LaserEntity::new` guarantees a
/// finite half-angle in `(0, π/2)` and a finite positive length, so the
/// triangle is always non-degenerate and `TriMesh::new`'s invariants hold.
#[must_use]
pub fn laser_fan_mesh(laser: &LaserEntity) -> TriMesh {
    let (s, c) = laser.fan_half_angle.sin_cos();
    let l = laser.fan_length;

    // Apex at the origin; the two far corners straddle +z in the y–z plane.
    let apex = Point3::origin();
    let far_neg = Point3::new(0.0, -l * s, l * c);
    let far_pos = Point3::new(0.0, l * s, l * c);

    // Front face (normal toward local +x): apex → far_neg → far_pos winds CCW
    // seen from +x. Back face reverses the winding and the normal.
    let plus_x = Vector3::new(1.0, 0.0, 0.0);
    let minus_x = Vector3::new(-1.0, 0.0, 0.0);
    let vertices = vec![
        // Front triangle.
        apex, far_neg, far_pos, // Back triangle.
        apex, far_pos, far_neg,
    ];
    let normals = vec![plus_x, plus_x, plus_x, minus_x, minus_x, minus_x];
    let indices = vec![[0, 1, 2], [3, 4, 5]];

    TriMesh::new(vertices, normals, indices)
        .expect("laser fan triangle is non-degenerate for a valid LaserEntity")
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

/// The M5 laser stripe — where the first laser's fan strikes the first
/// target — as colored line segments in **world** coordinates.
///
/// Builds a [`LaserPlane`] for `scene`'s
/// first laser, intersects it with `scene`'s first target via
/// [`stripe_on_target`], and returns
/// the resulting 3D polyline as consecutive `(start, end, color)` segments.
///
/// Returns an empty `Vec` when there is no stripe to draw: the scene has no
/// laser or no target, or the fan simply misses the target (parallel planes,
/// or the intersection line never enters both the fan extent and the target
/// rectangle). The caller then contributes no stripe geometry.
///
/// Unlike the entity-local helpers, the result is already in world
/// coordinates — the stripe is a derived curve spanning the laser *and* the
/// target, so it cannot live in a single entity's local frame. The caller
/// draws it with an identity model matrix (as it does the ground grid).
#[must_use]
pub fn laser_stripe_segments(
    scene: &Scene,
    color: [f32; 3],
) -> Vec<(Point3<f64>, Point3<f64>, [f32; 3])> {
    let (Some(laser), Some(target)) = (scene.lasers.first(), scene.targets.first()) else {
        return Vec::new();
    };
    let plane = LaserPlane::from_entity(laser);
    let Ok(width_model) = GaussianBeamWidth::from_laser(laser) else {
        return Vec::new();
    };
    let Some(stripe) = stripe_on_target(&plane, target, &width_model, STRIPE_SAMPLES) else {
        return Vec::new();
    };

    // Lift the stripe a hair off the target surface so it does not z-fight
    // the coplanar target quad / heatmap grid. The lift is along the target's
    // outward normal toward the laser side (the laser illuminates the front
    // face, so the dot-product sign picks the correct direction).
    let mut surface_normal = target.pose * Vector3::z();
    let to_laser = (plane.origin() - (target.pose * Point3::origin())).normalize();
    if surface_normal.dot(&to_laser) < 0.0 {
        surface_normal = -surface_normal;
    }
    let lift = surface_normal * STRIPE_SURFACE_LIFT_M;

    // Consecutive samples -> line-list segments, each vertex lifted off the
    // surface.
    let samples = stripe.samples();
    samples
        .windows(2)
        .map(|w| (w[0].point + lift, w[1].point + lift, color))
        .collect()
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
    fn laser_fan_mesh_has_two_triangles() {
        let laser = LaserEntity::new(Isometry3::identity(), 0.25, 1.2, 660.0, 0.25e-3).unwrap();
        let mesh = laser_fan_mesh(&laser);
        assert_eq!(mesh.triangle_count(), 2);
        assert_eq!(mesh.vertices().len(), 6);
    }

    #[test]
    fn laser_fan_mesh_lies_in_the_local_x_plane() {
        // The fan is a flat sheet in the laser-local x = 0 plane.
        let laser = LaserEntity::new(Isometry3::identity(), 0.25, 1.2, 660.0, 0.25e-3).unwrap();
        let mesh = laser_fan_mesh(&laser);
        for v in mesh.vertices() {
            assert_relative_eq!(v.x, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn laser_fan_half_angle_matches_the_far_corners() {
        // The far corners must subtend exactly the configured half-angle from
        // the local +z axis.
        let half_angle = 0.3;
        let length = 2.0;
        let laser =
            LaserEntity::new(Isometry3::identity(), half_angle, length, 660.0, 0.25e-3).unwrap();
        let mesh = laser_fan_mesh(&laser);
        // Vertex 2 is a far corner; angle from +z is atan2(|y|, z).
        let corner = mesh.vertices()[2];
        let angle = corner.y.abs().atan2(corner.z);
        assert_relative_eq!(angle, half_angle, epsilon = 1e-9);
        // And the corner is `length` away from the apex along its edge ray.
        assert_relative_eq!(corner.coords.norm(), length, epsilon = 1e-9);
    }

    #[test]
    fn laser_fan_faces_point_opposite_ways() {
        // The two triangles carry opposite normals so the translucent fan
        // shades from both sides.
        let laser = LaserEntity::new(Isometry3::identity(), 0.25, 1.2, 660.0, 0.25e-3).unwrap();
        let mesh = laser_fan_mesh(&laser);
        let n_front = mesh.normals()[0];
        let n_back = mesh.normals()[3];
        assert_relative_eq!(n_front.dot(&n_back), -1.0, epsilon = 1e-12);
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
        assert_eq!(laser_fan_mesh(&scene.lasers[0]).triangle_count(), 2);
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
