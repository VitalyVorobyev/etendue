//! A linear image-sensor model (P4-6), after EMVA 1288: radiance → photo-electrons
//! (with photo-response non-uniformity) → shot, dark and read noise → full-well clip →
//! conversion gain and black level → quantisation.
//!
//! Every random draw is seeded: the PRNU map from the sensor's `seed`, temporal noise from
//! `(seed, frame)`. The generator is ChaCha8, stable across platforms and versions, so a
//! dataset re-renders bit-identically. No OS entropy is used.

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use rand_distr::{Distribution, Normal, Poisson};
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// A monochrome sensor's photometric parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SensorModel {
    /// Photo-electrons per unit of linear scene radiance (the renderer's output) at the
    /// chosen exposure: exposure time × aperture × quantum efficiency, folded into one number.
    pub electrons_per_unit: f64,
    /// Linear RGB → sensor response weights (a mono sensor's spectral sensitivity), summing
    /// to about 1. Default: Rec. 709 luminance.
    #[serde(default = "rec709")]
    pub rgb_weights: [f64; 3],
    /// Photo-response non-uniformity: relative standard deviation of per-pixel gain.
    #[serde(default)]
    pub prnu: f64,
    /// Mean dark current, electrons per exposure.
    #[serde(default)]
    pub dark_electrons: f64,
    /// Read noise, electrons (standard deviation).
    #[serde(default)]
    pub read_noise_electrons: f64,
    /// Full-well capacity, electrons.
    pub full_well_electrons: f64,
    /// Conversion gain K, digital numbers per electron.
    pub gain_dn_per_electron: f64,
    /// Black level (offset), DN.
    #[serde(default)]
    pub black_level_dn: f64,
    /// ADC bit depth, 1 … 16.
    pub bits: u32,
    /// Seed of the fixed pattern (PRNU); temporal noise mixes in the frame index.
    #[serde(default)]
    pub seed: u64,
}

fn rec709() -> [f64; 3] {
    [0.2126, 0.7152, 0.0722]
}

/// A digital image: row-major DN values, top row first.
#[derive(Clone, Debug, PartialEq)]
pub struct RawImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Bit depth of the values.
    pub bits: u32,
    /// `width · height` values in `0 … 2^bits − 1`.
    pub dn: Vec<u16>,
}

impl SensorModel {
    fn check(&self) -> Result<()> {
        let bad = |m: &str| Err(Error::InvalidInput(format!("sensor: {m}")));
        if !(self.electrons_per_unit.is_finite() && self.electrons_per_unit >= 0.0) {
            return bad("electrons_per_unit must be ≥ 0");
        }
        if !(self.full_well_electrons.is_finite() && self.full_well_electrons > 0.0) {
            return bad("full_well_electrons must be > 0");
        }
        if !(self.gain_dn_per_electron.is_finite() && self.gain_dn_per_electron > 0.0) {
            return bad("gain_dn_per_electron must be > 0");
        }
        if !(1..=16).contains(&self.bits) {
            return bad("bits must be in 1 … 16");
        }
        for (name, v) in [
            ("prnu", self.prnu),
            ("dark_electrons", self.dark_electrons),
            ("read_noise_electrons", self.read_noise_electrons),
            ("black_level_dn", self.black_level_dn),
        ] {
            if !(v.is_finite() && v >= 0.0) {
                return bad(&format!("{name} must be ≥ 0"));
            }
        }
        Ok(())
    }

    /// The per-pixel PRNU gain map (mean 1) of a `width × height` sensor.
    fn prnu_map(&self, n: usize) -> Vec<f64> {
        if self.prnu == 0.0 {
            return vec![1.0; n];
        }
        let mut rng = ChaCha8Rng::seed_from_u64(self.seed);
        let normal = Normal::new(1.0, self.prnu).expect("prnu is finite and ≥ 0");
        (0..n).map(|_| normal.sample(&mut rng).max(0.0)).collect()
    }

    /// Expose one frame: `rgb` is linear radiance (3 values per pixel, row-major), `frame`
    /// selects the temporal noise (use the capture index).
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] for invalid parameters or a buffer that is not
    /// `3 · width · height` long.
    pub fn expose(&self, rgb: &[f32], width: u32, height: u32, frame: u64) -> Result<RawImage> {
        self.check()?;
        let n = width as usize * height as usize;
        if rgb.len() != 3 * n {
            return Err(Error::InvalidInput(format!(
                "sensor: {} values for {width}×{height} RGB pixels",
                rgb.len()
            )));
        }
        let prnu = self.prnu_map(n);
        // Temporal noise stream: independent of the PRNU stream for every frame.
        let mut rng = ChaCha8Rng::seed_from_u64(
            self.seed ^ 0x9E37_79B9_7F4A_7C15_u64.wrapping_mul(frame + 1),
        );
        let read =
            Normal::new(0.0, self.read_noise_electrons).expect("read noise is finite and ≥ 0");
        let max_dn = f64::from((1u32 << self.bits) - 1);
        let [wr, wg, wb] = self.rgb_weights;
        let mut dn = Vec::with_capacity(n);
        for (i, px) in rgb.chunks_exact(3).enumerate() {
            let radiance = wr * f64::from(px[0]) + wg * f64::from(px[1]) + wb * f64::from(px[2]);
            let mean_e =
                (radiance.max(0.0) * self.electrons_per_unit * prnu[i]) + self.dark_electrons;
            let electrons = poisson(&mut rng, mean_e) + read.sample(&mut rng);
            let electrons = electrons.clamp(0.0, self.full_well_electrons);
            let value = self.black_level_dn + self.gain_dn_per_electron * electrons;
            // Round to the nearest code: uniform quantisation noise of 1/12 DN².
            dn.push(value.round().clamp(0.0, max_dn) as u16);
        }
        Ok(RawImage {
            width,
            height,
            bits: self.bits,
            dn,
        })
    }
}

/// A Poisson draw of mean `lambda` (0 for `lambda ≤ 0`).
fn poisson(rng: &mut ChaCha8Rng, lambda: f64) -> f64 {
    if lambda <= 0.0 {
        return 0.0;
    }
    // Deterministic given the stream. `Poisson::new` rejects only non-finite means, which
    // the parameter checks exclude; fall back to the mean rather than panic.
    Poisson::new(lambda).map_or(lambda, |p| p.sample(rng))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> SensorModel {
        SensorModel {
            electrons_per_unit: 10_000.0,
            rgb_weights: rec709(),
            prnu: 0.01,
            dark_electrons: 5.0,
            read_noise_electrons: 3.0,
            full_well_electrons: 12_000.0,
            gain_dn_per_electron: 0.25,
            black_level_dn: 16.0,
            bits: 12,
            seed: 7,
        }
    }

    fn flat(level: f32, n: usize) -> Vec<f32> {
        vec![level; 3 * n]
    }

    #[test]
    fn frames_are_reproducible_and_distinct() {
        let m = model();
        let a = m.expose(&flat(0.5, 1024), 32, 32, 0).unwrap();
        assert_eq!(a, m.expose(&flat(0.5, 1024), 32, 32, 0).unwrap());
        assert_ne!(a.dn, m.expose(&flat(0.5, 1024), 32, 32, 1).unwrap().dn);
    }

    #[test]
    fn black_level_clipping_and_bit_depth() {
        let m = SensorModel {
            prnu: 0.0,
            dark_electrons: 0.0,
            read_noise_electrons: 0.0,
            ..model()
        };
        let dark = m.expose(&flat(0.0, 16), 4, 4, 0).unwrap();
        assert!(dark.dn.iter().all(|&v| v == 16));
        // Far beyond full well: clipped to 16 + 0.25 · 12000 = 3016 DN.
        let bright = m.expose(&flat(100.0, 16), 4, 4, 0).unwrap();
        assert!(bright.dn.iter().all(|&v| v == 3016));
        let tiny = SensorModel { bits: 8, ..m };
        assert!(
            tiny.expose(&flat(100.0, 16), 4, 4, 0)
                .unwrap()
                .dn
                .iter()
                .all(|&v| v == 255)
        );
    }

    #[test]
    fn rejects_bad_parameters_and_buffers() {
        for bad in [
            SensorModel { bits: 0, ..model() },
            SensorModel {
                gain_dn_per_electron: 0.0,
                ..model()
            },
            SensorModel {
                prnu: -1.0,
                ..model()
            },
            SensorModel {
                full_well_electrons: 0.0,
                ..model()
            },
            SensorModel {
                electrons_per_unit: f64::NAN,
                ..model()
            },
        ] {
            assert!(bad.expose(&flat(0.5, 4), 2, 2, 0).is_err());
        }
        assert!(model().expose(&[0.0; 5], 2, 2, 0).is_err());
    }

    #[test]
    fn json_defaults() {
        let m: SensorModel = serde_json::from_str(
            r#"{"electrons_per_unit": 1000, "full_well_electrons": 10000, "gain_dn_per_electron": 0.1, "bits": 10}"#,
        )
        .unwrap();
        assert_eq!(m.rgb_weights, rec709());
        assert_eq!(m.prnu, 0.0);
    }
}
