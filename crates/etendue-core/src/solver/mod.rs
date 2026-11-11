//! Solvers: an in-tree, deterministic Nelder–Mead ([`nelder_mead`]), which builds for wasm (no
//! `getrandom`, G9.3). The triangulation head's optimiser ([`crate::head`]) runs on it.
//!
//! The egui-era Scheimpflug solver (min-max CoC over a target, gate G9.2) was removed in P19f:
//! nothing called it once the head optimiser replaced it. G9.2's record
//! (`docs/measurements/`) stays as history.

pub mod nelder_mead;
