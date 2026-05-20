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
//! - [`geom`]: pure geometry — triangle meshes and ray–triangle intersection.
//! - [`scene`]: the posed-entity scene — cameras, lasers, and targets.
//! - [`optics`]: the thick-lens defocus physics — conjugates, the Scheimpflug
//!   plane of best focus, and the geometric circle of confusion.
//! - [`laser`]: the laser-line model — the laser fan plane, its Gaussian-beam
//!   cross-section width, the stripe where it strikes a target, and the
//!   projection of that stripe into the camera's pixel space.
//! - [`analysis`]: derived quantities over a scene — currently the per-target
//!   defocus map.

pub mod analysis;
pub mod bank;
pub mod error;
pub mod geom;
pub mod laser;
pub mod optics;
pub mod scene;

pub use error::{Error, Result};
pub use optics::ThickLens;
pub use scene::{CameraEntity, LaserEntity, Scene, TargetEntity};

/// Re-export of the upstream calibration kernel.
///
/// Re-exported (rather than left as an opaque dependency) so that downstream
/// crates — chiefly `etendue-ui` — and `etendue-core`'s own future modules
/// share one identical set of `nalgebra` types and camera models across the
/// path boundary.
pub use vision_calibration_core as calibration;

/// Smoke check that the `vision-calibration-core` path dependency is wired up
/// and that `nalgebra` 0.34 unifies across the path boundary.
///
/// This builds a trivial pinhole [`CameraParams`](vision_calibration_core::CameraParams),
/// compiles it into a
/// [`CameraModel`](vision_calibration_core::CameraModel) via `build()`, and
/// projects a camera-frame point — exercising types (`CameraParams`,
/// `CameraModel`, `nalgebra::Vector3<f64>`, `nalgebra::Point2<f64>`) that
/// originate in the dependency. It exists as an M0 de-risking probe and will
/// be superseded by the real `optics` / `scene` modules.
///
/// Returns the projected pixel for a point on the camera's optical axis at
/// unit depth, which lands at the principal point.
///
/// # Examples
///
/// ```
/// use etendue_core::probe_calibration_link;
///
/// // A point straight ahead at z = 1 projects to the principal point.
/// let px = probe_calibration_link();
/// assert!((px.x - 640.0).abs() < 1e-9);
/// assert!((px.y - 360.0).abs() < 1e-9);
/// ```
pub fn probe_calibration_link() -> nalgebra::Point2<f64> {
    use vision_calibration_core::{
        CameraParams, DistortionParams, FxFyCxCySkew, IntrinsicsParams, ProjectionParams,
        SensorParams,
    };

    let params = CameraParams {
        projection: ProjectionParams::Pinhole,
        distortion: DistortionParams::None,
        sensor: SensorParams::Identity,
        intrinsics: IntrinsicsParams::FxFyCxCySkew {
            params: FxFyCxCySkew {
                fx: 800.0,
                fy: 800.0,
                cx: 640.0,
                cy: 360.0,
                skew: 0.0,
            },
        },
    };

    // `build()` lives in vision-calibration-core; `project_point_c` rejects
    // z <= 0, so a point at z = 1 is the simplest valid input.
    let camera = params.build();
    let p_c = nalgebra::Vector3::new(0.0, 0.0, 1.0);
    camera
        .project_point_c(&p_c)
        .expect("on-axis point at z = 1 must project")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calibration_path_dep_resolves_and_projects() {
        let px = probe_calibration_link();
        assert!((px.x - 640.0).abs() < 1e-9, "px.x = {}", px.x);
        assert!((px.y - 360.0).abs() < 1e-9, "px.y = {}", px.y);
    }
}
