//! The laser-line model: the laser fan, its cross-section width, where it
//! strikes a target, and how that stripe projects into the camera.
//!
//! This is the M5 layer. It turns a posed [`LaserEntity`](crate::scene::LaserEntity)
//! into the geometry and physics of a projected laser line — the headless
//! companion to the UI's 3D laser stripe and 2D "simulated image" panel.
//! Everything here is pure `f64`, with no UI and no `wgpu` dependency.
//!
//! # The pipeline
//!
//! ```text
//! LaserEntity ──► LaserPlane ──┐
//!                              ├──► LaserStripe ──► ProjectedStripe
//! TargetEntity ────────────────┘    (intersect)      (project)
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
//!    plane intersected with a planar [`TargetEntity`](crate::scene::TargetEntity),
//!    clipped to the fan extent and the target rectangle, and sampled into a
//!    3D polyline with a per-sample width. **MVP scope: plane∩plane only — a
//!    single straight segment, no triangle-mesh stitching** (development plan,
//!    de-risking spike #3).
//! 4. [`project_stripe`] / [`ProjectedStripe`] ([`project`]) — the 3D stripe
//!    pushed through the scene camera into pixel space, with a per-vertex
//!    pixel width that combines the geometric cross-section and the M4 defocus
//!    blur **in quadrature**.

pub mod intersect;
pub mod plane;
pub mod project;
pub mod width;

pub use intersect::{LaserStripe, StripeSample, stripe_on_target, stripe_segments_on_mesh};
pub use plane::LaserPlane;
pub use project::{ProjectedPoint, ProjectedStripe, project_stripe};
pub use width::{GaussianBeamWidth, WidthModel};
