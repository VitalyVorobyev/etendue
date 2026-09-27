//! Canonical render camera and remap LUT (ADR 0004).
//!
//! A renderer draws the **canonical camera**: a pinhole with square pixels,
//! no skew, no distortion, the principal point at the image centre, and a
//! field of view that covers the whole target image. The **remap LUT** then
//! gives, for every pixel `u_out` of the target camera, where to sample the
//! canonical image:
//!
//! ```text
//! ray    = target.backproject_pixel(u_out)     K⁻¹, sensor⁻¹, undistort, unproject
//! u_rend = canonical.project_point_c(ray)      the canonical pinhole
//! ```
//!
//! Both steps are `vision-calibration-core` calls, so the LUT inherits the
//! target model exactly: every distortion model, skew, principal point, and
//! Scheimpflug geometry.
//!
//! # Pixel-centre convention
//!
//! Which image coordinate names the centre of pixel `i` — `i`, or `i + ½` —
//! is decided empirically by probe P4-2 and recorded in ADR 0004. Until then
//! it is a [`PixelCentre`] argument, never a default: it fixes where the
//! image edges lie (for the canonical camera's coverage and principal point)
//! and which coordinate each LUT entry is evaluated at.

use nalgebra::Point2;
use serde::{Deserialize, Serialize};
use vision_calibration_core::{
    CameraModel, CameraParams, DistortionParams, FxFyCxCySkew, IntrinsicsParams, ProjectionParams,
    SensorParams,
};

use crate::{Error, Result};

/// The image coordinate of a pixel's centre.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PixelCentre {
    /// Pixel `i` is centred at coordinate `i`; an image of width `W` spans
    /// `[-½, W - ½]`.
    Integer,
    /// Pixel `i` is centred at coordinate `i + ½`; an image of width `W`
    /// spans `[0, W]`.
    Half,
}

impl PixelCentre {
    /// Coordinate of the centre of pixel 0.
    #[must_use]
    pub fn offset(self) -> f64 {
        match self {
            Self::Integer => 0.0,
            Self::Half => 0.5,
        }
    }

    /// Coordinate of the leading edge of pixel 0 (the image's edge).
    #[must_use]
    pub fn edge(self) -> f64 {
        self.offset() - 0.5
    }
}

/// How to choose the canonical camera for a target camera.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalSpec {
    /// Supersampling: canonical pixels per target pixel at the image centre,
    /// `≥ 1`. The canonical focal length is `supersample · max(fx, fy)`.
    pub supersample: f64,
    /// Extra field of view beyond the target image, as a fraction of each
    /// half-extent (`0.02` = 2 %), `≥ 0`.
    pub margin: f64,
    /// Spacing in target pixels of the grid scanned to find the image's
    /// extent in viewing directions, `≥ 1`. The image border is always
    /// sampled at this spacing, and the interior too, so a model whose
    /// extreme ray is not on the border is still covered.
    pub scan_step_px: u32,
}

impl Default for CanonicalSpec {
    fn default() -> Self {
        Self {
            supersample: 1.0,
            margin: 0.02,
            scan_step_px: 8,
        }
    }
}

/// The canonical render camera for one target camera.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalCamera {
    /// The canonical pinhole as calibration-rs parameters (`Pinhole`, no
    /// distortion, `Identity` sensor, `fx = fy`, `skew = 0`, principal point
    /// at the image centre under [`CanonicalCamera::pixel_centre`]).
    pub params: CameraParams,
    /// Image size `[width, height]` in canonical pixels.
    pub resolution: [u32; 2],
    /// The pixel-centre convention the principal point was placed with.
    pub pixel_centre: PixelCentre,
}

impl CanonicalCamera {
    /// Choose the canonical camera covering `target` (with image size
    /// `resolution`) under `spec`.
    ///
    /// The target image is scanned on a grid of spacing `spec.scan_step_px`
    /// (border included); each pixel is back-projected through the target
    /// model, and the canonical field of view is the symmetric one that
    /// contains every ray, widened by `spec.margin`.
    ///
    /// # Errors
    ///
    /// [`Error::Camera`] if the target model does not build;
    /// [`Error::InvalidInput`] for a zero resolution, `supersample < 1`,
    /// a negative margin, a zero scan step, or a target whose rays do not all
    /// point forward.
    pub fn cover(
        target: &CameraParams,
        resolution: [u32; 2],
        spec: &CanonicalSpec,
        pixel_centre: PixelCentre,
    ) -> Result<Self> {
        let [w, h] = resolution;
        if w == 0 || h == 0 {
            return Err(Error::InvalidInput(format!(
                "resolution must be non-zero, got {resolution:?}"
            )));
        }
        if !(spec.supersample.is_finite() && spec.supersample >= 1.0) {
            return Err(Error::InvalidInput(format!(
                "supersample must be ≥ 1, got {}",
                spec.supersample
            )));
        }
        if !(spec.margin.is_finite() && spec.margin >= 0.0) {
            return Err(Error::InvalidInput(format!(
                "margin must be ≥ 0, got {}",
                spec.margin
            )));
        }
        if spec.scan_step_px == 0 {
            return Err(Error::InvalidInput("scan step must be ≥ 1".into()));
        }
        let model = target.build()?;
        let IntrinsicsParams::FxFyCxCySkew { params: k } = target.intrinsics;

        // Extreme viewing directions over the image, on the z = 1 plane.
        let edge = pixel_centre.edge();
        let axis = |n: u32| -> Vec<f64> {
            let step = spec.scan_step_px as usize;
            let mut v: Vec<f64> = (0..=n as usize).step_by(step).map(|i| i as f64).collect();
            if v.last() != Some(&f64::from(n)) {
                v.push(f64::from(n));
            }
            v.into_iter().map(|t| edge + t).collect()
        };
        let (xs, ys) = (axis(w), axis(h));
        let (mut x_max, mut y_max) = (0.0_f64, 0.0_f64);
        for &v in &ys {
            for &u in &xs {
                let ray = model.backproject_pixel(&Point2::new(u, v)).point;
                if !(ray.x.is_finite() && ray.y.is_finite()) {
                    return Err(Error::InvalidInput(format!(
                        "pixel ({u}, {v}) does not back-project to a forward ray"
                    )));
                }
                x_max = x_max.max(ray.x.abs());
                y_max = y_max.max(ray.y.abs());
            }
        }

        let f = spec.supersample * k.fx.abs().max(k.fy.abs());
        let widen = 1.0 + spec.margin;
        // Even sizes keep the principal point on a pixel boundary under
        // either convention, so the canonical image is exactly symmetric.
        let size = |extent: f64| -> u32 {
            let half = (extent * widen * f).ceil() as u32;
            2 * half.max(1)
        };
        let (cw, ch) = (size(x_max), size(y_max));
        let centre = |n: u32| edge + f64::from(n) / 2.0;
        let params = CameraParams {
            projection: ProjectionParams::Pinhole,
            distortion: DistortionParams::None,
            sensor: SensorParams::Identity,
            intrinsics: IntrinsicsParams::FxFyCxCySkew {
                params: FxFyCxCySkew {
                    fx: f,
                    fy: f,
                    cx: centre(cw),
                    cy: centre(ch),
                    skew: 0.0,
                },
            },
        };
        Ok(Self {
            params,
            resolution: [cw, ch],
            pixel_centre,
        })
    }

    /// Focal length in canonical pixels (`fx = fy`).
    #[must_use]
    pub fn focal_px(&self) -> f64 {
        let IntrinsicsParams::FxFyCxCySkew { params } = self.params.intrinsics;
        params.fx
    }

    /// Full horizontal field of view in radians.
    #[must_use]
    pub fn horizontal_fov(&self) -> f64 {
        2.0 * (f64::from(self.resolution[0]) / 2.0 / self.focal_px()).atan()
    }

    /// Full vertical field of view in radians.
    #[must_use]
    pub fn vertical_fov(&self) -> f64 {
        2.0 * (f64::from(self.resolution[1]) / 2.0 / self.focal_px()).atan()
    }

    /// The calibration-rs model of the canonical camera.
    ///
    /// # Errors
    ///
    /// Never for a camera built by [`CanonicalCamera::cover`] (identity
    /// sensor); [`Error::Camera`] for hand-edited parameters.
    pub fn model(&self) -> Result<CameraModel> {
        Ok(self.params.build()?)
    }
}

/// A remap LUT: for each target pixel, where to sample the canonical image.
#[derive(Clone, Debug, PartialEq)]
pub struct RemapLut {
    /// Target image width in pixels.
    pub width: u32,
    /// Target image height in pixels.
    pub height: u32,
    /// The convention the LUT was evaluated with (target pixel `(i, j)` is
    /// the coordinate `(i, j) + offset`; entries are canonical coordinates in
    /// the same convention).
    pub pixel_centre: PixelCentre,
    /// Row-major, two `f32` per target pixel: the canonical-image coordinate
    /// `(u, v)` to sample, `NaN` where the canonical camera cannot image the
    /// ray. `2 · width · height` values — the RG32F texture layout.
    pub data: Vec<f32>,
}

impl RemapLut {
    /// The entry of target pixel `(i, j)`, or `None` outside the image.
    #[must_use]
    pub fn get(&self, i: u32, j: u32) -> Option<[f32; 2]> {
        if i >= self.width || j >= self.height {
            return None;
        }
        let k = 2 * (j as usize * self.width as usize + i as usize);
        Some([self.data[k], self.data[k + 1]])
    }
}

/// Tabulate the remap from the target camera to `canonical` (ADR 0004).
///
/// `resolution` is the target image size; the LUT is evaluated at every
/// target pixel centre under `canonical.pixel_centre`.
///
/// # Errors
///
/// [`Error::Camera`] if either model does not build;
/// [`Error::InvalidInput`] for a zero resolution.
pub fn remap_lut(
    target: &CameraParams,
    resolution: [u32; 2],
    canonical: &CanonicalCamera,
) -> Result<RemapLut> {
    let [w, h] = resolution;
    if w == 0 || h == 0 {
        return Err(Error::InvalidInput(format!(
            "resolution must be non-zero, got {resolution:?}"
        )));
    }
    let target = target.build()?;
    let render = canonical.model()?;
    let off = canonical.pixel_centre.offset();
    let mut data = Vec::with_capacity(2 * w as usize * h as usize);
    for j in 0..h {
        for i in 0..w {
            let ray = target
                .backproject_pixel(&Point2::new(f64::from(i) + off, f64::from(j) + off))
                .point;
            match render.project_point_c(&ray) {
                Some(p) => data.extend_from_slice(&[p.x as f32, p.y as f32]),
                None => data.extend_from_slice(&[f32::NAN, f32::NAN]),
            }
        }
    }
    Ok(RemapLut {
        width: w,
        height: h,
        pixel_centre: canonical.pixel_centre,
        data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use vision_calibration_core::BrownConrady5;

    fn pinhole(fx: f64, fy: f64, cx: f64, cy: f64) -> CameraParams {
        CameraParams {
            projection: ProjectionParams::Pinhole,
            distortion: DistortionParams::None,
            sensor: SensorParams::Identity,
            intrinsics: IntrinsicsParams::FxFyCxCySkew {
                params: FxFyCxCySkew {
                    fx,
                    fy,
                    cx,
                    cy,
                    skew: 0.0,
                },
            },
        }
    }

    #[test]
    fn pixel_centre_conventions() {
        assert_eq!(PixelCentre::Integer.offset(), 0.0);
        assert_eq!(PixelCentre::Integer.edge(), -0.5);
        assert_eq!(PixelCentre::Half.offset(), 0.5);
        assert_eq!(PixelCentre::Half.edge(), 0.0);
    }

    #[test]
    fn a_centred_pinhole_is_its_own_canonical_camera() {
        // Centred principal point, square pixels, no margin, s = 1: the
        // canonical camera is the target itself, and the LUT is the identity.
        for centre in [PixelCentre::Integer, PixelCentre::Half] {
            let c = centre.edge() + 32.0;
            let target = pinhole(100.0, 100.0, c, centre.edge() + 24.0);
            let spec = CanonicalSpec {
                margin: 0.0,
                scan_step_px: 1,
                ..CanonicalSpec::default()
            };
            let cam = CanonicalCamera::cover(&target, [64, 48], &spec, centre).unwrap();
            assert_eq!(cam.resolution, [64, 48]);
            assert!((cam.focal_px() - 100.0).abs() < 1e-12);
            let lut = remap_lut(&target, [64, 48], &cam).unwrap();
            for (i, j) in [(0, 0), (63, 47), (10, 30)] {
                let [u, v] = lut.get(i, j).unwrap();
                let want = [
                    f64::from(i) + centre.offset(),
                    f64::from(j) + centre.offset(),
                ];
                assert!((f64::from(u) - want[0]).abs() < 1e-4, "{u} vs {want:?}");
                assert!((f64::from(v) - want[1]).abs() < 1e-4, "{v} vs {want:?}");
            }
            assert_eq!(lut.get(64, 0), None);
        }
    }

    #[test]
    fn an_off_centre_principal_point_widens_the_canonical_camera() {
        // cx at a quarter of the width: the far side needs 3/4 of the width as
        // half-extent, so the symmetric canonical image is 1.5× wider.
        let target = pinhole(100.0, 100.0, 15.5, 23.5);
        let spec = CanonicalSpec {
            margin: 0.0,
            scan_step_px: 1,
            ..CanonicalSpec::default()
        };
        let cam = CanonicalCamera::cover(&target, [64, 48], &spec, PixelCentre::Integer).unwrap();
        assert_eq!(cam.resolution, [96, 48]);
        let lut = remap_lut(&target, [64, 48], &cam).unwrap();
        // Every LUT entry lands inside the canonical image.
        for pair in lut.data.chunks_exact(2) {
            assert!(pair[0] >= -0.5 && pair[0] <= 95.5, "{pair:?}");
            assert!(pair[1] >= -0.5 && pair[1] <= 47.5, "{pair:?}");
        }
    }

    #[test]
    fn supersampling_margin_and_fov() {
        let target = pinhole(100.0, 80.0, 31.5, 23.5);
        let spec = CanonicalSpec {
            supersample: 4.0,
            margin: 0.1,
            scan_step_px: 4,
        };
        let cam = CanonicalCamera::cover(&target, [64, 48], &spec, PixelCentre::Integer).unwrap();
        // f = 4 · max(fx, fy); half-extents widened by 10 %.
        assert!((cam.focal_px() - 400.0).abs() < 1e-12);
        assert_eq!(cam.resolution, [2 * 141, 2 * 132]);
        assert!(cam.horizontal_fov() > 2.0 * (32.0f64 / 100.0).atan());
        assert!(cam.vertical_fov() > 2.0 * (24.0f64 / 80.0).atan());
        let json = serde_json::to_string(&cam).unwrap();
        let back: CanonicalCamera = serde_json::from_str(&json).unwrap();
        assert_eq!(back.resolution, cam.resolution);
    }

    #[test]
    fn barrel_distortion_needs_a_wider_canonical_view() {
        let mut target = pinhole(500.0, 500.0, 319.5, 239.5);
        let flat = CanonicalCamera::cover(
            &target,
            [640, 480],
            &CanonicalSpec::default(),
            PixelCentre::Integer,
        )
        .unwrap();
        target.distortion = DistortionParams::BrownConrady5 {
            params: BrownConrady5 {
                k1: -0.3,
                k2: 0.1,
                k3: 0.0,
                p1: 0.0,
                p2: 0.0,
                iters: 8,
            },
        };
        let barrel = CanonicalCamera::cover(
            &target,
            [640, 480],
            &CanonicalSpec::default(),
            PixelCentre::Integer,
        )
        .unwrap();
        assert!(barrel.resolution[0] > flat.resolution[0]);
        assert!(barrel.resolution[1] > flat.resolution[1]);
    }

    #[test]
    fn rejects_bad_specs() {
        let target = pinhole(100.0, 100.0, 31.5, 23.5);
        let bad = [
            CanonicalSpec {
                supersample: 0.5,
                ..CanonicalSpec::default()
            },
            CanonicalSpec {
                margin: -0.1,
                ..CanonicalSpec::default()
            },
            CanonicalSpec {
                scan_step_px: 0,
                ..CanonicalSpec::default()
            },
        ];
        for spec in bad {
            assert!(matches!(
                CanonicalCamera::cover(&target, [64, 48], &spec, PixelCentre::Integer),
                Err(Error::InvalidInput(_))
            ));
        }
        assert!(matches!(
            CanonicalCamera::cover(
                &target,
                [0, 48],
                &CanonicalSpec::default(),
                PixelCentre::Integer
            ),
            Err(Error::InvalidInput(_))
        ));
        let cam = CanonicalCamera::cover(
            &target,
            [64, 48],
            &CanonicalSpec::default(),
            PixelCentre::Integer,
        )
        .unwrap();
        assert!(matches!(
            remap_lut(&target, [64, 0], &cam),
            Err(Error::InvalidInput(_))
        ));
    }
}
