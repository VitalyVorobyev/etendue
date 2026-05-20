//! The viewport's orbit / pan / zoom camera.
//!
//! This is the **user's** viewpoint onto the 3D scene — the thing the mouse
//! drives. It is emphatically *not* a simulated sensor: the optical camera
//! being designed lives in `etendue-core::scene` and is rendered as scene
//! geometry. This struct therefore belongs in the UI crate and never leaks
//! into the kernel.
//!
//! The camera is parameterised as a point orbiting a `target`: a `yaw`,
//! `pitch`, and `distance` in spherical coordinates. Mouse-drag changes
//! yaw/pitch, secondary-drag pans the `target`, scroll changes `distance`.
//! [`OrbitCamera::view_proj`] composes the view and a right-handed
//! perspective projection into a single matrix for the shaders.
//!
//! All math is `f32` — this never touches the `f64` kernel.

use nalgebra::{Matrix4, Perspective3, Point3, Vector3};

/// Smallest allowed orbit distance — keeps the eye from passing through the
/// target and the projection from degenerating.
const MIN_DISTANCE: f32 = 0.05;
/// Largest allowed orbit distance.
const MAX_DISTANCE: f32 = 500.0;
/// Pitch is clamped just shy of straight up/down so the view basis never
/// becomes singular (eye direction parallel to world up).
const MAX_PITCH: f32 = std::f32::consts::FRAC_PI_2 - 0.01;

/// Orbit camera state plus the tuning constants for its mouse mapping.
#[derive(Debug, Clone)]
pub struct OrbitCamera {
    /// The point the camera orbits and looks at, in world space.
    target: Point3<f32>,
    /// Azimuth around the world `+Z` (up) axis, radians.
    yaw: f32,
    /// Elevation above the horizon, radians; clamped to `±MAX_PITCH`.
    pitch: f32,
    /// Distance from `target` to the eye.
    distance: f32,
    /// Vertical field of view, radians.
    fov_y: f32,
    /// Near clip plane.
    near: f32,
    /// Far clip plane.
    far: f32,
    /// Radians of orbit per pixel of primary drag.
    orbit_speed: f32,
    /// World units of pan per pixel, scaled by `distance` at apply time.
    pan_speed: f32,
    /// Multiplicative zoom factor per scroll-line.
    zoom_speed: f32,
}

impl Default for OrbitCamera {
    /// A camera looking at the origin from above and to the side — a good
    /// default for inspecting the M1 demo scene.
    fn default() -> Self {
        Self {
            target: Point3::origin(),
            yaw: std::f32::consts::FRAC_PI_4,
            pitch: 0.45,
            distance: 9.0,
            fov_y: 50.0_f32.to_radians(),
            near: 0.05,
            far: 800.0,
            orbit_speed: 0.008,
            pan_speed: 0.0016,
            zoom_speed: 0.12,
        }
    }
}

impl OrbitCamera {
    /// World up axis. The scene is modelled `+Z`-up (matches the kernel's
    /// optical conventions); the camera basis is built around it.
    const WORLD_UP: Vector3<f32> = Vector3::new(0.0, 0.0, 1.0);

    /// The eye position in world space, derived from yaw/pitch/distance.
    #[must_use]
    pub fn eye(&self) -> Point3<f32> {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        // Spherical -> Cartesian offset from the target, +Z up.
        let offset = Vector3::new(cp * cy, cp * sy, sp) * self.distance;
        self.target + offset
    }

    /// Apply a primary-button drag of `(dx, dy)` pixels: orbit the camera.
    ///
    /// Dragging right increases yaw; dragging down lowers the pitch. Pitch is
    /// clamped so the camera never flips over the pole.
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * self.orbit_speed;
        self.pitch = (self.pitch + dy * self.orbit_speed).clamp(-MAX_PITCH, MAX_PITCH);
    }

    /// Apply a secondary/middle-button drag of `(dx, dy)` pixels: pan the
    /// target across the camera's right/up plane.
    ///
    /// Pan speed scales with `distance` so the world tracks the cursor at a
    /// roughly constant rate regardless of zoom.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        let (right, up) = self.right_up();
        let scale = self.pan_speed * self.distance;
        // Drag right -> world moves right under the cursor, so the target
        // moves left; drag down -> target moves up.
        self.target -= right * (dx * scale);
        self.target += up * (dy * scale);
    }

    /// Apply a scroll of `lines` (wheel notches; up is positive): zoom.
    ///
    /// Zoom is multiplicative so each notch covers a constant *fraction* of
    /// the current distance, which feels uniform across the whole range.
    pub fn zoom(&mut self, lines: f32) {
        let factor = (1.0 - self.zoom_speed).powf(lines);
        self.distance = (self.distance * factor).clamp(MIN_DISTANCE, MAX_DISTANCE);
    }

    /// The camera's right and up basis vectors in world space.
    ///
    /// `right` is perpendicular to both the view direction and world up;
    /// `up` completes the orthonormal frame. Used for panning.
    fn right_up(&self) -> (Vector3<f32>, Vector3<f32>) {
        let forward = (self.target - self.eye()).normalize();
        let right = forward.cross(&Self::WORLD_UP).normalize();
        let up = right.cross(&forward);
        (right, up)
    }

    /// The view matrix (world space -> camera/eye space).
    #[must_use]
    pub fn view(&self) -> Matrix4<f32> {
        Matrix4::look_at_rh(&self.eye(), &self.target, &Self::WORLD_UP)
    }

    /// The perspective projection matrix for the given viewport `aspect`
    /// (width / height).
    ///
    /// `aspect` is clamped to a small positive value so a degenerate (zero
    /// width or height) viewport cannot produce a non-finite matrix.
    #[must_use]
    pub fn projection(&self, aspect: f32) -> Matrix4<f32> {
        let aspect = aspect.max(1e-3);
        // `Perspective3` yields an OpenGL-style clip space (z in [-1, 1]);
        // wgpu wants z in [0, 1], so squash it with a correction matrix.
        let gl = Perspective3::new(aspect, self.fov_y, self.near, self.far).to_homogeneous();
        Self::WGPU_CLIP_CORRECTION * gl
    }

    /// Combined view-projection matrix uploaded to the shaders.
    #[must_use]
    pub fn view_proj(&self, aspect: f32) -> Matrix4<f32> {
        self.projection(aspect) * self.view()
    }

    /// Maps an OpenGL `[-1, 1]` clip-space depth to wgpu's `[0, 1]` range.
    ///
    /// `nalgebra`'s `Perspective3` targets OpenGL conventions; wgpu (like
    /// Direct3D / Metal) uses a half-depth clip cube. The transform
    /// `z' = z * 0.5 + w * 0.5` rescales it, applied as a left-multiply.
    #[rustfmt::skip]
    const WGPU_CLIP_CORRECTION: Matrix4<f32> = Matrix4::new(
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 0.5, 0.5,
        0.0, 0.0, 0.0, 1.0,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn eye_sits_at_the_orbit_distance() {
        let cam = OrbitCamera::default();
        let d = (cam.eye() - cam.target).norm();
        assert_relative_eq!(d, cam.distance, epsilon = 1e-5);
    }

    #[test]
    fn zoom_is_clamped_and_multiplicative() {
        let mut cam = OrbitCamera::default();
        // Many zoom-out notches: distance saturates at MAX_DISTANCE.
        for _ in 0..500 {
            cam.zoom(-1.0);
        }
        assert_relative_eq!(cam.distance, MAX_DISTANCE, epsilon = 1e-3);
        // Many zoom-in notches: distance saturates at MIN_DISTANCE.
        for _ in 0..500 {
            cam.zoom(1.0);
        }
        assert_relative_eq!(cam.distance, MIN_DISTANCE, epsilon = 1e-3);
    }

    #[test]
    fn pitch_is_clamped_to_avoid_the_pole() {
        let mut cam = OrbitCamera::default();
        for _ in 0..1000 {
            cam.orbit(0.0, 100.0);
        }
        assert!(cam.pitch <= MAX_PITCH);
        for _ in 0..1000 {
            cam.orbit(0.0, -100.0);
        }
        assert!(cam.pitch >= -MAX_PITCH);
    }

    #[test]
    fn view_proj_is_finite_for_a_normal_aspect() {
        let cam = OrbitCamera::default();
        let vp = cam.view_proj(16.0 / 9.0);
        assert!(vp.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn view_proj_is_finite_for_a_degenerate_aspect() {
        // A zero-height viewport must not yield NaNs/inf.
        let cam = OrbitCamera::default();
        let vp = cam.view_proj(0.0);
        assert!(vp.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn projection_keeps_depth_in_the_wgpu_range() {
        // A point on the near plane maps to clip z = 0; on the far plane to
        // clip z = w (NDC z = 1). Verify the clip-correction did its job.
        let cam = OrbitCamera::default();
        let proj = cam.projection(1.0);
        let near_pt = nalgebra::Vector4::new(0.0, 0.0, -cam.near, 1.0);
        let far_pt = nalgebra::Vector4::new(0.0, 0.0, -cam.far, 1.0);
        let near_clip = proj * near_pt;
        let far_clip = proj * far_pt;
        assert_relative_eq!(near_clip.z / near_clip.w, 0.0, epsilon = 1e-4);
        assert_relative_eq!(far_clip.z / far_clip.w, 1.0, epsilon = 1e-4);
    }

    #[test]
    fn pan_moves_the_target_only() {
        let mut cam = OrbitCamera::default();
        let before = cam.distance;
        cam.pan(40.0, -25.0);
        // Panning changes the target (and hence the eye) but not the orbit
        // distance or angles.
        assert_relative_eq!(cam.distance, before, epsilon = 1e-6);
        assert!(cam.target != Point3::origin());
    }
}
