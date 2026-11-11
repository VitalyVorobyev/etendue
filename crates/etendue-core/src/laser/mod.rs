//! The laser-line model: the laser fan, its cross-section width, and where it strikes a target.
//!
//! It turns a posed [`LaserEntity`](crate::scene::LaserEntity) into the geometry of a laser line
//! on a target. Everything here is pure `f64`, with no rendering dependency. `etendue-synth`
//! images the stripe (its `laser` and `laser_render`), through the physical camera.
//!
//! # The pipeline
//!
//! ```text
//! LaserEntity ──► LaserPlane ──┐
//!                              ├──► LaserStripe
//! TargetEntity ────────────────┘    (intersect)
//!                                       ▲
//!                       WidthModel ─────┘
//! ```
//!
//! 1. [`LaserPlane`] ([`plane`]) — the geometric model of the fan: its plane
//!    (apex + normal), central direction, and angular/radial extent, all in
//!    world coordinates. Built from a [`LaserEntity`](crate::scene::LaserEntity).
//! 2. [`WidthModel`] / [`GaussianBeamWidth`] ([`width`]) — the laser's
//!    cross-section thickness versus distance from the origin. The trait is
//!    the per-component override hook; [`GaussianBeamWidth`] is the default
//!    Gaussian-beam law `w(d) = w0·sqrt(1 + (d/zR)²)`.
//! 3. [`stripe_on_target`] / [`LaserStripe`] ([`intersect`]) — the laser fan
//!    plane intersected with a [`TargetEntity`](crate::scene::TargetEntity) (a rectangle, a
//!    cylinder or a mesh), clipped to the fan extent and the target, and sampled into a 3D
//!    polyline with a per-sample width.

pub mod intersect;
pub mod plane;
pub mod width;

pub use intersect::{
    LaserStripe, StripeSample, stripe_on_cylinder, stripe_on_target, stripe_segments_on_mesh,
};
pub use plane::LaserPlane;
pub use width::{GaussianBeamWidth, WidthModel};
