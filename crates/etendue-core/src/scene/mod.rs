//! The posed laser and target entities ([`entity`]) that the laser stripe model consumes.
//! Cameras are [`crate::optics::PhysicalCamera`] (ADR 0008).

pub mod entity;

pub use entity::{LaserEntity, MeshTarget, TargetEntity};
