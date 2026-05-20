//! Component bank: serializable specifications and a catalogue loader for
//! image sensors, lenses, and line lasers.
//!
//! The bank holds the *design-time* component specs a designer picks from when
//! setting up a triangulation sensor. It is intentionally separate from the
//! runtime [`Scene`](crate::scene::Scene) — the scene holds posed, parametric
//! entities; the bank holds the manufacturer-spec sheets those entities are
//! sized from.
//!
//! # Structure
//!
//! - [`schema`] — `SensorSpec`, `LensSpec`, `LaserSpec` with optional
//!   electro-optical fields and parametric line-width model.
//! - [`catalog`] — `Catalog` holding all three component families plus
//!   [`Catalog::load_from_str`] / [`Catalog::load_from_path`].
//!
//! Seed component JSON files live under `assets/bank/` in the
//! `etendue-core` crate; they are `include_str!`-embedded in the catalogue
//! unit tests to catch schema drift at compile time.
//!
//! # Component-picker UI
//!
//! Post-MVP. Schema + catalogue + seed files only for M3.

pub mod catalog;
pub mod schema;

pub use catalog::Catalog;
pub use schema::{
    GaussianBeamParams, LaserSpec, LensMount, LensSpec, LineWidthModel, MtfPlaceholder, SensorSpec,
};
