//! egui UI panels for the etendue application.
//!
//! - [`params`] — the parameter side-panel: a `Panel` with Save/Load buttons,
//!   the M4 defocus-heatmap toggle + color-scale legend, and collapsible
//!   sections per scene entity (camera, laser, target). The camera section
//!   carries the M4 physical-optics sliders (effective focal length,
//!   f-number, focus distance, principal-plane gap); the laser section
//!   carries the M5 beam-waist slider.
//! - [`image_view`] — the M5 "simulated image" panel: a bottom dock that
//!   draws the projected laser line in pixel space (via `egui_plot`), with
//!   per-point thickness encoding the imaged line width.

pub mod image_view;
pub mod params;
