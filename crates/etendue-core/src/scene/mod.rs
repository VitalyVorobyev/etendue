//! The optical-design scene: posed cameras, lasers, and targets.
//!
//! A [`Scene`] is the data the whole tool revolves around — the UI edits it,
//! the renderer draws it, and the analysis layer consumes it. It holds three
//! entity kinds, each a parametric optical or geometric object with an
//! `Isometry3<f64>` pose:
//!
//! - [`CameraEntity`] — a simulated triangulation camera. Retains its
//!   `vision-calibration-core` [`CameraParams`](vision_calibration_core::CameraParams)
//!   spec (not a built model) so Scheimpflug tilt angles survive for M4.
//! - [`LaserEntity`] — a line-projecting laser, modelled minimally as a fan
//!   with a half-angle and an extent.
//! - [`TargetEntity`] — a planar inspection target.
//!
//! All entity types derive `Serialize`/`Deserialize`; M3 uses that for JSON
//! save/load. See [`entity`] for the per-entity coordinate conventions and
//! [`scene`] for the world frame and the default MVP scene.
//!
//! # Contents
//!
//! - [`entity`] — the three entity types and their constructors.
//! - [`scene`] — the [`Scene`] aggregate and [`Scene::default_mvp`].

pub mod entity;
// `scene::scene` repeats the module name, which `clippy::module_inception`
// flags by default. The name is deliberate: the crate's module layout (see
// the development plan) prescribes `scene/scene.rs` for the `Scene` aggregate
// alongside `scene/entity.rs` for the entity types, with `scene/mod.rs` as the
// re-export façade. The `Scene` type itself is re-exported below, so callers
// never write the doubled path.
#[allow(clippy::module_inception)]
pub mod scene;

pub use entity::{CameraEntity, LaserEntity, MeshTarget, PhysicalOptics, TargetEntity};
pub use scene::Scene;
