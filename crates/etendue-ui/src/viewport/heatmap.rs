//! The M4 defocus heatmap: a colormap and the tessellated grid mesh that
//! paints a [`DefocusMap`] onto the target quad.
//!
//! [`crate::analysis::defocus_map`](etendue_core::analysis::defocus_map)
//! produces a `cols × rows` grid of circle-of-confusion values, in pixels,
//! over a planar target. This module turns that grid into renderable
//! geometry:
//!
//! - [`coc_color`] maps one CoC-in-pixels value (or `None`) to an RGB color
//!   through a depth-of-field colormap;
//! - [`heatmap_mesh`] tessellates the target rectangle into a per-vertex-
//!   colored triangle grid matching the map's resolution.
//!
//! # The colormap
//!
//! A focus heatmap wants the *in-focus band* to be unmistakable, so the
//! colormap is the classic depth-of-field ramp rather than a generic
//! perceptual gradient:
//!
//! - **`None`** (the target point cannot be imaged) → a neutral mid-grey,
//!   visually distinct from every in-range color;
//! - **CoC ≤ [`SHARP_THRESHOLD_PX`]** — the *acceptably-sharp band*, the
//!   classic ~1-pixel depth-of-field criterion → a saturated **green**;
//! - **CoC above the threshold** → a ramp from green through yellow to a
//!   saturated **red** as the blur grows toward the map's maximum.
//!
//! The ramp's upper bound is the caller-supplied scale maximum (the heatmap
//! uses [`DefocusMap::max_coc_px`](etendue_core::analysis::DefocusMap::max_coc_px));
//! the same bound drives the UI legend so the colors and the legend agree.
//!
//! The grid is rendered through the unlit per-vertex-color path
//! ([`GpuColoredMesh`](crate::viewport::mesh::GpuColoredMesh)) so the colormap
//! is shown faithfully — no scene lighting darkens the ramp.

use etendue_core::analysis::DefocusMap;
use nalgebra::Point3;

use crate::viewport::mesh::LineVertex;

/// The acceptably-sharp circle-of-confusion threshold, in pixels.
///
/// A target point whose geometric circle of confusion is at or below this
/// blurs by less than about one pixel — the textbook depth-of-field
/// criterion. The heatmap paints every such point the in-focus green; the
/// legend marks this value. A fixed constant is sufficient for the MVP.
pub const SHARP_THRESHOLD_PX: f64 = 1.0;

/// RGB color shown for a target point that cannot be imaged (`None`).
///
/// A neutral mid-grey, deliberately outside the green→yellow→red ramp so an
/// un-imageable region reads as "no data" rather than as a blur value.
pub const NONE_COLOR: [f32; 3] = [0.32, 0.34, 0.38];

/// In-focus color: a saturated green for the acceptably-sharp band.
const SHARP_COLOR: [f32; 3] = [0.16, 0.78, 0.30];
/// Mid-ramp color: yellow, for moderate defocus.
const MID_COLOR: [f32; 3] = [0.96, 0.86, 0.20];
/// Blur-saturation color: a saturated red for the largest defocus.
const BLUR_COLOR: [f32; 3] = [0.90, 0.20, 0.18];

/// Map a circle-of-confusion value to a heatmap RGB color.
///
/// `coc_px` is the CoC at a target sample, in pixels, or `None` if the point
/// could not be imaged. `scale_max_px` is the upper bound of the color ramp,
/// in pixels — normally the map's
/// [`max_coc_px`](etendue_core::analysis::DefocusMap::max_coc_px); the legend
/// shows the same value.
///
/// The mapping:
/// - `None` → [`NONE_COLOR`] (neutral grey);
/// - `Some(c)` with `c <= ` [`SHARP_THRESHOLD_PX`] → the in-focus green;
/// - `Some(c)` above the threshold → a green→yellow→red interpolation, with
///   `c == scale_max_px` reaching saturated red.
///
/// A non-finite or non-positive `scale_max_px` (e.g. an all-sharp map whose
/// maximum is zero) collapses the ramp: every imageable point is then drawn
/// the in-focus green, which is the correct reading — nothing is blurred.
#[must_use]
pub fn coc_color(coc_px: Option<f64>, scale_max_px: f64) -> [f32; 3] {
    let Some(c) = coc_px else {
        return NONE_COLOR;
    };
    if !c.is_finite() {
        return NONE_COLOR;
    }
    // At or below the depth-of-field threshold: the acceptably-sharp band.
    if c <= SHARP_THRESHOLD_PX {
        return SHARP_COLOR;
    }
    // Above the threshold: normalize the remaining range onto [0, 1] and ramp
    // green -> yellow -> red. A degenerate (<= threshold) scale max means
    // there is no blur to show, so everything imageable is the sharp color.
    if !(scale_max_px.is_finite() && scale_max_px > SHARP_THRESHOLD_PX) {
        return SHARP_COLOR;
    }
    let t = ((c - SHARP_THRESHOLD_PX) / (scale_max_px - SHARP_THRESHOLD_PX)).clamp(0.0, 1.0);
    if t <= 0.5 {
        lerp_rgb(SHARP_COLOR, MID_COLOR, (t * 2.0) as f32)
    } else {
        lerp_rgb(MID_COLOR, BLUR_COLOR, ((t - 0.5) * 2.0) as f32)
    }
}

/// Linear interpolation between two RGB colors; `t` is clamped to `[0, 1]`.
fn lerp_rgb(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// A tessellated, per-vertex-colored grid mesh for a [`DefocusMap`].
///
/// The result is in **target-local** coordinates: it lies in the local
/// `z = 0` plane and spans the full target rectangle, exactly like
/// [`target_quad_mesh`](crate::viewport::scene::target_quad_mesh) — so the
/// caller hands the target entity's `pose` to the renderer as the model
/// matrix, unchanged.
///
/// Each grid vertex sits at the map's sample lattice point
/// `(map.local_x(col), map.local_y(row), 0)` and is colored by
/// [`coc_color`] from that sample's CoC value. The grid has
/// `map.cols() × map.rows()` vertices and `(cols-1)·(rows-1)·2` triangles,
/// wound counter-clockwise seen from local `+z` so it faces the same way as
/// the plain target quad.
///
/// `scale_max_px` is the colormap's upper bound (pass
/// [`DefocusMap::max_coc_px`](etendue_core::analysis::DefocusMap::max_coc_px),
/// or any positive value for an all-sharp map). The returned tuple is
/// `(vertices, indices)`, ready for
/// [`GpuColoredMesh::from_colored_vertices`](crate::viewport::mesh::GpuColoredMesh::from_colored_vertices).
#[must_use]
pub fn heatmap_mesh(map: &DefocusMap, scale_max_px: f64) -> (Vec<LineVertex>, Vec<u32>) {
    let cols = map.cols();
    let rows = map.rows();

    // One vertex per sample lattice point, colored by its CoC value.
    let mut vertices = Vec::with_capacity(rows * cols);
    for row in 0..rows {
        let y = map.local_y(row);
        for col in 0..cols {
            let x = map.local_x(col);
            let color = coc_color(map.get(row, col), scale_max_px);
            vertices.push(LineVertex::from_point(Point3::new(x, y, 0.0), color));
        }
    }

    // Two triangles per grid cell. Vertex `(row, col)` is index
    // `row * cols + col`; each cell's quad is wound CCW seen from +z.
    let mut indices = Vec::with_capacity((rows.saturating_sub(1)) * (cols.saturating_sub(1)) * 6);
    for row in 0..rows.saturating_sub(1) {
        for col in 0..cols.saturating_sub(1) {
            let tl = (row * cols + col) as u32;
            let tr = tl + 1;
            let bl = tl + cols as u32;
            let br = bl + 1;
            // CCW from +z: (tl, tr, br) and (tl, br, bl).
            indices.extend_from_slice(&[tl, tr, br, tl, br, bl]);
        }
    }

    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use etendue_core::analysis::defocus_map;
    use etendue_core::scene::Scene;

    #[test]
    fn none_maps_to_the_neutral_grey() {
        assert_eq!(coc_color(None, 20.0), NONE_COLOR);
        // A non-finite CoC is treated like missing data.
        assert_eq!(coc_color(Some(f64::NAN), 20.0), NONE_COLOR);
    }

    #[test]
    fn a_sharp_sample_maps_to_the_in_focus_green() {
        // At or below the depth-of-field threshold: the sharp color.
        assert_eq!(coc_color(Some(0.0), 20.0), SHARP_COLOR);
        assert_eq!(coc_color(Some(SHARP_THRESHOLD_PX), 20.0), SHARP_COLOR);
        assert_eq!(coc_color(Some(SHARP_THRESHOLD_PX * 0.5), 20.0), SHARP_COLOR);
    }

    /// Largest per-channel difference between two RGB colors.
    fn rgb_diff(a: [f32; 3], b: [f32; 3]) -> f32 {
        (0..3).map(|i| (a[i] - b[i]).abs()).fold(0.0_f32, f32::max)
    }

    #[test]
    fn a_blurred_sample_ramps_toward_red() {
        // A sample at the scale maximum reaches the saturated blur red (an
        // f32 interpolation, so compare with a small tolerance).
        assert!(rgb_diff(coc_color(Some(20.0), 20.0), BLUR_COLOR) < 1e-5);
        // A sample well above the scale max is clamped to the same red.
        assert!(rgb_diff(coc_color(Some(200.0), 20.0), BLUR_COLOR) < 1e-5);
        // A mid-range sample is neither the in-focus green nor the blur red.
        let mid = coc_color(Some(10.0), 20.0);
        assert!(rgb_diff(mid, SHARP_COLOR) > 1e-2);
        assert!(rgb_diff(mid, BLUR_COLOR) > 1e-2);
    }

    #[test]
    fn the_ramp_climbs_toward_blue_loss_as_blur_grows() {
        // The depth-of-field ramp runs green -> yellow -> red, so the blue
        // channel — high in neither yellow nor red — falls monotonically once
        // past the sharp band. (Red itself peaks at yellow, so it is the blue
        // channel, not red, that is monotone across the whole ramp.)
        let mut last = f32::INFINITY;
        for i in 0..=20 {
            let c = SHARP_THRESHOLD_PX + (20.0 - SHARP_THRESHOLD_PX) * (f64::from(i) / 20.0);
            let blue = coc_color(Some(c), 20.0)[2];
            assert!(
                blue <= last + 1e-6,
                "blue channel must not increase along the ramp, c = {c}"
            );
            last = blue;
        }
    }

    #[test]
    fn a_degenerate_scale_max_shows_everything_sharp() {
        // An all-sharp map has max_coc_px == 0; the ramp then collapses and
        // every imageable sample reads as the in-focus green.
        assert_eq!(coc_color(Some(0.0), 0.0), SHARP_COLOR);
        assert_eq!(coc_color(Some(5.0), 0.0), SHARP_COLOR);
        // None is still grey regardless of the scale.
        assert_eq!(coc_color(None, 0.0), NONE_COLOR);
    }

    #[test]
    fn heatmap_mesh_has_grid_topology() {
        let scene = Scene::default_mvp();
        let map = defocus_map(&scene.cameras[0], &scene.targets[0], 12, 9).unwrap();
        let (verts, indices) = heatmap_mesh(&map, map.max_coc_px().unwrap_or(1.0));
        // One vertex per sample.
        assert_eq!(verts.len(), 12 * 9);
        // Two triangles per grid cell, three indices each.
        assert_eq!(indices.len(), (12 - 1) * (9 - 1) * 2 * 3);
        // Every index is in range.
        assert!(indices.iter().all(|&i| (i as usize) < verts.len()));
    }

    #[test]
    fn heatmap_mesh_spans_the_target_rectangle() {
        let scene = Scene::default_mvp();
        let target = &scene.targets[0];
        let map = defocus_map(&scene.cameras[0], target, 8, 8).unwrap();
        let (verts, _) = heatmap_mesh(&map, 1.0);
        // Corners of the grid reach ±width/2, ±height/2 in the z = 0 plane.
        let hw = (target.width * 0.5) as f32;
        let hh = (target.height * 0.5) as f32;
        let max_x = verts.iter().map(|v| v.position[0]).fold(f32::MIN, f32::max);
        let max_y = verts.iter().map(|v| v.position[1]).fold(f32::MIN, f32::max);
        assert!((max_x - hw).abs() < 1e-5, "grid must span the target width");
        assert!(
            (max_y - hh).abs() < 1e-5,
            "grid must span the target height"
        );
        // The grid lies in the local z = 0 plane.
        assert!(verts.iter().all(|v| v.position[2].abs() < 1e-6));
    }
}
