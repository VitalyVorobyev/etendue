//! WebAssembly facade for the etendue kernel — **P0-3 spike surface**.
//!
//! This crate exists to prove gate G0.1 (`docs/pivot/PLAN.md`): `etendue-core`
//! and `vision-calibration-core` build for `wasm32-unknown-unknown`, and the
//! projection chain returns bit-identical results under Node and natively.
//! The exported functions are placeholders. The real `@etendue/wasm` API
//! (`validate`, `bake`, `project_points`, `remap_lut`, the analyses) replaces
//! them in P2-1.
//!
//! The kernel call lives in a plain Rust function ([`project_points_world`])
//! so the native verifier (`examples/g01_verify.rs`) runs exactly the code the
//! `#[wasm_bindgen]` wrappers call. All projection math is
//! `vision-calibration-core`'s; this crate only marshals.

use etendue_core::Scene;
use nalgebra::Point3;
use wasm_bindgen::prelude::*;

/// Project world-frame points through one camera of `scene`.
///
/// `xyz_world` is a flat `[x0, y0, z0, x1, …]` array of world-frame points in
/// metres. Each point is mapped into the camera frame with the inverse of the
/// camera pose (`camera_se3_world = world_se3_camera⁻¹`) and projected by the
/// `vision-calibration-core` camera model built from the camera's
/// `CameraParams`.
///
/// Returns a flat `[u0, v0, u1, v1, …]` pixel array. A point the camera cannot
/// image (on or behind the camera plane, or rejected by the projection chain)
/// yields `NaN, NaN`.
///
/// # Errors
///
/// - [`etendue_core::Error::InvalidInput`] if `camera_index` is out of range
///   or `xyz_world.len()` is not a multiple of 3.
/// - [`etendue_core::Error::Calibration`] if the camera's projection spec
///   cannot be built (a singular sensor homography).
pub fn project_points_world(
    scene: &Scene,
    camera_index: usize,
    xyz_world: &[f64],
) -> etendue_core::Result<Vec<f64>> {
    let camera =
        scene
            .cameras
            .get(camera_index)
            .ok_or_else(|| etendue_core::Error::InvalidInput {
                reason: format!(
                    "camera index {camera_index} out of range (scene has {} cameras)",
                    scene.cameras.len()
                ),
            })?;
    if !xyz_world.len().is_multiple_of(3) {
        return Err(etendue_core::Error::InvalidInput {
            reason: format!(
                "point array length must be a multiple of 3, got {}",
                xyz_world.len()
            ),
        });
    }
    let model = camera.params.build()?;
    let camera_se3_world = camera.pose.inverse();
    let mut uv = Vec::with_capacity(xyz_world.len() / 3 * 2);
    for p in xyz_world.chunks_exact(3) {
        let p_cam = camera_se3_world * Point3::new(p[0], p[1], p[2]);
        match model.project_point_c(&p_cam.coords) {
            Some(px) => uv.extend_from_slice(&[px.x, px.y]),
            None => uv.extend_from_slice(&[f64::NAN, f64::NAN]),
        }
    }
    Ok(uv)
}

/// The default MVP scene ([`Scene::default_mvp`]) serialised as JSON.
///
/// # Errors
///
/// Throws if serialisation fails (not expected for the built-in scene).
#[wasm_bindgen]
pub fn default_mvp_scene_json() -> Result<String, JsError> {
    serde_json::to_string(&Scene::default_mvp()).map_err(|e| JsError::new(&e.to_string()))
}

/// JS entry point for [`project_points_world`]. `scene_json` is a serialised
/// [`Scene`]; `xyz_world` is a `Float64Array`; the result is a
/// `Float64Array` of pixel coordinates with `NaN` for unimageable points.
///
/// # Errors
///
/// Throws if `scene_json` does not parse as a [`Scene`], or on any
/// [`project_points_world`] error.
#[wasm_bindgen]
pub fn project_points(
    scene_json: &str,
    camera_index: usize,
    xyz_world: &[f64],
) -> Result<Vec<f64>, JsError> {
    let scene: Scene =
        serde_json::from_str(scene_json).map_err(|e| JsError::new(&e.to_string()))?;
    project_points_world(&scene, camera_index, xyz_world).map_err(|e| JsError::new(&e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_centre_projects_to_the_principal_point() {
        // The default camera looks straight at the target centre, so that
        // point lands on the principal point (cx, cy) = (640, 512).
        let scene = Scene::default_mvp();
        let uv = project_points_world(&scene, 0, &[0.0, 0.0, 0.30]).unwrap();
        assert!((uv[0] - 640.0).abs() < 1e-9, "u = {}", uv[0]);
        assert!((uv[1] - 512.0).abs() < 1e-9, "v = {}", uv[1]);
    }

    #[test]
    fn point_behind_the_camera_is_nan() {
        // The default camera sits at (0.28, -0.55, 0.30) looking toward
        // (0, 0, 0.30); a point further out along -view is behind it.
        let scene = Scene::default_mvp();
        let uv = project_points_world(&scene, 0, &[0.56, -1.10, 0.30]).unwrap();
        assert!(uv[0].is_nan() && uv[1].is_nan());
    }

    #[test]
    fn rejects_bad_camera_index_and_ragged_input() {
        let scene = Scene::default_mvp();
        assert!(matches!(
            project_points_world(&scene, 1, &[0.0, 0.0, 0.3]),
            Err(etendue_core::Error::InvalidInput { .. })
        ));
        assert!(matches!(
            project_points_world(&scene, 0, &[0.0, 0.0]),
            Err(etendue_core::Error::InvalidInput { .. })
        ));
    }

    #[test]
    fn json_round_trip_is_bit_exact() {
        // The wasm side receives the scene as JSON; serde_json's shortest
        // round-trip float formatting must not perturb a single bit of the
        // projected output.
        let scene = Scene::default_mvp();
        let reparsed: Scene = serde_json::from_str(&default_mvp_scene_json().unwrap()).unwrap();
        let xyz = [0.01, -0.02, 0.31, -0.05, 0.04, 0.27, 0.07, 0.0, 0.33];
        let a = project_points_world(&scene, 0, &xyz).unwrap();
        let b = project_points_world(&reparsed, 0, &xyz).unwrap();
        let bits = |v: &[f64]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
        assert_eq!(bits(&a), bits(&b));
    }
}
