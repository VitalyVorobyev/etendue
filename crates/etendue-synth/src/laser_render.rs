//! Laser images (P7-7, [ADR 0005](https://github.com/VitalyVorobyev/etendue/blob/main/docs/adrs/0005-blender-renderer.md)):
//! a line laser's stripe composited onto a rendered capture, in Rust.
//!
//! Cycles is not asked to render the laser. A sub-millimetre beam is a light
//! that a path tracer samples noisily. Instead, each canonical pixel of a
//! camera's render is turned into the surface it sees:
//! - the **Depth** pass (planar depth, measured: ADR 0005) and the canonical
//!   pinhole give the world point;
//! - the **Normal** and **Diffuse Color** passes give the surface's normal
//!   and albedo.
//!
//! A laser adds Lambertian radiance `L = ρ/π · E` with
//!
//! ```text
//! E = P/(2α) · 1/r · exp(−2d²/w(r)²) / (w(r)·√(π/2)) · max(0, n̂·l̂)
//! ```
//!
//! - The power `P` is spread evenly over the fan angle `2α`.
//! - `r` is the point's distance from the apex within the fan plane.
//! - `d` is its distance from the fan plane.
//! - `w(r)` is the Gaussian beam's `1/e²` radius ([`GaussianBeamWidth`]).
//! - `l̂` points to the laser.
//!
//! Only points inside the fan's angle and reach are lit. Integrated over a
//! surface the fan crosses, `E` gives back `P`.
//!
//! **Shadows:** a point is lit only if a depth view from the laser's apex
//! sees it: [`LaserView`]s, which Blender renders as depth only ([`laser_views`]).
//!
//! **Not modelled:**
//! - defocus: the scene's cameras have no aperture, and the renders are
//!   pinhole;
//! - speckle;
//! - the beam's specular reflection;
//! - interreflection.
//!
//! The laser counts the same in every colour channel: the camera's spectral
//! response at the wavelength is folded into the power (`LaserSpec::power_w`).

use std::f64::consts::{FRAC_PI_2, PI};

use etendue_core::LaserEntity;
use etendue_core::laser::{GaussianBeamWidth, LaserPlane, WidthModel};
use etendue_scene::LaserSpec;
use nalgebra::{Isometry3, Point2, Point3, Translation3, UnitQuaternion, Vector3};
use vision_calibration_core::CameraModel;

use crate::images::{LinearImage, RenderPasses};
use crate::job::LaserView;
use crate::remap::{CanonicalCamera, PixelCentre};
use crate::{Error, Result};

/// Laser power when the scene gives none, watts.
pub const DEFAULT_POWER_W: f64 = 5e-4;

/// Fan angle one laser view covers at most; a wider fan gets more views.
const VIEW_SPAN_RAD: f64 = 60.0 * PI / 180.0;
/// Extra angle each view covers beyond its share of the fan, each side.
const VIEW_MARGIN_RAD: f64 = 2.0 * PI / 180.0;
/// Half-height of a view, out of the fan plane: the beam's light is within
/// a few `w` of the plane, far less than this.
const VIEW_ELEVATION_RAD: f64 = 1.0 * PI / 180.0;
/// Width of a view in pixels.
const VIEW_WIDTH_PX: u32 = 2048;
/// Depths beyond this are background (Cycles writes about 1e10).
const BACKGROUND_DEPTH_M: f32 = 1e6;
/// A point is in shadow when it is farther from the laser than the surface
/// its view sees, by more than this plus [`SHADOW_RELATIVE`] of its depth.
const SHADOW_ABSOLUTE_M: f64 = 2e-3;
/// See [`SHADOW_ABSOLUTE_M`].
const SHADOW_RELATIVE: f64 = 0.01;
/// The beam is cut off this many `1/e²` radii from the fan plane
/// (`exp(−2·4²) ≈ 1e-14`).
const CUTOFF_RADII: f64 = 4.0;

/// The depth views from `laser`'s apex that its shadows are taken from:
/// pinholes looking along the fan, each covering at most 60° of it, the
/// image's +X along the fan. Ids are `<laser>/view<k>`.
#[must_use]
pub fn laser_views(laser: &LaserSpec, pixel_centre: PixelCentre) -> Vec<LaserView> {
    let alpha = laser.fan_half_angle;
    let count = (2.0 * alpha / VIEW_SPAN_RAD).ceil().max(1.0) as usize;
    let span = 2.0 * alpha / count as f64;
    let focal = f64::from(VIEW_WIDTH_PX) / 2.0 / (span / 2.0 + VIEW_MARGIN_RAD).tan();
    let height = 2 * ((focal * VIEW_ELEVATION_RAD.tan()).ceil() as u32).max(1);
    // The view's CV axes in the laser frame: +X the fan's in-plane axis (laser
    // +Y), +Y the laser's −X, +Z the central ray (laser +Z); then turned
    // about the laser's +X to the view's share of the fan.
    let base = UnitQuaternion::from_axis_angle(&Vector3::z_axis(), FRAC_PI_2);
    (0..count)
        .map(|k| {
            let phi = -alpha + (k as f64 + 0.5) * span;
            LaserView {
                id: format!("{}/view{k}", laser.id),
                laser: laser.id.clone(),
                camera: CanonicalCamera::pinhole(focal, [VIEW_WIDTH_PX, height], pixel_centre),
                laser_se3_view: Isometry3::from_parts(
                    Translation3::identity(),
                    UnitQuaternion::from_axis_angle(&Vector3::x_axis(), -phi) * base,
                ),
            }
        })
        .collect()
}

/// A rendered laser view at one capture: where the laser's light stops.
pub struct ShadowView {
    model: CameraModel,
    view_se3_world: Isometry3<f64>,
    width: u32,
    height: u32,
    offset: f64,
    depth: Vec<f32>,
}

impl ShadowView {
    /// `view` rendered with the laser at `world_se3_laser`, from its depth
    /// pass.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] if the render is not the view's size.
    pub fn new(
        view: &LaserView,
        world_se3_laser: &Isometry3<f64>,
        passes: &RenderPasses,
    ) -> Result<Self> {
        let [width, height] = view.camera.resolution;
        if [passes.width, passes.height] != view.camera.resolution {
            return Err(Error::InvalidInput(format!(
                "laser view `{}`: render is {}×{}, not {width}×{height}",
                view.id, passes.width, passes.height
            )));
        }
        Ok(Self {
            model: view.camera.model()?,
            view_se3_world: (world_se3_laser * view.laser_se3_view).inverse(),
            width,
            height,
            offset: view.camera.pixel_centre.offset(),
            depth: passes.depth.clone(),
        })
    }

    /// Whether the laser's light reaches `p`; `None` if this view does not
    /// cover it.
    fn lit(&self, p: &Point3<f64>) -> Option<bool> {
        let q = self.view_se3_world * p;
        if q.z <= 0.0 {
            return None;
        }
        let px = self.model.project_point(&q)?;
        let (i, j) = ((px.x - self.offset).round(), (px.y - self.offset).round());
        if !(i >= 0.0 && j >= 0.0 && i < f64::from(self.width) && j < f64::from(self.height)) {
            return None;
        }
        let surface = f64::from(self.depth[j as usize * self.width as usize + i as usize]);
        Some(q.z <= surface + SHADOW_ABSOLUTE_M + SHADOW_RELATIVE * q.z)
    }
}

/// A line laser at one capture: its fan, beam and shadows.
pub struct Beam {
    fan: LaserPlane,
    width: GaussianBeamWidth,
    power_w: f64,
    shadows: Vec<ShadowView>,
}

impl Beam {
    /// `laser` at `world_se3_laser`, its light stopped where `shadows` show
    /// a nearer surface (none: no shadows).
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] if the laser's parameters are out of range.
    pub fn new(
        laser: &LaserSpec,
        world_se3_laser: &Isometry3<f64>,
        shadows: Vec<ShadowView>,
    ) -> Result<Self> {
        let core =
            |e: etendue_core::Error| Error::InvalidInput(format!("laser `{}`: {e}", laser.id));
        let entity = LaserEntity::new(
            *world_se3_laser,
            laser.fan_half_angle,
            laser.fan_length,
            laser.wavelength_nm,
            laser.beam_waist_m,
        )
        .map_err(core)?;
        let power_w = laser.power_w.unwrap_or(DEFAULT_POWER_W);
        if !(power_w.is_finite() && power_w > 0.0) {
            return Err(Error::InvalidInput(format!(
                "laser `{}`: power must be positive, got {power_w}",
                laser.id
            )));
        }
        Ok(Self {
            fan: LaserPlane::from_entity(&entity),
            width: GaussianBeamWidth::from_laser(&entity).map_err(core)?,
            power_w,
            shadows,
        })
    }

    /// The irradiance (W/m²) the beam puts on a surface at `p` whose unit
    /// normal `n` faces the side the light must arrive from.
    #[must_use]
    pub fn irradiance(&self, p: &Point3<f64>, n: &Vector3<f64>) -> f64 {
        let v = p - self.fan.origin();
        let distance = v.norm();
        let w = self.width.width_at(distance) / 2.0;
        let d = self.fan.signed_distance(p);
        if d.abs() > CUTOFF_RADII * w || !self.fan.contains_in_extent(p) {
            return 0.0;
        }
        let r = self.fan.radius_of(p);
        let cos = -n.dot(&v) / distance;
        if !(r > 0.0 && cos > 0.0) {
            return 0.0;
        }
        if self.shadows.iter().find_map(|s| s.lit(p)) == Some(false) {
            return 0.0;
        }
        let line = self.power_w / (2.0 * self.fan.half_angle()) / r;
        line * (-2.0 * d * d / (w * w)).exp() / (w * FRAC_PI_2.sqrt()) * cos
    }
}

/// The laser radiance a camera sees at every pixel of its canonical render:
/// each pixel's surface (from `passes`) lit by `beams`. `world_se3_camera`
/// is the camera's pose at the capture.
///
/// # Errors
///
/// [`Error::InvalidInput`] if the render lacks the Normal or Diffuse Color
/// pass (a job without laser passes) or is not the canonical camera's size.
pub fn laser_radiance(
    canonical: &CanonicalCamera,
    world_se3_camera: &Isometry3<f64>,
    passes: &RenderPasses,
    beams: &[&Beam],
) -> Result<LinearImage> {
    let (Some(normal), Some(albedo)) = (&passes.normal, &passes.albedo) else {
        return Err(Error::InvalidInput(
            "the render has no Normal and Diffuse Color passes (render job without laser passes)"
                .into(),
        ));
    };
    let [w, h] = canonical.resolution;
    if [passes.width, passes.height] != canonical.resolution {
        return Err(Error::InvalidInput(format!(
            "render is {}×{}, the canonical camera {w}×{h}",
            passes.width, passes.height
        )));
    }
    let model = canonical.model()?;
    let offset = canonical.pixel_centre.offset();
    let eye = Point3::from(world_se3_camera.translation.vector);
    let mut out = LinearImage::black(w, h);
    for j in 0..h {
        for i in 0..w {
            let k = (j * w + i) as usize;
            let z = passes.depth[k];
            if !(z.is_finite() && z > 0.0 && z < BACKGROUND_DEPTH_M) {
                continue;
            }
            let ray = model
                .backproject_pixel(&Point2::new(f64::from(i) + offset, f64::from(j) + offset))
                .point;
            let p = world_se3_camera * Point3::from(ray * f64::from(z) / ray.z);
            let Some(mut n) = Vector3::new(normal[3 * k], normal[3 * k + 1], normal[3 * k + 2])
                .cast::<f64>()
                .try_normalize(1e-6)
            else {
                continue;
            };
            // The side the camera sees is the side that must be lit.
            if n.dot(&(eye - p)) < 0.0 {
                n = -n;
            }
            let e: f64 = beams.iter().map(|b| b.irradiance(&p, &n)).sum();
            if e > 0.0 {
                for c in 0..3 {
                    out.rgb[3 * k + c] = (f64::from(albedo[3 * k + c]) * e / PI) as f32;
                }
            }
        }
    }
    Ok(out)
}

/// A laser image: the capture's render dimmed to `ambient` (the light left
/// on while the laser is) plus the laser radiance.
///
/// # Panics
///
/// If the two images differ in size.
#[must_use]
pub fn laser_image(render: &LinearImage, laser: &LinearImage, ambient: f32) -> LinearImage {
    assert_eq!(
        (render.width, render.height),
        (laser.width, laser.height),
        "laser image size"
    );
    LinearImage {
        width: render.width,
        height: render.height,
        rgb: render
            .rgb
            .iter()
            .zip(&laser.rgb)
            .map(|(r, l)| ambient * r + l)
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn laser(fan_half_angle: f64) -> LaserSpec {
        serde_json::from_value(serde_json::json!({
            "id": "laser", "parent": "world",
            "parent_se3_self": {"rotation": [0.0, 0.0, 0.0, 1.0], "translation": [0.0, 0.0, 0.0]},
            "fan_half_angle": fan_half_angle, "fan_length": 2.0,
            "wavelength_nm": 660.0, "beam_waist_m": 5e-4, "power_w": 1e-3
        }))
        .unwrap()
    }

    #[test]
    fn views_cover_the_fan() {
        for alpha in [0.2, 0.6, 1.2] {
            let l = laser(alpha);
            let views = laser_views(&l, PixelCentre::Integer);
            assert_eq!(views.len(), (2.0 * alpha / VIEW_SPAN_RAD).ceil() as usize);
            // Every ray of the fan, at 0.5 m, falls inside some view.
            for t in -20..=20 {
                let phi = alpha * f64::from(t) / 20.0;
                let p = Point3::new(0.0, phi.sin(), phi.cos()) * 0.5;
                let seen = views.iter().any(|v| {
                    let q = v.laser_se3_view.inverse() * p;
                    let px = v.camera.model().unwrap().project_point(&q);
                    px.is_some_and(|px| {
                        px.x >= 0.0
                            && px.y >= 0.0
                            && px.x < f64::from(v.camera.resolution[0])
                            && px.y < f64::from(v.camera.resolution[1])
                    })
                });
                assert!(seen, "α {alpha}, φ {phi}");
            }
        }
    }

    /// Over a plane square to the central ray, the irradiance integrates to
    /// the laser's power.
    #[test]
    fn irradiance_integrates_to_the_power() {
        let l = laser(0.35);
        let beam = Beam::new(&l, &Isometry3::identity(), vec![]).unwrap();
        let (z, n) = (0.5, -Vector3::z());
        let (dx, dy) = (2e-5, 2e-4);
        let mut total = 0.0;
        let y_max = z * l.fan_half_angle.tan() + 1e-3;
        let mut y = -y_max;
        while y < y_max {
            let mut x = -3e-3;
            while x < 3e-3 {
                total += beam.irradiance(&Point3::new(x, y, z), &n) * dx * dy;
                x += dx;
            }
            y += dy;
        }
        assert!((total / 1e-3 - 1.0).abs() < 2e-3, "∫E dA = {total} W");
    }

    #[test]
    fn a_nearer_surface_casts_a_shadow() {
        let l = laser(0.35);
        let view = &laser_views(&l, PixelCentre::Integer)[0];
        let [w, h] = view.camera.resolution;
        let passes = |depth: f32| RenderPasses {
            width: w,
            height: h,
            depth: vec![depth; (w * h) as usize],
            normal: None,
            albedo: None,
        };
        let p = Point3::new(0.0, 0.05, 0.5);
        let n = -Vector3::z();
        let lit = |depth| {
            let shadow = ShadowView::new(view, &Isometry3::identity(), &passes(depth)).unwrap();
            Beam::new(&l, &Isometry3::identity(), vec![shadow])
                .unwrap()
                .irradiance(&p, &n)
        };
        assert!(lit(0.5) > 0.0, "the surface itself");
        assert_eq!(lit(0.3), 0.0, "something nearer");
        assert!(lit(f32::MAX) > 0.0, "nothing in the way");
    }

    /// A camera above a plane, the laser fan across it: in every image row
    /// the stripe's intensity-weighted centre is where the fan meets the
    /// plane.
    #[test]
    fn the_stripe_is_where_the_fan_meets_the_surface() {
        let canonical = CanonicalCamera::pinhole(800.0, [400, 300], PixelCentre::Integer);
        let model = canonical.model().unwrap();
        // Camera at 0.5 m looking down −Z; the plane z = 0, normal +Z.
        let world_se3_camera = Isometry3::from_parts(
            Translation3::new(0.0, 0.0, 0.5),
            UnitQuaternion::from_axis_angle(&Vector3::x_axis(), PI),
        );
        // The laser 0.1 m to the side, its central ray turned β from straight
        // down, its fan along y: it meets z = 0 on the line x = 0.1 − 0.5·tan β.
        let beta: f64 = 0.2;
        let world_se3_laser = Isometry3::from_parts(
            Translation3::new(0.1, 0.0, 0.5),
            UnitQuaternion::from_axis_angle(&Vector3::y_axis(), PI + beta),
        );
        let (w, h) = (400usize, 300usize);
        // The camera looks straight down: every pixel sees z = 0 at depth 0.5.
        let passes = RenderPasses {
            width: 400,
            height: 300,
            depth: vec![0.5; w * h],
            normal: Some([0.0, 0.0, 1.0].repeat(w * h)),
            albedo: Some([0.8, 0.8, 0.8].repeat(w * h)),
        };
        // A 2 mm waist: a stripe several pixels wide, so sampling it at pixel
        // centres does not bias its centroid.
        let mut wide = laser(0.6);
        wide.beam_waist_m = 2e-3;
        let beam = Beam::new(&wide, &world_se3_laser, vec![]).unwrap();
        let image = laser_radiance(&canonical, &world_se3_camera, &passes, &[&beam]).unwrap();
        // Where the fan meets z = 0: x = 0.1 − 0.5·tan β, for every y.
        let x0 = 0.1 - 0.5 * beta.tan();
        let camera_se3_world = world_se3_camera.inverse();
        let mut worst = 0.0_f64;
        for j in 20..h - 20 {
            let row = &image.rgb[3 * j * w..3 * (j + 1) * w];
            let (mut s, mut si) = (0.0, 0.0);
            for i in 0..w {
                let v = f64::from(row[3 * i]);
                s += v;
                si += v * i as f64;
            }
            assert!(s > 0.0, "row {j} has stripe");
            let u = si / s;
            // The true stripe at this row: project points of the line, find its column.
            let y = {
                let ray = model.backproject_pixel(&Point2::new(u, j as f64)).point;
                let p = world_se3_camera * Point3::from(ray * 0.5 / ray.z);
                p.y
            };
            let truth = model
                .project_point(&(camera_se3_world * Point3::new(x0, y, 0.0)))
                .unwrap();
            worst = worst.max((u - truth.x).abs());
        }
        assert!(worst < 0.02, "stripe centre off by {worst} px");
    }
}
