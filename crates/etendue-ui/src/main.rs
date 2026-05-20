//! `etendue` — desktop tool for designing laser-triangulation optical systems.
//!
//! This binary is the application shell. It owns a hand-written winit event
//! loop and a wgpu + egui render stack — deliberately *not* `eframe`, because
//! later milestones add a custom 3D wgpu render pass interleaved with the
//! egui frame, which needs direct control of the device, surface, and
//! per-frame command encoding.
//!
//! M1 adds the hand-written 3D viewport (see [`viewport`]): a wgpu render
//! pass — depth buffer, orbit camera, grid + axes, flat-shaded and wireframe
//! meshes, an opaque and a translucent pass — drawn behind the egui frame.
//! The physics kernel lives in `etendue-core`; the parameter panels arrive in
//! later milestones.

mod app;
mod panels;
mod viewport;

use winit::event_loop::{ControlFlow, EventLoop};

use crate::app::App;

fn main() {
    // `RUST_LOG=etendue=info` (or similar) controls verbosity at runtime.
    env_logger::init();

    // `etendue-core` is wired in from the binary so the kernel <-> UI crate
    // boundary is exercised by `cargo build`; this also confirms the shared
    // nalgebra type set resolves all the way through to the binary.
    let probe = etendue_core::probe_calibration_link();
    log::info!(
        "etendue-core link check: on-axis pixel = ({:.1}, {:.1})",
        probe.x,
        probe.y,
    );

    let event_loop = EventLoop::new().expect("failed to create the winit event loop");
    // Continuous redraw: the app re-requests a redraw every iteration, so the
    // loop should poll rather than wait for OS events.
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new();
    event_loop
        .run_app(&mut app)
        .expect("the winit event loop exited with an error");
}
