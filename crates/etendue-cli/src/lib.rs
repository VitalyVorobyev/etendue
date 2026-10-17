//! The etendue pipeline behind the `etendue` CLI, as a library so the studio's
//! Tauri shell runs the same code: load a scene, write its ground truth,
//! render it with Blender, detect the board, and build scenarios from tool
//! poses. Long steps report through a [`progress::Control`] and can be
//! cancelled.
//!
//! Nothing in the workspace depends on this crate (`cargo xtask
//! check-layering`); it sits on top of the library crates.

pub mod detect;
pub mod gt;
pub mod load;
pub mod poses;
pub mod progress;
pub mod render;

pub use load::{Loaded, bake_file, load, read_json};
