//! The M5 "simulated image" panel — the projected laser line in pixel space.
//!
//! Where the 3D viewport shows *where* the laser stripe lands on the target,
//! this panel shows what the **camera sees**: the laser line projected into
//! the camera's pixel coordinate system, drawn with a thickness that encodes
//! the per-point imaged line width.
//!
//! # What is drawn
//!
//! [`simulated_image_panel`] takes a
//! [`ProjectedStripe`](etendue_core::laser::ProjectedStripe) — the projected
//! polyline with a per-vertex `total_px` width — and renders it inside an
//! [`egui_plot`] plot whose axes span the full camera resolution
//! (`0..res_x`, `0..res_y` pixels):
//!
//! - a **filled band** ([`egui_plot::Polygon`]) whose half-width at each
//!   projected vertex is `total_px / 2` — the simulated laser line *with its
//!   physical thickness*, geometric cross-section ⊕ defocus blur made visible;
//! - the **centre polyline** ([`egui_plot::Line`]) over the band, so the line
//!   stays legible where it is thin;
//! - a sensor-frame rectangle so the `0..res` extent is unmistakable.
//!
//! The image-`y`-down convention is honoured: the plot's `y` axis is
//! inverted, so pixel `(0, 0)` sits at the top-left as it does on the sensor.
//!
//! # The width encoding
//!
//! The band is built in pixel space. At each projected vertex the across-line
//! direction is the unit normal of the projected polyline (perpendicular to
//! the segment direction); the band's two rails are the vertex offset by
//! `±total_px/2` along that normal. Tracing one rail forward and the other
//! back yields a closed ribbon whose local width *is* the imaged line width
//! in pixels — so a defocused or obliquely-viewed stretch of the line visibly
//! fattens.

use egui_plot::{Line, Plot, PlotPoints, Points, Polygon};
use etendue_core::laser::ProjectedStripe;
use etendue_core::scene::CameraEntity;
use nalgebra::{Point2, Vector2};

/// Fill color of the simulated laser-line band — a translucent red.
const BAND_FILL: egui::Color32 = egui::Color32::from_rgba_premultiplied(150, 40, 36, 90);
/// Color of the centre polyline drawn over the band — a bright red.
const LINE_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 90, 80);
/// Color of the sensor-frame rectangle — a dim grey.
const FRAME_COLOR: egui::Color32 = egui::Color32::from_rgb(110, 116, 128);

/// A floor on the rendered band half-width, in pixels.
///
/// A sub-pixel imaged line (a sharp, tightly-focused stripe) would draw an
/// invisibly thin band; clamping the half-width up to this keeps the band
/// perceptible while the centre polyline carries the true geometry.
const MIN_HALF_WIDTH_PX: f64 = 0.35;

/// Draw the simulated-image panel: the projected laser line in pixel space.
///
/// `projected` is the laser line projected through the scene camera (from
/// [`project_stripe`](etendue_core::laser::project_stripe)); `camera` supplies
/// the sensor resolution that fixes the plot extent. When `projected` is
/// `None` or has fewer than two visible vertices, the panel shows the empty
/// sensor frame with an explanatory note instead of a line.
///
/// The panel is a bottom dock; it never mutates the scene, so it returns
/// nothing.
pub fn simulated_image_panel(
    ui: &mut egui::Ui,
    projected: Option<&ProjectedStripe>,
    camera: Option<&CameraEntity>,
) {
    egui::Panel::bottom("simulated-image-panel")
        .resizable(true)
        .default_size(220.0)
        .min_size(120.0)
        .show_inside(ui, |ui| {
            ui.heading("Simulated image");

            // Resolution sets the plot extent. Without a camera there is no
            // pixel frame to draw into.
            let Some(camera) = camera else {
                ui.label("No camera in the scene.");
                return;
            };
            let (res_x, res_y) = camera.resolution_f64();

            // A short status line above the plot.
            match projected {
                Some(p) if p.len() >= 2 => {
                    let max_w = p.max_total_px().unwrap_or(0.0);
                    ui.label(format!(
                        "Projected laser line — {} pts, peak width {max_w:.2} px \
                         (geometric ⊕ defocus)",
                        p.len()
                    ));
                }
                _ => {
                    ui.colored_label(
                        egui::Color32::LIGHT_YELLOW,
                        "The laser line is not visible to the camera \
                         (it misses the target or falls outside the frame).",
                    );
                }
            }

            // The plot: pixel axes, y inverted so (0,0) is top-left like the
            // sensor, a fixed 1:1 data aspect so the line is not distorted.
            Plot::new("simulated-image-plot")
                .data_aspect(1.0)
                .invert_y(true)
                .show_axes([true, true])
                .show_grid([true, true])
                .x_axis_label("x (px)")
                .y_axis_label("y (px)")
                // Pin the view to the sensor extent (with a small margin) so
                // the line is always read against the full frame.
                .include_x(-0.05 * res_x)
                .include_x(1.05 * res_x)
                .include_y(-0.05 * res_y)
                .include_y(1.05 * res_y)
                .show(ui, |plot_ui| {
                    // The sensor frame: the 0..res rectangle.
                    plot_ui.polygon(
                        Polygon::new(
                            "sensor",
                            PlotPoints::from(vec![
                                [0.0, 0.0],
                                [res_x, 0.0],
                                [res_x, res_y],
                                [0.0, res_y],
                            ]),
                        )
                        .fill_color(egui::Color32::TRANSPARENT)
                        .stroke(egui::Stroke::new(1.0, FRAME_COLOR)),
                    );

                    // The projected line, when it has a drawable run.
                    if let Some(p) = projected
                        && p.len() >= 2
                    {
                        draw_projected_line(plot_ui, p);
                    }
                });
        });
}

/// Draw the projected laser line: the width-encoding band, then the centre
/// polyline, then the vertex markers.
fn draw_projected_line(plot_ui: &mut egui_plot::PlotUi<'_>, projected: &ProjectedStripe) {
    let points = projected.points();

    // --- The width-encoding band ----------------------------------------
    // Build the two rails by offsetting each vertex ±half-width along the
    // projected polyline's local normal, then close them into one ribbon.
    let band = band_polygon(points);
    if band.len() >= 3 {
        plot_ui.polygon(
            Polygon::new("laser-line-width", PlotPoints::from(band))
                .fill_color(BAND_FILL)
                .stroke(egui::Stroke::NONE),
        );
    }

    // --- The centre polyline --------------------------------------------
    let centre: Vec<[f64; 2]> = points.iter().map(|p| [p.pixel.x, p.pixel.y]).collect();
    plot_ui.line(
        Line::new("laser-line", PlotPoints::from(centre.clone()))
            .color(LINE_COLOR)
            .width(1.5),
    );

    // --- Vertex markers --------------------------------------------------
    // Small dots at each projected sample, so the polyline's sampling is
    // visible and the panel reads as a discrete simulated measurement.
    plot_ui.points(
        Points::new("laser-line-samples", PlotPoints::from(centre))
            .color(LINE_COLOR)
            .radius(1.6),
    );
}

/// Build the closed band polygon (a ribbon) around the projected polyline.
///
/// At each vertex the across-line normal is the perpendicular of the local
/// polyline direction (averaged across the vertex for interior points). The
/// two rails are the vertex `± total_px/2` along that normal; the polygon
/// traces the `+` rail forward then the `-` rail backward.
///
/// Returns the polygon's vertices as `[x, y]` pixel pairs, or an empty `Vec`
/// if the line is too short to form a band.
fn band_polygon(points: &[etendue_core::laser::ProjectedPoint]) -> Vec<[f64; 2]> {
    let n = points.len();
    if n < 2 {
        return Vec::new();
    }

    // Per-vertex unit normal of the projected polyline.
    let normals = vertex_normals(points);

    let mut positive: Vec<[f64; 2]> = Vec::with_capacity(n);
    let mut negative: Vec<[f64; 2]> = Vec::with_capacity(n);
    for (p, nrm) in points.iter().zip(&normals) {
        // Half the imaged line width, with a small visibility floor.
        let half = (0.5 * p.total_px).max(MIN_HALF_WIDTH_PX);
        let off = nrm * half;
        positive.push([p.pixel.x + off.x, p.pixel.y + off.y]);
        negative.push([p.pixel.x - off.x, p.pixel.y - off.y]);
    }

    // Trace the + rail forward, the - rail backward -> a closed ribbon.
    let mut ring = positive;
    ring.extend(negative.into_iter().rev());
    ring
}

/// The per-vertex across-line unit normals of a projected polyline.
///
/// For an interior vertex the tangent is the central difference of the
/// neighbouring vertices; at the ends it is the one adjacent segment. The
/// normal is the tangent rotated 90°. A degenerate (zero-length) tangent
/// falls back to `+x`'s normal so the band stays well-defined.
fn vertex_normals(points: &[etendue_core::laser::ProjectedPoint]) -> Vec<Vector2<f64>> {
    let n = points.len();
    let pos: Vec<Point2<f64>> = points.iter().map(|p| p.pixel).collect();
    let mut normals = Vec::with_capacity(n);
    for i in 0..n {
        let lo = i.saturating_sub(1);
        let hi = (i + 1).min(n - 1);
        let tangent = pos[hi] - pos[lo];
        let len = tangent.norm();
        let unit_tangent = if len > 1e-9 {
            tangent / len
        } else {
            Vector2::new(1.0, 0.0)
        };
        // 90° rotation in the pixel plane: (tx, ty) -> (-ty, tx).
        normals.push(Vector2::new(-unit_tangent.y, unit_tangent.x));
    }
    normals
}

#[cfg(test)]
mod tests {
    use super::*;
    use etendue_core::laser::ProjectedPoint;
    use nalgebra::Point2;

    /// A projected vertex at `(x, y)` with a given total imaged width.
    fn pt(x: f64, y: f64, total: f64) -> ProjectedPoint {
        ProjectedPoint {
            pixel: Point2::new(x, y),
            geom_px: total * 0.5,
            defocus_px: total * 0.5,
            total_px: total,
        }
    }

    #[test]
    fn vertex_normals_are_unit_and_perpendicular_to_the_line() {
        // A straight horizontal line: every normal is the vertical unit
        // vector (perpendicular to the +x tangent).
        let pts = vec![pt(0.0, 5.0, 1.0), pt(10.0, 5.0, 1.0), pt(20.0, 5.0, 1.0)];
        let normals = vertex_normals(&pts);
        assert_eq!(normals.len(), 3);
        for nrm in &normals {
            assert!((nrm.norm() - 1.0).abs() < 1e-12);
            // Perpendicular to the +x line direction.
            assert!(nrm.x.abs() < 1e-12);
            assert!((nrm.y.abs() - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn band_polygon_has_two_rails_and_encodes_width() {
        // A horizontal line of width 4 px: the band's two rails are 4 px
        // apart (±2 px about the centre).
        let pts = vec![pt(0.0, 10.0, 4.0), pt(10.0, 10.0, 4.0), pt(20.0, 10.0, 4.0)];
        let ring = band_polygon(&pts);
        // One vertex per rail per point: 2 * 3 = 6.
        assert_eq!(ring.len(), 6);
        // The + rail (first 3) sits 2 px above the centre y = 10, the - rail
        // (last 3) 2 px below. (y axis is plain here; the plot inverts it.)
        for v in &ring[0..3] {
            assert!((v[1] - 12.0).abs() < 1e-9 || (v[1] - 8.0).abs() < 1e-9);
        }
        // The full band width at the middle vertex is 4 px.
        let top = ring[1][1];
        let bottom = ring[4][1];
        assert!((top - bottom).abs() - 4.0 < 1e-9);
    }

    #[test]
    fn band_half_width_has_a_visibility_floor() {
        // A zero-width (perfectly sharp, sub-pixel) line still yields a
        // visible band — the half-width is floored.
        let pts = vec![pt(0.0, 0.0, 0.0), pt(10.0, 0.0, 0.0)];
        let ring = band_polygon(&pts);
        assert_eq!(ring.len(), 4);
        // The rails are separated by 2 * MIN_HALF_WIDTH_PX, not zero.
        let sep = (ring[0][1] - ring[3][1]).abs();
        assert!((sep - 2.0 * MIN_HALF_WIDTH_PX).abs() < 1e-9);
    }

    #[test]
    fn band_widens_where_the_line_is_wider() {
        // A line that is thin at one end and thick at the other: the band's
        // local width tracks total_px.
        let pts = vec![pt(0.0, 0.0, 2.0), pt(10.0, 0.0, 2.0), pt(20.0, 0.0, 12.0)];
        let ring = band_polygon(&pts);
        assert_eq!(ring.len(), 6);
        // Vertex 0 (thin end): rails ±1 px. Vertex 2 (thick end): rails ±6 px.
        let thin_sep = (ring[0][1] - ring[5][1]).abs();
        let thick_sep = (ring[2][1] - ring[3][1]).abs();
        assert!((thin_sep - 2.0).abs() < 1e-9);
        assert!((thick_sep - 12.0).abs() < 1e-9);
        assert!(thick_sep > thin_sep);
    }

    #[test]
    fn band_polygon_of_a_single_point_is_empty() {
        // A degenerate one-vertex projection cannot form a band.
        let pts = vec![pt(1.0, 2.0, 3.0)];
        assert!(band_polygon(&pts).is_empty());
    }

    #[test]
    fn vertex_normals_handle_a_bent_polyline() {
        // An L-shaped polyline: the corner normal bisects the two segment
        // normals; every normal is still unit length.
        let pts = vec![pt(0.0, 0.0, 1.0), pt(10.0, 0.0, 1.0), pt(10.0, 10.0, 1.0)];
        let normals = vertex_normals(&pts);
        for nrm in &normals {
            assert!((nrm.norm() - 1.0).abs() < 1e-9);
        }
    }
}
