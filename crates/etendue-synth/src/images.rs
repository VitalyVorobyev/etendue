//! Images of the native render path (feature `images`): ingest of the
//! Blender backend's multilayer EXR, resampling onto the calibrated camera
//! through the remap LUT, and PNG output (ADR 0004, ADR 0005).
//!
//! Sampling follows the LUT's [`PixelCentre`]: canonical pixel `x` (column
//! index) sits at canonical coordinate `x + offset`. The canonical image is
//! the renderer's raster, top row first.

use std::io::BufWriter;
use std::path::Path;

use crate::remap::{PixelCentre, RemapLut};
use crate::{Error, Result};

/// A linear-radiance RGB image, row-major, top row first.
#[derive(Clone, Debug, PartialEq)]
pub struct LinearImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `3 · width · height` values, `[r, g, b]` per pixel.
    pub rgb: Vec<f32>,
}

impl LinearImage {
    /// A black image.
    #[must_use]
    pub fn black(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            rgb: vec![0.0; 3 * width as usize * height as usize],
        }
    }

    fn px(&self, x: i64, y: i64) -> [f32; 3] {
        let x = x.clamp(0, i64::from(self.width) - 1) as usize;
        let y = y.clamp(0, i64::from(self.height) - 1) as usize;
        let k = 3 * (y * self.width as usize + x);
        [self.rgb[k], self.rgb[k + 1], self.rgb[k + 2]]
    }

    /// Bilinear sample at pixel-index coordinates (`(0, 0)` = centre of the
    /// top-left pixel), clamped to the edge.
    #[must_use]
    pub fn bilinear(&self, x: f64, y: f64) -> [f32; 3] {
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = ((x - x0) as f32, (y - y0) as f32);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let [a, b, c, d] = [
            self.px(x0, y0),
            self.px(x0 + 1, y0),
            self.px(x0, y0 + 1),
            self.px(x0 + 1, y0 + 1),
        ];
        std::array::from_fn(|i| {
            let top = a[i] + (b[i] - a[i]) * fx;
            let bottom = c[i] + (d[i] - c[i]) * fx;
            top + (bottom - top) * fy
        })
    }
}

/// Read the `Combined` RGB pass of a Blender multilayer EXR (linear radiance).
///
/// # Errors
///
/// [`Error::InvalidInput`] if the file does not read or has no
/// `Combined.{R,G,B}` channels.
pub fn read_exr_combined(path: &Path) -> Result<LinearImage> {
    use exr::prelude::{AnyChannel, FlatSamples, ReadChannels, ReadLayers, read};
    let bad = |m: String| crate::Error::InvalidInput(format!("{}: {m}", path.display()));
    let image = read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .all_layers()
        .all_attributes()
        .from_file(path)
        .map_err(|e| bad(e.to_string()))?;
    for layer in &image.layer_data {
        let find = |c: &str| {
            layer.channel_data.list.iter().find(|ch| {
                let name = ch.name.to_string();
                name == format!("Combined.{c}") || name.ends_with(&format!(".Combined.{c}"))
            })
        };
        let (Some(r), Some(g), Some(b)) = (find("R"), find("G"), find("B")) else {
            continue;
        };
        let (w, h) = (layer.size.width(), layer.size.height());
        let values =
            |ch: &AnyChannel<FlatSamples>| -> Vec<f32> { ch.sample_data.values_as_f32().collect() };
        let (r, g, b) = (values(r), values(g), values(b));
        let mut rgb = Vec::with_capacity(3 * w * h);
        for i in 0..w * h {
            rgb.extend_from_slice(&[r[i], g[i], b[i]]);
        }
        return Ok(LinearImage {
            width: w as u32,
            height: h as u32,
            rgb,
        });
    }
    Err(bad("no Combined.R/G/B channels".into()))
}

/// Resample a canonical render onto the calibrated camera: output pixel
/// `(i, j)` samples `canonical` bilinearly at the LUT entry. Pixels without
/// an entry (`NaN`) are black.
#[must_use]
pub fn remap_image(canonical: &LinearImage, lut: &RemapLut) -> LinearImage {
    let off = match lut.pixel_centre {
        PixelCentre::Integer => 0.0,
        PixelCentre::Half => 0.5,
    };
    let mut out = LinearImage::black(lut.width, lut.height);
    for (k, uv) in lut.data.chunks_exact(2).enumerate() {
        let (u, v) = (f64::from(uv[0]), f64::from(uv[1]));
        if u.is_finite() && v.is_finite() {
            out.rgb[3 * k..3 * k + 3].copy_from_slice(&canonical.bilinear(u - off, v - off));
        }
    }
    out
}

/// Resample like [`remap_image`], but average `taps × taps` bilinear samples
/// over each output pixel's footprint in the canonical image — a box filter,
/// so a supersampled canonical render is integrated over the pixel instead of
/// point-sampled (which aliases). The footprint is the parallelogram spanned
/// by the LUT's own finite differences (central inside, one-sided at the
/// borders); no camera model is evaluated. `taps = 1` is [`remap_image`].
#[must_use]
pub fn remap_image_box(canonical: &LinearImage, lut: &RemapLut, taps: u32) -> LinearImage {
    if taps <= 1 {
        return remap_image(canonical, lut);
    }
    let off = match lut.pixel_centre {
        PixelCentre::Integer => 0.0,
        PixelCentre::Half => 0.5,
    };
    let (w, h) = (lut.width as usize, lut.height as usize);
    let at = |i: usize, j: usize| -> [f64; 2] {
        let k = 2 * (j * w + i);
        [f64::from(lut.data[k]), f64::from(lut.data[k + 1])]
    };
    let diff = |a: [f64; 2], b: [f64; 2], d: f64| [(b[0] - a[0]) / d, (b[1] - a[1]) / d];
    let n = taps as usize;
    let weights: Vec<f64> = (0..n).map(|t| (t as f64 + 0.5) / n as f64 - 0.5).collect();
    let mut out = LinearImage::black(lut.width, lut.height);
    for j in 0..h {
        for i in 0..w {
            let c = at(i, j);
            if !(c[0].is_finite() && c[1].is_finite()) {
                continue;
            }
            let (i0, i1) = (i.saturating_sub(1), (i + 1).min(w - 1));
            let (j0, j1) = (j.saturating_sub(1), (j + 1).min(h - 1));
            let du = diff(at(i0, j), at(i1, j), (i1 - i0).max(1) as f64);
            let dv = diff(at(i, j0), at(i, j1), (j1 - j0).max(1) as f64);
            let (du, dv) = (
                if du.iter().all(|x| x.is_finite()) {
                    du
                } else {
                    [0.0; 2]
                },
                if dv.iter().all(|x| x.is_finite()) {
                    dv
                } else {
                    [0.0; 2]
                },
            );
            let mut acc = [0.0_f32; 3];
            for &b in &weights {
                for &a in &weights {
                    let x = c[0] + a * du[0] + b * dv[0] - off;
                    let y = c[1] + a * du[1] + b * dv[1] - off;
                    let p = canonical.bilinear(x, y);
                    for (a, v) in acc.iter_mut().zip(p) {
                        *a += v;
                    }
                }
            }
            let norm = (n * n) as f32;
            let k = 3 * (j * w + i);
            for (o, a) in out.rgb[k..k + 3].iter_mut().zip(acc) {
                *o = a / norm;
            }
        }
    }
    out
}

/// The sRGB transfer function (IEC 61966-2-1) of a linear value in `[0, 1]`.
#[must_use]
pub fn srgb_encode(linear: f32) -> f32 {
    let x = linear.clamp(0.0, 1.0);
    if x <= 0.003_130_8 {
        12.92 * x
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    }
}

/// Write `image × exposure` as an 8-bit sRGB PNG. A placeholder for the P4-6
/// sensor model (gain, noise, quantisation), which replaces it.
///
/// # Errors
///
/// [`Error::InvalidInput`] if the file cannot be written.
pub fn write_png_srgb(image: &LinearImage, exposure: f32, path: &Path) -> Result<()> {
    let bad = |m: String| Error::InvalidInput(format!("{}: {m}", path.display()));
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| bad(e.to_string()))?;
    }
    let file = std::fs::File::create(path).map_err(|e| bad(e.to_string()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), image.width, image.height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
    let bytes: Vec<u8> = image
        .rgb
        .iter()
        .map(|&v| (srgb_encode(v * exposure) * 255.0 + 0.5) as u8)
        .collect();
    let mut writer = encoder.write_header().map_err(|e| bad(e.to_string()))?;
    writer
        .write_image_data(&bytes)
        .map_err(|e| bad(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: u32, h: u32) -> LinearImage {
        let mut img = LinearImage::black(w, h);
        for y in 0..h {
            for x in 0..w {
                let k = 3 * (y * w + x) as usize;
                img.rgb[k] = x as f32;
                img.rgb[k + 1] = y as f32;
            }
        }
        img
    }

    #[test]
    fn bilinear_interpolates_and_clamps() {
        let img = gradient(4, 3);
        assert_eq!(img.bilinear(1.5, 0.25), [1.5, 0.25, 0.0]);
        assert_eq!(img.bilinear(-3.0, 9.0), [0.0, 2.0, 0.0]);
    }

    #[test]
    fn remap_follows_the_lut_and_its_convention() {
        let img = gradient(4, 3);
        // Two output pixels: one samples canonical (2, 1), the other has no entry.
        let lut = RemapLut {
            width: 2,
            height: 1,
            pixel_centre: PixelCentre::Half,
            data: vec![2.5, 1.5, f32::NAN, f32::NAN],
        };
        let out = remap_image(&img, &lut);
        assert_eq!(&out.rgb[0..3], &[2.0, 1.0, 0.0]);
        assert_eq!(&out.rgb[3..6], &[0.0, 0.0, 0.0]);
        let lut = RemapLut {
            pixel_centre: PixelCentre::Integer,
            data: vec![2.0, 1.0, 0.0, 0.0],
            ..lut
        };
        assert_eq!(&remap_image(&img, &lut).rgb[0..3], &[2.0, 1.0, 0.0]);
    }

    #[test]
    fn box_resampling_integrates_the_footprint() {
        // A 1-D step at canonical x = 2 (values 0 | 1), identity LUT scaled 4×:
        // output pixel i covers canonical [4i − 2, 4i + 2) around 4i.
        let mut canonical = LinearImage::black(16, 1);
        for x in 8..16 {
            canonical.rgb[3 * x] = 1.0;
        }
        let lut = RemapLut {
            width: 4,
            height: 1,
            pixel_centre: PixelCentre::Integer,
            data: (0..4).flat_map(|i| [4.0 * i as f32 + 0.0, 0.0]).collect(),
        };
        let point = remap_image(&canonical, &lut);
        let area = remap_image_box(&canonical, &lut, 4);
        // Pixel 2 is centred on canonical 8, the first bright column: a point
        // sample reads 1; its footprint [6, 10) straddles the step at 7.5, so the
        // box average is ≈ ½ (taps at 6.5, 7.5, 8.5, 9.5 → 0, ½, 1, 1).
        assert_eq!(point.rgb[6], 1.0);
        assert!((area.rgb[6] - 0.625).abs() < 1e-6, "{}", area.rgb[6]);
        assert_eq!(area.rgb[0], 0.0);
        assert_eq!(area.rgb[9], 1.0);
        assert_eq!(remap_image_box(&canonical, &lut, 1), point);
    }

    #[test]
    fn srgb_endpoints() {
        assert_eq!(srgb_encode(0.0), 0.0);
        assert!((srgb_encode(1.0) - 1.0).abs() < 1e-6);
        assert!((srgb_encode(0.5) - 0.735_356_7).abs() < 1e-5);
        assert_eq!(srgb_encode(-1.0), 0.0);
    }
}
