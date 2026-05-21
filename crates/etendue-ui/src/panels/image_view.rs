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
//! [`ProjectedStripe`] — the projected
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
use etendue_core::optics::coc_to_gaussian_sigma;
use etendue_core::scene::CameraEntity;
use nalgebra::{Point2, Vector2};

/// Fill color of the bright **core** of the simulated laser line — a
/// translucent red matching the M5 hard-edge band's apparent intensity. The
/// core extends `± geom_px / 2` from the centerline, representing the
/// laser's *physical* cross-section (the unblurred line).
const BAND_FILL: egui::Color32 = egui::Color32::from_rgba_premultiplied(150, 40, 36, 90);
/// Fill color of the Gaussian-PSF **halo** that surrounds the core. Lower
/// alpha so the wings read as a subtle glow at low defocus and a clear soft
/// halo at high defocus. The halo extends `± (geom_px/2 + 2·sigma)` from
/// the centerline, where `sigma = defocus_px / 2.3548` is the FWHM-matched
/// Gaussian sigma of the M9-follow-up PSF model.
const HALO_FILL: egui::Color32 = egui::Color32::from_rgba_premultiplied(70, 24, 20, 28);
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

            // A two-line status block above the plot: width summary on row 1,
            // mm/px resolution summary on row 2. Reserve worst-case height so
            // the plot doesn't jump as the user moves sliders and the laser
            // visibility transitions between visible (2 rows) and not (1 row).
            let row_h = ui.text_style_height(&egui::TextStyle::Body);
            let status_height = 2.0 * row_h + ui.spacing().item_spacing.y;
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), status_height),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| match projected {
                    Some(p) if p.len() >= 2 => {
                        let max_w = p.max_total_px().unwrap_or(0.0);
                        ui.label(format!(
                            "Projected laser line — {} pts, peak width {max_w:.2} px \
                             (geometric ⊕ defocus)",
                            p.len()
                        ));
                        match p.resolution_summary_mm_per_px() {
                            Some((mn, md, mx)) => ui.label(format!(
                                "Resolution along stripe: min {mn:.3} / median {md:.3} \
                                 / max {mx:.3} mm/px"
                            )),
                            None => ui.label("Resolution: — (need ≥ 2 projected vertices)"),
                        };
                    }
                    _ => {
                        ui.colored_label(
                            egui::Color32::LIGHT_YELLOW,
                            "The laser line is not visible to the camera \
                             (it misses the target or falls outside the frame).",
                        );
                    }
                },
            );

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

/// Draw the projected laser line: the Gaussian-PSF halo, then the bright
/// core band, then the centre polyline, then the vertex markers.
///
/// The hard-edge band of the original M5 panel splits into two nested bands
/// reflecting the M9 follow-up PSF model: an inner *core* of half-width
/// `geom_px / 2` (the physical laser cross-section, essentially unblurred)
/// and an outer *halo* of half-width `geom_px / 2 + 2·sigma`, where
/// `sigma = defocus_px / 2.3548` is the FWHM-matched Gaussian sigma. The
/// halo collapses to the core when the stripe is in perfect focus
/// (`defocus_px ≈ 0`), and widens visibly as the defocus grows — exactly
/// the physical reading of a Gaussian PSF.
fn draw_projected_line(plot_ui: &mut egui_plot::PlotUi<'_>, projected: &ProjectedStripe) {
    let points = projected.points();

    // --- Outer halo: Gaussian wings -------------------------------------
    // Drawn first so the core layer paints on top — alpha-blending then gives
    // an opacity gradient that visually reads as a soft Gaussian falloff.
    let halo = band_polygon_with(points, halo_half_width);
    if halo.len() >= 3 {
        plot_ui.polygon(
            Polygon::new("laser-line-halo", PlotPoints::from(halo))
                .fill_color(HALO_FILL)
                .stroke(egui::Stroke::NONE),
        );
    }

    // --- Inner core: physical line cross-section ------------------------
    let core = band_polygon_with(points, core_half_width);
    if core.len() >= 3 {
        plot_ui.polygon(
            Polygon::new("laser-line-core", PlotPoints::from(core))
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

/// Half-width of the bright **core** at a projected vertex: `geom_px / 2`,
/// floored to [`MIN_HALF_WIDTH_PX`] so a tightly-focused sub-pixel beam still
/// renders a visible line.
fn core_half_width(p: &etendue_core::laser::ProjectedPoint) -> f64 {
    (0.5 * p.geom_px).max(MIN_HALF_WIDTH_PX)
}

/// Half-width of the **Gaussian PSF halo** at a projected vertex:
/// `geom_px/2 + 2·sigma`, where `sigma = coc_to_gaussian_sigma(defocus_px)`
/// is the FWHM-matched Gaussian sigma. Two sigmas of wing cover ~95 % of the
/// Gaussian energy on each side of the core. Floored to
/// [`MIN_HALF_WIDTH_PX`] for visibility.
fn halo_half_width(p: &etendue_core::laser::ProjectedPoint) -> f64 {
    let sigma = coc_to_gaussian_sigma(p.defocus_px);
    (0.5 * p.geom_px + 2.0 * sigma).max(MIN_HALF_WIDTH_PX)
}

/// Build the closed band polygon (a ribbon) around the projected polyline,
/// using a per-vertex `half_width_of` callback to compute each rail's offset.
///
/// At each vertex the across-line normal is the perpendicular of the local
/// polyline direction (averaged across the vertex for interior points). The
/// two rails are the vertex `± half_width_of(vertex)` along that normal; the
/// polygon traces the `+` rail forward then the `-` rail backward into a
/// closed ribbon.
///
/// Returns the polygon's vertices as `[x, y]` pixel pairs, or an empty `Vec`
/// if the line is too short to form a band.
fn band_polygon_with(
    points: &[etendue_core::laser::ProjectedPoint],
    half_width_of: impl Fn(&etendue_core::laser::ProjectedPoint) -> f64,
) -> Vec<[f64; 2]> {
    let n = points.len();
    if n < 2 {
        return Vec::new();
    }

    // Per-vertex unit normal of the projected polyline.
    let normals = vertex_normals(points);

    let mut positive: Vec<[f64; 2]> = Vec::with_capacity(n);
    let mut negative: Vec<[f64; 2]> = Vec::with_capacity(n);
    for (p, nrm) in points.iter().zip(&normals) {
        let half = half_width_of(p);
        let off = nrm * half;
        positive.push([p.pixel.x + off.x, p.pixel.y + off.y]);
        negative.push([p.pixel.x - off.x, p.pixel.y - off.y]);
    }

    // Trace the + rail forward, the - rail backward -> a closed ribbon.
    let mut ring = positive;
    ring.extend(negative.into_iter().rev());
    ring
}

/// Build the band polygon using the legacy `total_px`-based half-width — the
/// M5 hard-edge behaviour. Kept for the unit tests that lock the original
/// band geometry; production rendering uses the M9 follow-up nested-Gaussian
/// layers via [`band_polygon_with`].
#[cfg(test)]
fn band_polygon(points: &[etendue_core::laser::ProjectedPoint]) -> Vec<[f64; 2]> {
    band_polygon_with(points, |p| (0.5 * p.total_px).max(MIN_HALF_WIDTH_PX))
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
    use nalgebra::{Point2, Point3};

    /// A projected vertex at `(x, y)` with a given total imaged width.
    /// `world_m` is set to the origin — these tests exercise the band-drawing
    /// helpers, which only read `pixel` and `total_px`.
    fn pt(x: f64, y: f64, total: f64) -> ProjectedPoint {
        ProjectedPoint {
            pixel: Point2::new(x, y),
            world_m: Point3::origin(),
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

    /// A projected vertex with separately controlled `geom_px` and
    /// `defocus_px` — the M9-follow-up tests need to vary them independently
    /// since the new nested-Gaussian rendering distinguishes the two.
    fn pt_gd(x: f64, y: f64, geom_px: f64, defocus_px: f64) -> ProjectedPoint {
        ProjectedPoint {
            pixel: Point2::new(x, y),
            world_m: Point3::origin(),
            geom_px,
            defocus_px,
            total_px: (geom_px * geom_px + defocus_px * defocus_px).sqrt(),
        }
    }

    #[test]
    fn gaussian_halo_collapses_to_core_for_a_sharp_stripe() {
        // With defocus_px = 0 the halo half-width equals the core half-width:
        // a perfectly focused stripe shows no Gaussian wings.
        let sharp = pt_gd(5.0, 5.0, 2.0, 0.0);
        let core = core_half_width(&sharp);
        let halo = halo_half_width(&sharp);
        assert!((halo - core).abs() < 1e-12);
        assert!(core > 0.0);
    }

    #[test]
    fn gaussian_halo_exceeds_core_when_defocus_is_present() {
        // For a defocused stripe, the halo extends visibly beyond the core
        // — that's the M9-follow-up visualisation contract.
        let blurred = pt_gd(5.0, 5.0, 2.0, 6.0);
        let core = core_half_width(&blurred);
        let halo = halo_half_width(&blurred);
        // Halo half-width = geom_px/2 + 2·sigma = 1 + 2·(6/2.3548) ≈ 6.10 px.
        // Core half-width = geom_px/2 = 1 px.
        assert!(
            halo > core + 3.0,
            "halo {halo} must exceed core {core} by ~5 px"
        );
        let expected_halo = 0.5 * 2.0 + 2.0 * coc_to_gaussian_sigma(6.0);
        assert!((halo - expected_halo).abs() < 1e-9);
    }

    #[test]
    fn halo_grows_linearly_with_defocus() {
        // Doubling defocus_px (at fixed geom_px) increases the halo width
        // by 2·sigma — i.e., the *excess* over the core doubles.
        let a = pt_gd(0.0, 0.0, 1.0, 3.0);
        let b = pt_gd(0.0, 0.0, 1.0, 6.0);
        let excess_a = halo_half_width(&a) - core_half_width(&a);
        let excess_b = halo_half_width(&b) - core_half_width(&b);
        assert!((excess_b - 2.0 * excess_a).abs() < 1e-9);
    }

    #[test]
    fn halo_respects_the_visibility_floor() {
        // A sub-pixel-narrow, perfectly focused stripe still rendered: the
        // floor keeps the halo visible even when geom and defocus are both
        // essentially zero.
        let tiny = pt_gd(0.0, 0.0, 0.0, 0.0);
        assert!((halo_half_width(&tiny) - MIN_HALF_WIDTH_PX).abs() < 1e-12);
        assert!((core_half_width(&tiny) - MIN_HALF_WIDTH_PX).abs() < 1e-12);
    }
}
