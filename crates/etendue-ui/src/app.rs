//! Application state and the per-frame render orchestration.
//!
//! [`App`] owns the winit window plus the full graphics stack — a wgpu
//! device/queue/surface, the hand-written 3D [`viewport::Renderer`], and the
//! egui context/state/renderer.
//!
//! # Per-frame structure (M3 / M4)
//!
//! Each frame [`Graphics::render`] records, into one command encoder:
//!
//! 1. The egui frame — calls [`panels::params::scene_panel`] which draws the
//!    parameter side-panel and returns `(changed, maybe_new_scene)`. If
//!    `changed` is set, the M4 defocus map is recomputed from the (now
//!    post-edit) scene and `Renderer::rebuild_scene` is called with it, so
//!    both the geometry and the heatmap reflect slider edits within the same
//!    frame. If `maybe_new_scene` is `Some`, the live scene is replaced and
//!    the viewport is rebuilt.
//! 2. The 3D viewport pass — clears color + depth, draws the opaque scene
//!    (ground grid, world axes, camera frustum wireframes, the target — as
//!    the plain quad or the M4 defocus heatmap grid — and entity-origin
//!    markers) then the translucent pass (the laser fans).
//! 3. The egui render pass — *loads* the surface (does not clear) and paints
//!    the UI on top of the finished 3D image.
//!
//! # The M4 defocus heatmap
//!
//! When the panel's heatmap toggle is on, [`Graphics`] computes a
//! [`DefocusMap`](etendue_core::analysis::DefocusMap) from the scene's first
//! camera against its first target via
//! [`defocus_map`](etendue_core::analysis::defocus_map). The map backs both
//! the legend (drawn by `scene_panel`) and the heatmap grid (built by the
//! renderer). It is recomputed every frame the scene changes — the scene is
//! tiny, so this is cheap — and re-fed to `rebuild_scene` alongside the
//! geometry rebuild.
//!
//! Mouse input drives the [`OrbitCamera`]. It is gated on egui's pointer
//! capture: when the cursor is over the egui panel, egui consumes the event
//! and the camera does not move.

use std::sync::Arc;

use egui_wgpu::{Renderer as EguiRenderer, RendererOptions, ScreenDescriptor};
use etendue_core::Scene;
use etendue_core::analysis::{
    DEFAULT_COC_THRESHOLD_PX, DefocusMap, WorkingVolume, defocus_map, working_volume,
};
use etendue_core::laser::{
    GaussianBeamWidth, LaserPlane, ProjectedStripe, project_stripe, stripe_on_target,
};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::window::Window;

use crate::panels::image_view::simulated_image_panel;
use crate::panels::params::{PanelState, scene_panel};
use crate::viewport::{OrbitCamera, Renderer};

/// Sampling resolution of the M4 defocus heatmap, `cols × rows`.
///
/// 64×64 ≈ 4k samples — fine enough that the heatmap grid reads as a smooth
/// field and the in-focus band has a clean edge, cheap enough to recompute on
/// every scene change (each sample is a handful of `f64` operations).
const DEFOCUS_MAP_RESOLUTION: usize = 64;

/// Number of polyline samples for the projected laser line (M5).
///
/// The MVP stripe is one straight segment, so the projected line is sampled
/// densely enough that the width-encoding band reads smoothly and the defocus
/// falloff along the line is resolved — still a trivial cost (a few dozen
/// projections) to recompute on every scene change.
const PROJECTED_STRIPE_SAMPLES: usize = 96;

/// Sampling resolution of the M6 working volume, `rows × cols` (fan angle ×
/// radius).
///
/// 128×128 ≈ 16k samples — fine enough that the in-volume region's boundary
/// reads as a clean curve on the rendered patch and the area/depth-range
/// readouts agree to a couple percent with the polygonal limit, cheap enough
/// to recompute on every scene change (each sample is a handful of `f64`
/// operations plus one projection through the camera).
const WORKING_VOLUME_RESOLUTION: usize = 128;

/// Compute the M4 defocus map for a scene's first camera against its first
/// target, or `None` if the scene has no camera/target or the map could not
/// be computed.
///
/// The `etendue-core` `defocus_map` analysis pairs one camera with one
/// target; the MVP scene has exactly one of each, so this drives the heatmap
/// off `cameras[0]` / `targets[0]`. A `None` result simply leaves the target
/// drawn as the plain quad — it never aborts a frame.
fn compute_defocus_map(scene: &Scene) -> Option<DefocusMap> {
    let camera = scene.cameras.first()?;
    let target = scene.targets.first()?;
    defocus_map(
        camera,
        target,
        DEFOCUS_MAP_RESOLUTION,
        DEFOCUS_MAP_RESOLUTION,
    )
    .ok()
}

/// Compute the M6 working volume for a scene's first camera + first laser,
/// or `None` if either is missing or the analysis cannot be set up.
///
/// The working volume is the set of points on the laser fan plane that are
/// simultaneously illuminated by the laser, visible to the camera, and in
/// focus (CoC ≤ [`DEFAULT_COC_THRESHOLD_PX`]). It backs both the M6
/// translucent patch on the laser fan and the area / depth-range readouts
/// in the parameter panel.
fn compute_working_volume(scene: &Scene) -> Option<WorkingVolume> {
    let camera = scene.cameras.first()?;
    let laser = scene.lasers.first()?;
    working_volume(
        camera,
        laser,
        WORKING_VOLUME_RESOLUTION,
        WORKING_VOLUME_RESOLUTION,
        DEFAULT_COC_THRESHOLD_PX,
    )
    .ok()
}

/// Compute the M5 projected laser line for a scene's first laser/target,
/// imaged by its first camera — the data behind the simulated-image panel.
///
/// Builds the laser fan plane, intersects it with the target to get the 3D
/// stripe, and projects that stripe through the camera (geometric ⊕ defocus
/// width in quadrature). Returns `None` when the scene lacks a camera, laser,
/// or target, when the fan misses the target, or when the projection cannot
/// be set up — in every case the panel simply shows an empty sensor frame.
fn compute_projected_stripe(scene: &Scene) -> Option<ProjectedStripe> {
    let camera = scene.cameras.first()?;
    let laser = scene.lasers.first()?;
    let target = scene.targets.first()?;

    let plane = LaserPlane::from_entity(laser);
    let width_model = GaussianBeamWidth::from_laser(laser).ok()?;
    let stripe = stripe_on_target(&plane, target, &width_model, PROJECTED_STRIPE_SAMPLES)?;
    project_stripe(&stripe, camera).ok()
}

/// Which mouse button started the in-progress viewport drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragButton {
    /// Primary button: orbit the camera.
    Orbit,
    /// Secondary/middle button: pan the camera target.
    Pan,
}

/// Mouse-drag tracking for the orbit camera.
///
/// A drag is "armed" only when the press landed on the viewport (egui did not
/// consume the press); a press over an egui panel never starts one.
#[derive(Debug, Default)]
struct DragState {
    /// The active drag, if a button is held over the viewport.
    button: Option<DragButton>,
    /// Last observed cursor position, physical pixels — used to form drag
    /// deltas. `None` until the first `CursorMoved`.
    last_cursor: Option<(f32, f32)>,
}

/// The live graphics stack — created once the window exists, on `resumed`.
///
/// winit 0.30 only guarantees a usable window after `resumed`, so the GPU
/// resources cannot be built in `App::new`; they live here behind an
/// `Option` and are initialized in [`App::resumed`].
struct Graphics {
    /// The OS window. An `Arc` so the wgpu surface can outlive this struct's
    /// stack frame and borrow the same handle.
    window: Arc<Window>,
    /// wgpu surface bound to `window`.
    surface: wgpu::Surface<'static>,
    /// Logical GPU device.
    device: wgpu::Device,
    /// Command queue for `device`.
    queue: wgpu::Queue,
    /// Current swapchain configuration; mutated on resize.
    surface_config: wgpu::SurfaceConfiguration,
    /// The optical-design scene — the cameras, lasers, and targets the
    /// renderer draws. M3 adds the editing UI that mutates it and rebuilds
    /// the viewport each frame that any widget changed.
    scene: Scene,
    /// The hand-written 3D viewport renderer (depth pass, scene drawables).
    viewport: Renderer,
    /// The user's orbit/pan/zoom viewpoint onto the 3D scene.
    camera: OrbitCamera,
    /// In-progress mouse drag for the orbit camera.
    drag: DragState,
    /// egui input + platform integration for this window.
    egui_state: egui_winit::State,
    /// egui-wgpu paint backend.
    egui_renderer: EguiRenderer,
    /// Persistent UI state for the parameter panel (collapsible sections,
    /// last I/O error message).
    panel_state: PanelState,
    /// The M5 projected laser line backing the simulated-image panel. Cached
    /// and recomputed on every scene change, alongside the defocus map, so the
    /// 2D panel stays in step with the 3D scene. `None` when the laser line is
    /// not imageable (no camera/laser/target, or the fan misses the target).
    projected_stripe: Option<ProjectedStripe>,
    /// The M6 working volume backing both the translucent patch on the laser
    /// fan and the parameter panel's area / depth-range readouts. Cached and
    /// recomputed on every scene change, alongside the defocus map and the
    /// projected stripe. `None` when the scene has no camera/laser to compute
    /// it from, or when the analysis itself failed.
    working_volume: Option<WorkingVolume>,
}

impl Graphics {
    /// Build the whole graphics stack for a freshly created `window`.
    ///
    /// Blocks on the async wgpu adapter/device requests with `pollster`; a
    /// desktop event loop has no async runtime, and these run once at startup.
    fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        // Guard against a zero-sized surface (can happen briefly on some
        // window managers): a zero width/height makes `configure` panic.
        let width = size.width.max(1);
        let height = size.height.max(1);

        // Default backends => wgpu selects Metal on macOS. The `_from_env`
        // variant also honours `WGPU_BACKEND` etc. if the user sets them.
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());

        let surface = instance
            .create_surface(window.clone())
            .expect("failed to create a wgpu surface for the window");

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        }))
        .expect("no compatible wgpu adapter found");

        log::info!(
            "wgpu adapter: {}",
            egui_wgpu::adapter_info_summary(&adapter.get_info())
        );

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("etendue-device"),
            ..Default::default()
        }))
        .expect("failed to create the wgpu device");

        let surface_config = surface
            .get_default_config(&adapter, width, height)
            .expect("the surface is not supported by the selected adapter");
        surface.configure(&device, &surface_config);

        // The optical-design scene the renderer draws: M3 starts from the
        // default triangulation arrangement (one camera, one laser, one
        // target) but immediately becomes user-editable via the side panel.
        let scene = Scene::default_mvp();

        // The 3D viewport renderer: its pipelines target the surface format,
        // it owns a depth texture sized to the current surface, and its
        // drawables are built from `scene`.
        let mut viewport = Renderer::new(&device, surface_config.format, width, height, &scene);

        // The parameter-panel state opens with both the heatmap and the
        // working-volume toggles on (the M4 + M6 deliverables' headline
        // visuals), so build the initial defocus map and working volume and
        // rebuild the viewport once with them — otherwise the first frame
        // would draw a plain quad and no working-volume patch until the
        // first slider edit.
        let panel_state = PanelState::default();
        let initial_map = if panel_state.show_heatmap {
            compute_defocus_map(&scene)
        } else {
            None
        };
        let initial_volume = if panel_state.show_working_volume {
            compute_working_volume(&scene)
        } else {
            None
        };
        viewport.rebuild_scene(
            &device,
            &scene,
            initial_map.as_ref(),
            initial_volume.as_ref(),
        );

        // The M5 projected laser line backing the simulated-image panel.
        // Computed once up front so the panel is populated on the first frame.
        let projected_stripe = compute_projected_stripe(&scene);
        // Move the initial working volume into the persistent cache so the
        // panel's status readout is populated on the first frame.
        let working_volume_cache = initial_volume;

        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx,
            egui::ViewportId::ROOT,
            window.as_ref(),
            Some(window.scale_factor() as f32),
            None,
            None,
        );

        let egui_renderer =
            EguiRenderer::new(&device, surface_config.format, RendererOptions::default());

        Self {
            window,
            surface,
            device,
            queue,
            surface_config,
            scene,
            viewport,
            camera: OrbitCamera::default(),
            drag: DragState::default(),
            egui_state,
            egui_renderer,
            panel_state,
            projected_stripe,
            working_volume: working_volume_cache,
        }
    }

    /// Resize the swapchain — and the viewport's depth texture — to
    /// `width` x `height` physical pixels.
    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);
        // The depth attachment must match the new surface size.
        self.viewport.resize(&self.device, width, height);
    }

    /// Handle a mouse event for the orbit camera.
    ///
    /// `egui_consumed` is true when egui has already claimed the event (the
    /// cursor is over a panel/widget); in that case the camera ignores it, so
    /// dragging a slider never also orbits the scene. A drag is only armed if
    /// the *press* itself landed on the viewport.
    fn handle_camera_input(&mut self, event: &WindowEvent, egui_consumed: bool) {
        match event {
            WindowEvent::MouseInput { state, button, .. } => match state {
                ElementState::Pressed => {
                    // Arm a drag only for a press that landed on the viewport.
                    if egui_consumed {
                        return;
                    }
                    self.drag.button = match button {
                        MouseButton::Left => Some(DragButton::Orbit),
                        MouseButton::Right | MouseButton::Middle => Some(DragButton::Pan),
                        _ => None,
                    };
                }
                ElementState::Released => {
                    // A release always ends the drag, even over a panel.
                    self.drag.button = None;
                }
            },
            WindowEvent::CursorMoved { position, .. } => {
                let pos = (position.x as f32, position.y as f32);
                if let (Some(drag_button), Some((px, py))) =
                    (self.drag.button, self.drag.last_cursor)
                {
                    let (dx, dy) = (pos.0 - px, pos.1 - py);
                    match drag_button {
                        DragButton::Orbit => self.camera.orbit(dx, dy),
                        DragButton::Pan => self.camera.pan(dx, dy),
                    }
                }
                self.drag.last_cursor = Some(pos);
            }
            WindowEvent::CursorLeft { .. } => {
                // Drop the stale cursor anchor so re-entry does not jump.
                self.drag.last_cursor = None;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                // Scrolling over an egui panel (e.g. a scroll area) belongs to
                // egui; only zoom when the viewport owns the pointer.
                if egui_consumed {
                    return;
                }
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    // Trackpad pixel scrolling: scale to a comparable notch.
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 50.0,
                };
                self.camera.zoom(lines);
            }
            _ => {}
        }
    }

    /// Render one frame: the 3D viewport, then the egui UI over it.
    fn render(&mut self) {
        // Acquire the next swapchain image. A lost/outdated surface just
        // means "reconfigure and skip this frame"; transient states (timeout,
        // occluded) are skipped without reconfiguring.
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.surface_config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                log::error!("wgpu surface acquisition raised a validation error");
                return;
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // --- Run the egui frame (build UI, get shapes + texture deltas) ----
        //
        // M3: the closure mutates `self.scene` through `scene_panel` and
        // returns whether any widget changed this frame. We pull the fields
        // we need out of `self` before the closure to satisfy the borrow
        // checker: `scene_panel` takes `&mut Scene` and `&mut PanelState`,
        // so we hand them by reference inside the closure while the rest of
        // `self` is still accessible.
        let raw_input = self.egui_state.take_egui_input(self.window.as_ref());
        let egui_ctx = self.egui_state.egui_ctx().clone();

        // We need &mut self.scene and &mut self.panel_state inside run_ui,
        // but run_ui takes a Fn, not FnMut. Work around with Option<> swap.
        let mut scene_ref = std::mem::replace(&mut self.scene, Scene::empty());
        let mut panel_state_ref = std::mem::take(&mut self.panel_state);
        let mut scene_changed = false;
        let mut loaded_scene: Option<Scene> = None;

        // The defocus map backing this frame's legend. The panel runs *after*
        // this is computed, so a slider edited this frame is reflected in the
        // legend only next frame — imperceptible, and the heatmap geometry
        // itself is rebuilt from a fresh post-edit map below.
        let legend_map = if panel_state_ref.show_heatmap {
            compute_defocus_map(&scene_ref)
        } else {
            None
        };

        // The M5 projected laser line for this frame's simulated-image panel.
        // Borrowed read-only out of `self` so the closure can still mutate
        // `scene_ref` / `panel_state_ref`. Lives across frames (cache in
        // `self.projected_stripe`); the post-edit value is recomputed below
        // when the scene changes, mirroring the heatmap rebuild path.
        let projected_stripe_ref = self.projected_stripe.as_ref();
        // The M6 working volume for this frame's parameter-panel status
        // readout. Same pattern: a one-frame-stale cache that is refreshed
        // *after* the scene mutates this frame so the next frame shows the
        // post-edit numbers.
        let working_volume_ref = self.working_volume.as_ref();

        let full_output = egui_ctx.run_ui(raw_input, |ui| {
            // Params side panel goes first so it spans the full window height;
            // the bottom dock (the simulated-image panel) then occupies only
            // the space to the right of the params panel.
            let (changed, maybe_new) = scene_panel(
                ui,
                &mut scene_ref,
                &mut panel_state_ref,
                legend_map.as_ref(),
                working_volume_ref,
            );
            scene_changed = changed;
            loaded_scene = maybe_new;

            // The M5 simulated-image panel: a bottom dock showing the
            // projected laser line in pixel space, with thickness encoding
            // the imaged line width (geometric ⊕ defocus). The sensor frame
            // is read from the live (post-edit) scene so a resolution edit
            // would reflect immediately.
            simulated_image_panel(ui, projected_stripe_ref, scene_ref.cameras.first());
        });

        // Restore mutably-borrowed fields.
        self.scene = scene_ref;
        self.panel_state = panel_state_ref;

        // If the user loaded a scene, replace the live scene.
        if let Some(new_scene) = loaded_scene {
            self.scene = new_scene;
            self.panel_state = PanelState::default();
            scene_changed = true; // force a rebuild
        }

        // Rebuild viewport drawables if any slider/drag changed this frame —
        // or the heatmap / working-volume toggle flipped (each swaps a piece
        // of geometry). Rebuilding every changed frame is correct and cheap:
        // the scene is small, focal-length / optics / fan-angle edits change
        // geometry (not just transforms), and recomputing the defocus map +
        // working volume is a few tens of thousands of `f64` operations.
        // Both are recomputed here from the *post-edit* scene so the heatmap
        // and the patch reflect this frame's slider changes.
        if scene_changed {
            let map = if self.panel_state.show_heatmap {
                compute_defocus_map(&self.scene)
            } else {
                None
            };
            let volume = if self.panel_state.show_working_volume {
                compute_working_volume(&self.scene)
            } else {
                None
            };
            self.viewport
                .rebuild_scene(&self.device, &self.scene, map.as_ref(), volume.as_ref());
            // M5: refresh the cached projected laser line for the
            // simulated-image panel on the same reactivity path as the
            // heatmap. The 2D panel reads `self.projected_stripe` next frame,
            // so the line broadens/shifts in step with the 3D stripe.
            self.projected_stripe = compute_projected_stripe(&self.scene);
            // M6: cache the recomputed working volume so the panel's status
            // readout (area, depth range) reads the post-edit numbers on the
            // next frame, on the same reactivity path.
            self.working_volume = volume;
        }

        self.egui_state
            .handle_platform_output(self.window.as_ref(), full_output.platform_output);

        let paint_jobs = egui_ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [self.surface_config.width, self.surface_config.height],
            pixels_per_point: full_output.pixels_per_point,
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("etendue-frame-encoder"),
            });

        // --- 3D viewport pass: clears color + depth, draws the scene -------
        // Recorded first so the egui pass below can load (not clear) the
        // result and paint the UI on top.
        self.viewport.render(
            &self.queue,
            &mut encoder,
            &view,
            &self.camera,
            self.surface_config.width,
            self.surface_config.height,
        );

        // Upload any new/changed egui textures before painting.
        for (id, image_delta) in &full_output.textures_delta.set {
            self.egui_renderer
                .update_texture(&self.device, &self.queue, *id, image_delta);
        }
        self.egui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &paint_jobs,
            &screen_descriptor,
        );

        // --- egui pass: load the 3D image, paint the UI on top ------------
        // `LoadOp::Load` preserves the viewport's rendering; `egui-wgpu`'s
        // `render` needs a `'static` render pass, hence `forget_lifetime()`.
        {
            let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("etendue-egui-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let mut render_pass = render_pass.forget_lifetime();
            self.egui_renderer
                .render(&mut render_pass, &paint_jobs, &screen_descriptor);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        self.window.pre_present_notify();
        frame.present();

        // Free textures egui retired this frame, after the GPU work is queued.
        for id in &full_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }
    }
}

/// Top-level application: a winit [`ApplicationHandler`] that owns the
/// graphics stack once the window is created.
///
/// [`ApplicationHandler`]: winit::application::ApplicationHandler
#[derive(Default)]
pub struct App {
    /// `None` until `resumed` builds the window and GPU resources.
    graphics: Option<Graphics>,
}

impl App {
    /// Create an app with no window yet; the window is built on `resumed`.
    pub fn new() -> Self {
        Self::default()
    }
}

impl winit::application::ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        // `resumed` can fire more than once; only build the stack once.
        if self.graphics.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("etendue")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 800.0));
        let window = event_loop
            .create_window(attributes)
            .expect("failed to create the application window");
        self.graphics = Some(Graphics::new(Arc::new(window)));
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        let Some(graphics) = self.graphics.as_mut() else {
            return;
        };

        // Let egui consume the event first (text fields, buttons, etc.).
        // `consumed` is true when the pointer is over an egui area/widget;
        // the orbit camera uses that to ignore input meant for the UI.
        let response = graphics
            .egui_state
            .on_window_event(graphics.window.as_ref(), &event);

        // Feed mouse events to the orbit camera, gated by egui's capture.
        graphics.handle_camera_input(&event, response.consumed);

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                graphics.resize(size.width, size.height);
                graphics.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                graphics.render();
            }
            _ => {}
        }

        // If egui wants another frame (animation, hover, etc.), schedule one.
        if response.repaint {
            graphics.window.request_redraw();
        }
    }

    fn about_to_wait(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        // Drive a continuous redraw loop so orbiting and egui animations stay
        // live.
        if let Some(graphics) = self.graphics.as_ref() {
            graphics.window.request_redraw();
        }
    }
}
