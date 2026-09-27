//! The camera set gate G3.1 runs over (`tests/g3_1.rs`, `examples/g3_1_remap.rs`).
//!
//! One camera per supported calibration-rs model family, with parameters of the
//! magnitude real lenses have — including a strong wide-angle barrel and
//! Scheimpflug tilts up to 6° — at 2048 × 1536 px. Hidden from the docs: it is
//! test fixture data, shared by the gate test and its report.

use vision_calibration_core::{
    BrownConrady5, CameraParams, DistortionParams, FxFyCxCySkew, IntrinsicsParams,
    ProjectionParams, RationalPolynomial, ScheimpflugParams, SensorParams, ThinPrism,
};

/// Image size of every gate camera.
pub const RESOLUTION: [u32; 2] = [2048, 1536];

fn camera(
    distortion: DistortionParams,
    sensor: SensorParams,
    k: FxFyCxCySkew<f64>,
) -> CameraParams {
    CameraParams {
        projection: ProjectionParams::Pinhole,
        distortion,
        sensor,
        intrinsics: IntrinsicsParams::FxFyCxCySkew { params: k },
    }
}

fn k(fx: f64, fy: f64, cx: f64, cy: f64, skew: f64) -> FxFyCxCySkew<f64> {
    FxFyCxCySkew {
        fx,
        fy,
        cx,
        cy,
        skew,
    }
}

fn brown(k1: f64, k2: f64, k3: f64, p1: f64, p2: f64) -> DistortionParams {
    DistortionParams::BrownConrady5 {
        params: BrownConrady5 {
            k1,
            k2,
            k3,
            p1,
            p2,
            iters: 8,
        },
    }
}

fn tilt(deg_x: f64, deg_y: f64) -> SensorParams {
    SensorParams::Scheimpflug {
        params: ScheimpflugParams {
            tilt_x: deg_x.to_radians(),
            tilt_y: deg_y.to_radians(),
        },
    }
}

/// `(name, params)` for every gate camera.
#[must_use]
pub fn cameras() -> Vec<(&'static str, CameraParams)> {
    let c = k(1800.0, 1800.0, 1023.5, 767.5, 0.0);
    let id = || SensorParams::Identity;
    vec![
        ("pinhole", camera(DistortionParams::None, id(), c)),
        (
            "pinhole_skew_offcentre",
            camera(
                DistortionParams::None,
                id(),
                k(1800.0, 1795.0, 1000.0, 790.0, 0.8),
            ),
        ),
        (
            "brown_mild",
            camera(brown(-0.08, 0.02, 0.0, 0.0, 0.0), id(), c),
        ),
        (
            "brown_strong_barrel",
            camera(brown(-0.35, 0.15, -0.03, 5e-4, -3e-4), id(), c),
        ),
        (
            "brown_pincushion",
            camera(brown(0.15, 0.05, 0.0, 0.0, 0.0), id(), c),
        ),
        (
            "rational",
            camera(
                DistortionParams::Rational {
                    params: RationalPolynomial {
                        k1: 0.8,
                        k2: 0.2,
                        k3: 0.01,
                        k4: 1.1,
                        k5: 0.35,
                        k6: 0.02,
                        p1: 1e-4,
                        p2: -1e-4,
                        iters: 10,
                    },
                },
                id(),
                c,
            ),
        ),
        (
            "thin_prism",
            camera(
                DistortionParams::ThinPrism {
                    params: ThinPrism {
                        k1: -0.1,
                        k2: 0.03,
                        k3: 0.0,
                        p1: 2e-4,
                        p2: -1e-4,
                        s1: 1e-3,
                        s2: -5e-4,
                        s3: 8e-4,
                        s4: 2e-4,
                        iters: 10,
                    },
                },
                id(),
                c,
            ),
        ),
        (
            "division",
            camera(DistortionParams::Division { lambda: -0.25 }, id(), c),
        ),
        (
            "scheimpflug_6x_brown",
            camera(brown(-0.08, 0.02, 0.0, 0.0, 0.0), tilt(6.0, 0.0), c),
        ),
        (
            "scheimpflug_4x4_barrel",
            camera(brown(-0.35, 0.15, -0.03, 5e-4, -3e-4), tilt(4.2, 4.2), c),
        ),
    ]
}

/// A copy of `params` whose iterative undistortion runs `iters` iterations
/// (`None` for a model that has no iteration count).
#[must_use]
pub fn with_iters(params: &CameraParams, iters: u32) -> Option<CameraParams> {
    let mut p = params.clone();
    match &mut p.distortion {
        DistortionParams::BrownConrady5 { params } => params.iters = iters,
        DistortionParams::Rational { params } => params.iters = iters,
        DistortionParams::ThinPrism { params } => params.iters = iters,
        DistortionParams::None | DistortionParams::Division { .. } => return None,
    }
    Some(p)
}

/// SplitMix64: a fixed, dependency-free stream of test pixels.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    /// A stream from `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// Uniform in `[0, 1)`.
    pub fn next_f64(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 53) as f64
    }
}
