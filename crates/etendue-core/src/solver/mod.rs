//! Design-space solvers — turn etendue from a viewer into a designer.
//!
//! The M9 [`scheimpflug`] solver takes a target working geometry — optimal
//! working distance plus a depth-of-field window — and proposes the camera
//! Scheimpflug tilt and focus distance that minimise the worst-case circle of
//! confusion across the working volume. Physics is reused from
//! [`crate::optics::coc`]; this module wraps it in a min-max optimisation.
//!
//! # Scope (M9)
//!
//! - **Decision variables**: `tau_x`, `tau_y` (sensor tilt about camera `x` /
//!   `y`), `s_o` (object-side focus distance from `H'`).
//! - **Held fixed**: focal length, f-number, principal-plane gap, pixel pitch,
//!   sensor resolution, camera pose, laser plane geometry.
//! - **Objective**: minimise `max(CoC_px)` over the visible samples of the
//!   laser plane that fall within the depth window.
//! - **Solver**: Nelder-Mead (no gradient — the cost is non-smooth at
//!   visibility / depth-window boundaries).
//!
//! Larger design spaces (varying baseline or f-number) and smoother objectives
//! (mean CoC, in-focus volume) are explicit out-of-scope deferrals — see
//! `docs/handoff.md` and the project roadmap.

pub mod scheimpflug;

pub use scheimpflug::{SampleGrid, SolverError, SolverResult, SolverSpec, solve_scheimpflug};
