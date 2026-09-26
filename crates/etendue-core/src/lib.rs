//! Geometric and physical kernel for **etendue** — an interactive tool for
//! designing laser-triangulation optical systems.
//!
//! This crate holds the headless, UI-free core: scene geometry, the thick-lens
//! and circle-of-confusion optics model, the laser-line model, and the
//! analysis layer that turns a posed camera + laser + target into a working
//! volume, a defocus map, and a projected laser line.
//!
//! # Relationship to `vision-calibration-core`
//!
//! `etendue-core` does not reimplement the camera projection pipeline. It
//! depends, by path, on [`vision_calibration_core`] and reuses its
//! `Camera` / `CameraModel` projection chain
//! (`pixel = K(sensor(distort(project(dir))))`), its `ScheimpflugParams`
//! tilted-sensor model, and — critically — its `nalgebra` type aliases.
//! Sharing one `nalgebra` instance across the path boundary is mandatory:
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
//! - [`geom`]: pure geometry — triangle meshes and primitive constructors.
//! - [`scene`]: the posed-entity scene — cameras, lasers, and targets.
//! - [`optics`]: the thick-lens defocus physics — conjugates, the Scheimpflug
//!   plane of best focus, and the geometric circle of confusion.
//! - [`laser`]: the laser-line model — the laser fan plane, its Gaussian-beam
//!   cross-section width, the stripe where it strikes a target, and the
//!   projection of that stripe into the camera's pixel space.
//! - [`analysis`]: derived quantities over a scene — currently the per-target
//!   defocus map.
//! - [`solver`]: design-space solvers — the M9 Scheimpflug solver that
//!   proposes optimal sensor tilt and focus distance for a target working
//!   geometry.

pub mod analysis;
pub mod bank;
pub mod error;
pub mod geom;
pub mod laser;
pub mod optics;
pub mod scene;
pub mod solver;

pub use error::{Error, Result};
pub use optics::ThickLens;
pub use scene::{CameraEntity, LaserEntity, MeshTarget, PhysicalOptics, Scene, TargetEntity};

/// Re-export of the upstream calibration kernel.
///
/// Re-exported (rather than left as an opaque dependency) so that downstream
/// crates — chiefly `etendue-ui` — and `etendue-core`'s own future modules
/// share one identical set of `nalgebra` types and camera models across the
/// path boundary.
pub use vision_calibration_core as calibration;

#[cfg(test)]
mod tests {
    /// Smoke-check that vision-calibration-core is wired and nalgebra unifies.
    ///
    /// Previously this called `probe_calibration_link()` directly; now it uses
    /// `Scene::default_mvp()` which exercises the same path-dep boundary while
    /// also covering the real scene construction contract.
    #[test]
    fn calibration_path_dep_resolves() {
        let scene = crate::Scene::default_mvp();
        assert_eq!(
            scene.cameras.len(),
            1,
            "default MVP scene must have 1 camera"
        );
        assert_eq!(scene.lasers.len(), 1, "default MVP scene must have 1 laser");
        assert_eq!(
            scene.targets.len(),
            1,
            "default MVP scene must have 1 target"
        );
    }
}
