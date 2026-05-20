//! The M3 parameter panel — a `SidePanel` with collapsible sections per entity.
//!
//! # Layout
//!
//! ```text
//! ┌─────────────────────────────┐
//! │ [Save Scene] [Load Scene]   │
//! │ ─────────────────────────── │
//! │ ☑ Defocus heatmap           │
//! │   sharp ▮▮▮▮▮▮▮▮ blurry      │
//! │   0 px ─ 1 px ───── max px   │
//! │ ─────────────────────────── │
//! │ ▾ Camera 0                  │
//! │   Effective focal length mm │
//! │   f-number                  │
//! │   Focus distance m          │
//! │   Principal gap H–H′ mm     │
//! │   cx / cy (optional)        │
//! │   τx  τy (Scheimpflug)      │
//! │   Pose: tx ty tz            │
//! │         rx ry rz (Euler°)   │
//! │   Frustum near / far        │
//! │ ─────────────────────────── │
//! │ ▾ Laser 0                   │
//! │   Fan half-angle °          │
//! │   Fan length                │
//! │   Wavelength nm             │
//! │   Beam waist w₀ µm          │
//! │   Pose: tx ty tz  rx ry rz  │
//! │ ─────────────────────────── │
//! │ ▾ Target 0                  │
//! │   Width / Height            │
//! │   Pose: tx ty tz  rx ry rz  │
//! └─────────────────────────────┘
//! ```
//!
//! # Reactivity
//!
//! Every slider/drag-value returns an egui `Response`. The function
//! [`scene_panel`] accumulates these with bitwise-OR (`|=`) into a single
//! `changed` bool that it returns. The caller ([`crate::app`]) checks that
//! flag and calls `Renderer::rebuild_scene` when it is set — and, since M4,
//! recomputes the defocus map for the same rebuild. Toggling the heatmap
//! checkbox also sets `changed`: it swaps the target's geometry (plain quad ↔
//! heatmap grid), so the viewport must rebuild.
//!
//! # M4 — defocus heatmap controls and optics sliders
//!
//! The panel's top section is the M4 heatmap control: a show/hide checkbox
//! ([`PanelState::show_heatmap`]) and a color-scale legend
//! ([`heatmap_legend`]). `camera_section` carries the **physical optics
//! sliders** — effective focal length, f-number, focus distance, and the
//! principal-plane gap — at the former `// M4:` marker. Editing any of them
//! calls [`CameraEntity::sync_intrinsics_from_physical`] so the derived
//! pixel-unit intrinsics stay consistent; M3's direct pixels-based focal
//! slider is gone (`fx`/`fy` are now derived, never edited).
//!
//! # M5 — laser line
//!
//! `laser_section` carries the M5 **beam-waist** slider (`w₀`, edited in
//! micrometres) alongside the existing fan controls. The waist drives the
//! Gaussian-beam line-width model; editing it sets `changed`, so the caller
//! recomputes the laser stripe and the simulated-image panel on the same
//! rebuild path the other sliders use.
//!
//! # Save / Load
//!
//! "Save Scene" serialises `&Scene` to pretty JSON via `serde_json` and
//! opens a native save dialog via `rfd`. "Load Scene" opens an open dialog,
//! reads the chosen file, and deserializes a new `Scene`. Errors are shown in
//! the panel (never panic). The caller owns the `Scene` mutation.

use etendue_core::analysis::{DefocusMap, WorkingVolume};
use etendue_core::scene::{CameraEntity, LaserEntity, Scene, TargetEntity};
use nalgebra::{Isometry3, Rotation3, Translation3, UnitQuaternion};

use etendue_core::calibration::{IntrinsicsParams, ScheimpflugParams, SensorParams};

use crate::viewport::heatmap::{NONE_COLOR, SHARP_THRESHOLD_PX, coc_color};

/// Persistent UI state for the parameter panel.
///
/// Lives in `Graphics` (in `crate::app`) alongside the `Scene` and is reset to
/// `default` on scene load.
#[derive(Debug)]
pub struct PanelState {
    /// Collapsible-section open/closed state for each camera entity (by index).
    pub camera_open: Vec<bool>,
    /// Collapsible-section open/closed state for each laser entity (by index).
    pub laser_open: Vec<bool>,
    /// Collapsible-section open/closed state for each target entity (by index).
    pub target_open: Vec<bool>,
    /// Last load/save error message (shown inline in the panel).
    pub io_error: Option<String>,
    /// Whether the M4 defocus heatmap is shown on the target (vs. the plain
    /// shaded quad). Defaults to `true` so the application opens on the M4
    /// visual checkpoint.
    pub show_heatmap: bool,
    /// Whether the M6 working-volume patch is shown on the laser fan plane.
    /// Defaults to `true` so the application opens on the M6 MVP-capstone
    /// visual (heatmap on target + working-volume patch on the fan + the
    /// 2D simulated-image panel below).
    pub show_working_volume: bool,
}

impl Default for PanelState {
    fn default() -> Self {
        Self {
            camera_open: Vec::new(),
            laser_open: Vec::new(),
            target_open: Vec::new(),
            io_error: None,
            // Open showing the heatmap — it is the M4 deliverable's headline.
            show_heatmap: true,
            // Open showing the working-volume patch — it is the M6 MVP
            // capstone's headline.
            show_working_volume: true,
        }
    }
}

impl PanelState {
    /// Ensure the open-state vecs are long enough for the current scene.
    pub fn sync_to_scene(&mut self, scene: &Scene) {
        sync_open_vec(&mut self.camera_open, scene.cameras.len());
        sync_open_vec(&mut self.laser_open, scene.lasers.len());
        sync_open_vec(&mut self.target_open, scene.targets.len());
    }
}

fn sync_open_vec(v: &mut Vec<bool>, len: usize) {
    if v.len() < len {
        v.resize(len, true); // new sections default to open
    }
}

/// Draw the full parameter side-panel.
///
/// `heatmap` is the current defocus map for the first target (or `None` when
/// the heatmap is off or no map could be computed); it backs the color-scale
/// legend so the legend's `max` matches the rendered colors. `working_volume`
/// is the M6 working volume for the first camera + first laser (or `None`);
/// it backs the area / depth-range status readout next to the working-volume
/// toggle.
///
/// Returns a pair `(changed, new_scene)` where:
/// - `changed` is `true` if any slider or drag-value was edited this frame —
///   *or* the heatmap / working-volume toggle flipped — signalling the
///   caller to rebuild the viewport drawables and recompute the maps.
/// - `new_scene` is `Some(Scene)` if the user loaded a scene from disk this
///   frame; the caller replaces the live scene with it and rebuilds the
///   viewport.
pub fn scene_panel(
    ui: &mut egui::Ui,
    scene: &mut Scene,
    state: &mut PanelState,
    heatmap: Option<&DefocusMap>,
    working_volume: Option<&WorkingVolume>,
) -> (bool, Option<Scene>) {
    state.sync_to_scene(scene);
    let mut changed = false;
    let mut loaded_scene: Option<Scene> = None;

    egui::Panel::left("params-panel")
        .resizable(true)
        .default_size(260.0)
        .min_size(200.0)
        .show_inside(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                // --- Save / Load buttons -----------------------------------
                ui.horizontal(|ui| {
                    if ui.button("Save Scene").clicked() {
                        save_scene_dialog(scene, state);
                    }
                    if ui.button("Load Scene").clicked()
                        && let Some(s) = load_scene_dialog(state)
                    {
                        loaded_scene = Some(s);
                    }
                });
                if let Some(err) = &state.io_error {
                    ui.colored_label(egui::Color32::LIGHT_RED, err);
                }
                ui.separator();

                // --- Defocus heatmap toggle + legend (M4) ------------------
                let toggle = ui.checkbox(&mut state.show_heatmap, "Defocus heatmap");
                // A flipped toggle swaps the target geometry, so it must
                // trigger the same rebuild path a slider edit does.
                changed |= toggle.changed();
                if state.show_heatmap {
                    heatmap_legend(ui, heatmap);
                }
                ui.separator();

                // --- Working volume toggle + readout (M6) -------------------
                let wv_toggle = ui.checkbox(&mut state.show_working_volume, "Working volume");
                // A flipped toggle adds / removes the fan-plane patch, so
                // trigger the same rebuild path a slider edit does.
                changed |= wv_toggle.changed();
                working_volume_readout(ui, working_volume, state.show_working_volume);
                ui.separator();

                // --- Camera sections ---------------------------------------
                for (i, camera) in scene.cameras.iter_mut().enumerate() {
                    let open = &mut state.camera_open[i];
                    let label = format!("Camera {i}");
                    let ch = camera_section(ui, camera, &label, open);
                    changed |= ch;
                    ui.separator();
                }

                // --- Laser sections ----------------------------------------
                for (i, laser) in scene.lasers.iter_mut().enumerate() {
                    let open = &mut state.laser_open[i];
                    let label = format!("Laser {i}");
                    let ch = laser_section(ui, laser, &label, open);
                    changed |= ch;
                    ui.separator();
                }

                // --- Target sections ---------------------------------------
                for (i, target) in scene.targets.iter_mut().enumerate() {
                    let open = &mut state.target_open[i];
                    let label = format!("Target {i}");
                    let ch = target_section(ui, target, &label, open);
                    changed |= ch;
                    ui.separator();
                }
            });
        });

    (changed, loaded_scene)
}

// ---------------------------------------------------------------------------
// Defocus-heatmap legend (M4)
// ---------------------------------------------------------------------------

/// Draw the defocus-heatmap color-scale legend.
///
/// Renders a horizontal strip sampled through the heatmap colormap
/// ([`coc_color`]) from sharp (left) to blurry (right) — so the strip itself
/// shows where the green in-focus band ends and the yellow→red blur ramp
/// begins — under which two labels report the circle-of-confusion-in-pixels
/// range it spans:
///
/// - the strip's left end is `0 px`, its right end the map's
///   [`DefocusMap::max_coc_px`];
/// - a caption names the acceptably-sharp threshold ([`SHARP_THRESHOLD_PX`]),
///   the green band's upper CoC bound.
///
/// `heatmap` is the current map: its maximum sets the scale's upper bound. An
/// absent map, or a map whose every sample failed to image, shows an explicit
/// "not imageable" note instead of a scale; an all-sharp map (maximum `0`)
/// shows a flat in-focus strip labelled accordingly.
fn heatmap_legend(ui: &mut egui::Ui, heatmap: Option<&DefocusMap>) {
    ui.label("Circle of confusion (px):");

    // Resolve the scale's upper bound from the map.
    let max_coc = heatmap.and_then(DefocusMap::max_coc_px);

    // Draw the color strip: a row of thin rectangles sampled through the
    // colormap. `scale_max` of 0 (or absent) collapses the ramp — coc_color
    // then returns the in-focus color everywhere, the correct "all sharp"
    // reading.
    let scale_max = max_coc.unwrap_or(0.0);
    let strip_height = 14.0_f32;
    let (rect, _response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().min(220.0), strip_height),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    let steps = 64_usize;
    for i in 0..steps {
        let t0 = i as f32 / steps as f32;
        let t1 = (i + 1) as f32 / steps as f32;
        // Sample the colormap at the centre of this slice. The strip spans
        // CoC 0 .. max(scale_max, threshold) so the green band is visible
        // even when the whole target is sharp.
        let span = scale_max.max(SHARP_THRESHOLD_PX * 2.0);
        let coc = f64::from((t0 + t1) * 0.5) * span;
        let [r, g, b] = coc_color(Some(coc), scale_max);
        let color =
            egui::Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8);
        let x0 = rect.left() + t0 * rect.width();
        let x1 = rect.left() + t1 * rect.width();
        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(x0, rect.top()), egui::pos2(x1, rect.bottom())),
            0.0,
            color,
        );
    }

    // Tick labels under the strip.
    match max_coc {
        None => {
            ui.colored_label(
                egui::Color32::from_rgb(
                    (NONE_COLOR[0] * 255.0) as u8,
                    (NONE_COLOR[1] * 255.0) as u8,
                    (NONE_COLOR[2] * 255.0) as u8,
                ),
                "target not imageable",
            );
        }
        Some(m) if m <= SHARP_THRESHOLD_PX => {
            ui.label(format!(
                "all sharp — peak {m:.2} px (≤ {SHARP_THRESHOLD_PX:.0} px in focus)"
            ));
        }
        Some(m) => {
            ui.horizontal(|ui| {
                ui.label("0");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("max {m:.1} px"));
                });
            });
            ui.label(format!(
                "in-focus band: CoC ≤ {SHARP_THRESHOLD_PX:.0} px (green)"
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// Working-volume readout (M6)
// ---------------------------------------------------------------------------

/// Draw the M6 working-volume status readout under the toggle.
///
/// Shows the two numbers the triangulation-sensor designer cares about most:
///
/// - **Area** — the working-volume area on the laser fan plane, in **mm²**
///   (a sensible unit for a few-square-cm measurement patch). This is the
///   covered measurement area: the patch of the fan plane the sensor can
///   illuminate, see, and focus on simultaneously.
/// - **Depth range** — the camera-frame `z` range across the working volume,
///   in millimetres. This is the sensor's achieved depth coverage: the slice
///   along the optical axis that the working volume actually spans.
///
/// When the toggle is **off** the labels are dim and prefixed accordingly so
/// the readout still gives the values (they are computed regardless) without
/// implying the rendered patch is showing. When the cache is `None` (no
/// camera/laser yet, or the analysis failed), a one-line "no working volume"
/// note is shown instead.
fn working_volume_readout(
    ui: &mut egui::Ui,
    working_volume: Option<&WorkingVolume>,
    show_patch: bool,
) {
    let dim_color = ui.visuals().weak_text_color();

    let Some(wv) = working_volume else {
        ui.colored_label(dim_color, "no working volume — check scene");
        return;
    };

    let area_m2 = wv.area_m2();
    let area_mm2 = area_m2 * 1.0e6;

    // Area line: report mm² for a sensible 1–10000 reading; switch to cm²
    // beyond that so the label stays compact for a wide-angle sensor.
    let area_text = if area_mm2 >= 1.0e4 {
        format!("Area: {:.2} cm²", area_mm2 * 1.0e-2)
    } else {
        format!("Area: {area_mm2:.1} mm²")
    };

    let depth_text = match wv.depth_range_m() {
        Some((z_min, z_max)) => {
            let span_mm = (z_max - z_min) * 1.0e3;
            format!(
                "Depth z: {:.1}..{:.1} mm  (Δ {:.1} mm)",
                z_min * 1.0e3,
                z_max * 1.0e3,
                span_mm,
            )
        }
        None => "Depth z: — (empty volume)".to_string(),
    };

    if show_patch {
        ui.label(area_text);
        ui.label(depth_text);
    } else {
        // Show the numbers in a dimmer colour so the off-state is obvious;
        // the values themselves are still useful for tuning sliders without
        // the on-fan overlay.
        ui.colored_label(dim_color, area_text);
        ui.colored_label(dim_color, depth_text);
    }
}

// ---------------------------------------------------------------------------
// Camera section
// ---------------------------------------------------------------------------

/// Draw the collapsible camera section; return true if any widget changed.
fn camera_section(
    ui: &mut egui::Ui,
    camera: &mut CameraEntity,
    label: &str,
    open: &mut bool,
) -> bool {
    let mut changed = false;
    egui::CollapsingHeader::new(label)
        .open(Some(*open))
        .show(ui, |ui| {
            *open = true; // if user re-opens it, remember that

            // --- Physical optics sliders (M4) ------------------------------
            // These four physical fields are the lens's source of truth; the
            // pixel-unit fx/fy are *derived* from the effective focal length
            // and the pixel pitch, so M3's direct fx slider is gone. After
            // any edit `sync_intrinsics_from_physical` rewrites fx/fy.
            changed |= optics_sliders(ui, camera);

            // --- Principal point (optional, for decentred lenses) ----------
            // cx / cy are genuine free intrinsics (not derived), so they stay
            // directly editable.
            {
                let IntrinsicsParams::FxFyCxCySkew { params } = &mut camera.params.intrinsics;
                ui.label("Principal point (px):");
                ui.horizontal(|ui| {
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut params.cx)
                                .prefix("cx ")
                                .speed(0.5),
                        )
                        .changed();
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut params.cy)
                                .prefix("cy ")
                                .speed(0.5),
                        )
                        .changed();
                });
            }

            // --- Scheimpflug tilt angles (only when sensor is Scheimpflug) -
            if let SensorParams::Scheimpflug { params: sp } = &mut camera.params.sensor {
                ui.label("Scheimpflug tilt (°):");
                changed |= scheimpflug_sliders(ui, sp);
            }

            // --- Frustum near / far ----------------------------------------
            ui.label("Frustum (m):");
            ui.horizontal(|ui| {
                let near_r = ui.add(
                    egui::DragValue::new(&mut camera.frustum_near)
                        .prefix("near ")
                        .speed(0.001)
                        .range(0.001..=camera.frustum_far - 0.001),
                );
                let far_r = ui.add(
                    egui::DragValue::new(&mut camera.frustum_far)
                        .prefix("far ")
                        .speed(0.005)
                        .range(camera.frustum_near + 0.001..=100.0),
                );
                if near_r.changed() || far_r.changed() {
                    changed = true;
                }
            });

            // --- Pose (translation + Euler rotation) -----------------------
            changed |= pose_editor(ui, &mut camera.pose, "Camera pose");
        });
    changed
}

/// The M4 physical-optics sliders for a camera.
///
/// Draws four sliders for the lens parameters that drive the defocus model:
///
/// - **effective focal length**, edited in millimetres, stored as
///   `effective_focal_length_m`;
/// - **f-number** `N`, dimensionless;
/// - **focus distance**, in metres, the object-side conjugate the lens is
///   focused on;
/// - **principal-plane gap** `H − H′`, edited in millimetres, stored as
///   `principal_gap_m`.
///
/// The pixel-unit `fx`/`fy` intrinsics are *derived* from the effective focal
/// length and the pixel pitch — never edited directly — so after **any**
/// change here [`CameraEntity::sync_intrinsics_from_physical`] is called to
/// rewrite them. Returns `true` if any slider changed.
///
/// The slider ranges keep the lens physically valid: the focus distance is
/// held strictly above the focal length (an object inside the front focal
/// point forms no real image, which [`CameraEntity`]'s invariants reject),
/// and the focal length, f-number, and gap stay in sane machine-vision
/// ranges. The on-screen units (mm) are converted to the stored SI metres.
fn optics_sliders(ui: &mut egui::Ui, camera: &mut CameraEntity) -> bool {
    let mut changed = false;

    // Effective focal length, edited in mm.
    let mut focal_mm = camera.effective_focal_length_m * 1000.0;
    let r = ui.add(
        egui::Slider::new(&mut focal_mm, 2.0..=100.0)
            .text("Focal length (mm)")
            .step_by(0.1),
    );
    if r.changed() {
        camera.effective_focal_length_m = focal_mm / 1000.0;
        changed = true;
    }

    // f-number.
    let r = ui.add(
        egui::Slider::new(&mut camera.f_number, 1.0..=22.0)
            .text("f-number")
            .step_by(0.1),
    );
    changed |= r.changed();

    // Focus distance, in metres. Held strictly above the focal length so the
    // lens always forms a real image (CameraEntity's invariant).
    let focus_lower = camera.effective_focal_length_m * 1.001;
    let mut focus_m = camera.focus_distance_m.max(focus_lower);
    let r = ui.add(
        egui::Slider::new(&mut focus_m, focus_lower..=5.0)
            .text("Focus distance (m)")
            .step_by(0.001),
    );
    if r.changed() {
        camera.focus_distance_m = focus_m;
        changed = true;
    }

    // Inter-principal gap H − H′, edited in mm (>= 0).
    let mut gap_mm = camera.principal_gap_m * 1000.0;
    let r = ui.add(
        egui::Slider::new(&mut gap_mm, 0.0..=50.0)
            .text("Principal gap H−H′ (mm)")
            .step_by(0.1),
    );
    if r.changed() {
        camera.principal_gap_m = (gap_mm / 1000.0).max(0.0);
        changed = true;
    }

    // Any physical-optics edit invalidates the derived pixel intrinsics —
    // re-derive fx/fy from the (possibly new) focal length and pixel pitch.
    let mut focus_was_clamped = false;
    if changed {
        // Keep the focus distance strictly above the focal length: dragging
        // the focal length up can raise the `focal * 1.001` floor past the
        // stored focus distance, and an object at/inside the front focal
        // point forms no real image (a `CameraEntity` invariant). Clamping to
        // the same floor the focus slider uses also stops the displayed value
        // jumping on the next frame.
        let focus_floor = camera.effective_focal_length_m * 1.001;
        if camera.focus_distance_m < focus_floor {
            camera.focus_distance_m = focus_floor;
            focus_was_clamped = true;
        }
        camera.sync_intrinsics_from_physical();
    }
    if focus_was_clamped {
        ui.colored_label(egui::Color32::from_gray(140), "Focus pinned to >f");
    }

    changed
}

/// Scheimpflug tilt sliders, converting between degrees (UI) and radians.
fn scheimpflug_sliders(ui: &mut egui::Ui, sp: &mut ScheimpflugParams) -> bool {
    let mut changed = false;
    let mut tilt_x_deg = sp.tilt_x.to_degrees();
    let mut tilt_y_deg = sp.tilt_y.to_degrees();
    ui.horizontal(|ui| {
        let rx = ui.add(
            egui::Slider::new(&mut tilt_x_deg, -30.0..=30.0)
                .text("τx°")
                .step_by(0.1),
        );
        if rx.changed() {
            sp.tilt_x = tilt_x_deg.to_radians();
            changed = true;
        }
        let ry = ui.add(
            egui::Slider::new(&mut tilt_y_deg, -30.0..=30.0)
                .text("τy°")
                .step_by(0.1),
        );
        if ry.changed() {
            sp.tilt_y = tilt_y_deg.to_radians();
            changed = true;
        }
    });
    changed
}

// ---------------------------------------------------------------------------
// Laser section
// ---------------------------------------------------------------------------

/// Draw the collapsible laser section; return true if any widget changed.
fn laser_section(ui: &mut egui::Ui, laser: &mut LaserEntity, label: &str, open: &mut bool) -> bool {
    let mut changed = false;
    egui::CollapsingHeader::new(label)
        .open(Some(*open))
        .show(ui, |ui| {
            *open = true;

            // Fan half-angle: UI in degrees, stored in radians.
            let mut half_deg = laser.fan_half_angle.to_degrees();
            let r = ui.add(
                egui::Slider::new(&mut half_deg, 1.0..=89.0)
                    .text("Fan half-angle (°)")
                    .step_by(0.1),
            );
            if r.changed() {
                // Clamp to entity's valid range: (0, π/2) exclusive.
                let clamped = half_deg.clamp(0.1, 89.9);
                laser.fan_half_angle = clamped.to_radians();
                changed = true;
            }

            let r = ui.add(
                egui::Slider::new(&mut laser.fan_length, 0.01..=5.0)
                    .text("Fan length (m)")
                    .step_by(0.01),
            );
            if r.changed() {
                changed = true;
            }

            let r = ui.add(
                egui::Slider::new(&mut laser.wavelength_nm, 380.0..=1100.0)
                    .text("Wavelength (nm)")
                    .step_by(1.0),
            );
            if r.changed() {
                changed = true;
            }

            // --- Beam waist (M5) -------------------------------------------
            // The Gaussian-beam waist radius w0 drives the M5 line-width
            // model: w(d) = w0·sqrt(1 + (d/zR)²), zR = π·w0²/λ. Edited in
            // micrometres (a machine-vision line laser focuses to a waist of
            // tens to a few hundred µm), stored as metres.
            let mut waist_um = laser.beam_waist_m * 1.0e6;
            let r = ui.add(
                egui::Slider::new(&mut waist_um, 10.0..=1000.0)
                    .text("Beam waist w₀ (µm)")
                    .step_by(1.0),
            );
            if r.changed() {
                // Keep w0 strictly positive (the entity's invariant and the
                // GaussianBeamWidth model both require it).
                laser.beam_waist_m = (waist_um * 1.0e-6).max(1.0e-7);
                changed = true;
            }

            changed |= pose_editor(ui, &mut laser.pose, "Laser pose");
        });
    changed
}

// ---------------------------------------------------------------------------
// Target section
// ---------------------------------------------------------------------------

/// Draw the collapsible target section; return true if any widget changed.
fn target_section(
    ui: &mut egui::Ui,
    target: &mut TargetEntity,
    label: &str,
    open: &mut bool,
) -> bool {
    let mut changed = false;
    egui::CollapsingHeader::new(label)
        .open(Some(*open))
        .show(ui, |ui| {
            *open = true;

            let rw = ui.add(
                egui::Slider::new(&mut target.width, 0.01..=2.0)
                    .text("Width (m)")
                    .step_by(0.005),
            );
            let rh = ui.add(
                egui::Slider::new(&mut target.height, 0.01..=2.0)
                    .text("Height (m)")
                    .step_by(0.005),
            );
            if rw.changed() || rh.changed() {
                changed = true;
            }

            changed |= pose_editor(ui, &mut target.pose, "Target pose");
        });
    changed
}

// ---------------------------------------------------------------------------
// Shared pose editor
// ---------------------------------------------------------------------------

/// Draw translation + Euler-angle drag-values for an `Isometry3<f64>`.
///
/// Translation is in metres; rotation is displayed in degrees. Euler
/// convention is `(roll, pitch, yaw)` via `nalgebra::Rotation3::euler_angles`.
/// Gimbal-lock is acceptable for this MVP tool.
///
/// Returns true if any of the six drag-values was changed.
fn pose_editor(ui: &mut egui::Ui, pose: &mut Isometry3<f64>, header: &str) -> bool {
    let mut changed = false;

    ui.collapsing(header, |ui| {
        // --- Translation ---------------------------------------------------
        let mut t = pose.translation.vector;
        ui.label("Translation (m):");
        ui.horizontal(|ui| {
            changed |= ui
                .add(egui::DragValue::new(&mut t.x).prefix("x ").speed(0.001))
                .changed();
            changed |= ui
                .add(egui::DragValue::new(&mut t.y).prefix("y ").speed(0.001))
                .changed();
            changed |= ui
                .add(egui::DragValue::new(&mut t.z).prefix("z ").speed(0.001))
                .changed();
        });

        // --- Rotation (Euler angles) ---------------------------------------
        let rot = pose.rotation.to_rotation_matrix();
        let (roll, pitch, yaw) = rot.euler_angles();
        let mut roll_deg = roll.to_degrees();
        let mut pitch_deg = pitch.to_degrees();
        let mut yaw_deg = yaw.to_degrees();
        ui.label("Rotation (°):");
        ui.horizontal(|ui| {
            changed |= ui
                .add(egui::DragValue::new(&mut roll_deg).prefix("r ").speed(0.1))
                .changed();
            changed |= ui
                .add(egui::DragValue::new(&mut pitch_deg).prefix("p ").speed(0.1))
                .changed();
            changed |= ui
                .add(egui::DragValue::new(&mut yaw_deg).prefix("y ").speed(0.1))
                .changed();
        });

        // Write back
        let new_rot = Rotation3::from_euler_angles(
            roll_deg.to_radians(),
            pitch_deg.to_radians(),
            yaw_deg.to_radians(),
        );
        *pose = Isometry3::from_parts(
            Translation3::from(t),
            UnitQuaternion::from_rotation_matrix(&new_rot),
        );
    });

    changed
}

// ---------------------------------------------------------------------------
// Save / Load helpers
// ---------------------------------------------------------------------------

/// Open a native save dialog and write the scene as pretty JSON.
///
/// On error, records the message in `state.io_error` (shown next frame).
fn save_scene_dialog(scene: &Scene, state: &mut PanelState) {
    state.io_error = None;

    let Some(path) = rfd::FileDialog::new()
        .set_title("Save Scene")
        .add_filter("JSON", &["json"])
        .set_file_name("scene.json")
        .save_file()
    else {
        return; // user cancelled
    };

    match serde_json::to_string_pretty(scene) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, &json) {
                state.io_error = Some(format!("Save failed: {e}"));
            }
        }
        Err(e) => {
            state.io_error = Some(format!("Serialize failed: {e}"));
        }
    }
}

/// Open a native open dialog and deserialize a scene from the chosen file.
///
/// Returns `Some(Scene)` on success; records errors in `state.io_error`.
fn load_scene_dialog(state: &mut PanelState) -> Option<Scene> {
    state.io_error = None;

    let path = rfd::FileDialog::new()
        .set_title("Load Scene")
        .add_filter("JSON", &["json"])
        .pick_file()?; // user cancelled => None

    match std::fs::read_to_string(&path) {
        Err(e) => {
            state.io_error = Some(format!("Read failed: {e}"));
            None
        }
        Ok(text) => match serde_json::from_str::<Scene>(&text) {
            Ok(s) => Some(s),
            Err(e) => {
                state.io_error = Some(format!("Parse failed: {e}"));
                None
            }
        },
    }
}
