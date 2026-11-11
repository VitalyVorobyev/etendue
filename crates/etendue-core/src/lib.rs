//! Geometric and physical kernel for **etendue**: the physical camera model, the laser-line
//! model, triangulation heads and rig coverage.
//!
//! # Relationship to `vision-calibration-core`
//!
//! `etendue-core` does not reimplement the camera projection pipeline. It
//! depends on [`vision_calibration_core`] (crates.io; patched to a sibling
//! calibration-rs checkout for local development) and reuses its
//! `Camera` / `CameraModel` projection chain
//! (`pixel = K(sensor(distort(project(dir))))`), its `ScheimpflugParams`
//! tilted-sensor model, and — critically — its `nalgebra` type aliases.
//! Sharing one `nalgebra` instance across the crate boundary is mandatory:
//! poses and points cross that boundary as `Isometry3<f64>` / `Point3<f64>`,
//! and a second, semver-incompatible `nalgebra` would make those distinct
//! types.
//!
//! The numeric type is concrete `f64` throughout this crate; there is no
//! `S: RealField` genericity.
//!
//! # Modules
//!
//! - [`error`]: the crate's typed [`Error`] enum and [`Result`] alias.
//! - [`optics`]: [`optics::PhysicalCamera`] (ADR 0008) and a paraxial trace that checks it.
//! - [`from_scene`]: the bridge from `etendue-scene` documents to the camera model: a scene in
//!   projection frames, for everything that projects.
//! - [`head`]: a laser-triangulation head (a physical camera and a line laser on its body)
//!   analysed in its laser plane: blur, resolution, the Scheimpflug drawing, the optimiser.
//! - [`rig`]: a rig layout's coverage and each camera's sharp range.
//! - [`laser`]: the laser-line model — the fan plane, its Gaussian-beam width, and the stripe
//!   where it strikes a target ([`scene`]'s posed laser and target entities).
//! - [`geom`]: triangle meshes, for mesh targets.
//! - [`library`]: a library item's content hash (ADR 0010).
//! - [`solver`]: the in-tree Nelder–Mead.
//!
//! The egui-era analysis layer (working volume, defocus map, voxel overlap), the `Scene`
//! aggregate, `CameraEntity`, `ThickLens`, the CoC formulas and the Scheimpflug solver were removed
//! in P19f: nothing outside their own tests used them.

pub mod error;
pub mod from_scene;
pub mod geom;
pub mod head;
pub mod laser;
pub mod library;
pub mod optics;
pub mod rig;
pub mod scene;
pub mod solver;

pub use error::{Error, Result};
pub use scene::{LaserEntity, MeshTarget, TargetEntity};

/// Re-export of the upstream calibration kernel.
///
/// Re-exported (rather than left as an opaque dependency) so that downstream
/// crates and `etendue-core`'s own modules share one identical set of
/// `nalgebra` types and camera models across the crate boundary.
pub use vision_calibration_core as calibration;

#[cfg(test)]
mod tests {
    /// calibration-rs's pose type is this crate's: one nalgebra across the boundary (the hard
    /// pin), so a pose passes between them without conversion.
    #[test]
    fn calibration_types_are_this_crates_nalgebra() {
        let pose: crate::calibration::Iso3 = nalgebra::Isometry3::identity();
        let back: nalgebra::Isometry3<f64> = pose;
        assert_eq!(back, nalgebra::Isometry3::identity());
    }
}
