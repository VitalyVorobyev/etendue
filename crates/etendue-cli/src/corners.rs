//! `etendue measure g4-2` — the P4-3 corner bias study.
//!
//! Render a chessboard in Blender at several poses and supersampling factors,
//! resample through the remap LUT, detect corners with `chess-corners`, and
//! compare them with the analytic ground truth (`etendue-synth::gt`). Gate
//! G4.2: RMS ≤ 0.02 px at the chosen default.
//!
//! Renders are noise-free in the sense of P4-3: no sensor model, a uniform
//! white environment (a Lambertian plane under it has no shading to sample),
//! and enough Cycles samples per target pixel that the pixel-filter jitter
//! averages out. The detector sees 8-bit luminance, linear (as a sensor
//! delivers it) and sRGB-encoded (as `etendue render` writes by default).

use anyhow::{Context, Result, anyhow};
use chess_corners::{ChessRefiner, CornerDescriptor, Detector, DetectorConfig};
use etendue_synth::gt::{VisibilitySpec, project_points};
use etendue_synth::images::{LinearImage, read_exr_combined, remap_image_box, srgb_encode};
use etendue_synth::job::{
    Device, JOB_VERSION, JobBoard, JobCamera, JobOutput, JobShot, RenderJob, RenderSettings,
    TARGET_PASS_INDEX, row_major,
};
use etendue_synth::{CanonicalCamera, CanonicalSpec, remap_lut};
use nalgebra::{Isometry3, Translation3, UnitQuaternion, Vector3};
use vision_calibration_core::{
    BrownConrady5, CameraParams, DistortionParams, FxFyCxCySkew, IntrinsicsParams,
    ProjectionParams, SensorParams,
};

use crate::measure::ProbeArgs;
use crate::render::{PIXEL_CENTRE, checked_blender, run_blender};

const RESOLUTION: [u32; 2] = [1280, 1024];
/// Squares of the board (10 × 8, so 9 × 7 inner corners).
const SQUARES: [u32; 2] = [10, 8];
const SQUARE_M: f64 = 0.02;
const SUPERSAMPLING: [f64; 4] = [1.0, 2.0, 4.0, 8.0];
/// A detection within this distance of a visible ground-truth corner is its match.
const MATCH_PX: f64 = 1.5;
/// Keep corners this far from the image edge (the detector's ring radius and
/// refiner window must fit).
const MARGIN_PX: f64 = 12.0;
const GATE_PX: f64 = 0.02;
/// Gaussian PSF applied to the resampled image before detection (pixels): 0 is
/// the perfectly sharp render; real lenses are ≳ 0.5 px.
const PSF_SIGMA_PX: [f64; 4] = [0.0, 0.5, 0.7, 1.0];
/// calib-targets' workspace ChESS threshold (`calib_targets_core::default_chess_config`).
const CHESS_THRESHOLD: f32 = 15.0;

/// The examples' camera (the G4.1 `brown` probe camera).
fn camera() -> CameraParams {
    CameraParams {
        projection: ProjectionParams::Pinhole,
        distortion: DistortionParams::BrownConrady5 {
            params: BrownConrady5 {
                k1: -0.08,
                k2: 0.02,
                k3: 0.0,
                p1: 0.0,
                p2: 0.0,
                iters: 8,
            },
        },
        sensor: SensorParams::Identity,
        intrinsics: IntrinsicsParams::FxFyCxCySkew {
            params: FxFyCxCySkew {
                fx: 2318.8,
                fy: 2318.8,
                cx: 640.0,
                cy: 512.0,
                skew: 0.0,
            },
        },
    }
}

/// `camera_se3_target` of each pose (camera at the world origin). The target's
/// +Z faces the camera; tilts are about the board's own axes.
fn poses() -> Vec<(&'static str, Isometry3<f64>)> {
    let facing = UnitQuaternion::from_axis_angle(&Vector3::x_axis(), std::f64::consts::PI);
    let pose = |t: [f64; 3], tilt_x: f64, tilt_y: f64, spin: f64| {
        let tilt = UnitQuaternion::from_axis_angle(&Vector3::x_axis(), tilt_x.to_radians())
            * UnitQuaternion::from_axis_angle(&Vector3::y_axis(), tilt_y.to_radians())
            * UnitQuaternion::from_axis_angle(&Vector3::z_axis(), spin.to_radians());
        Isometry3::from_parts(Translation3::new(t[0], t[1], t[2]), facing * tilt)
    };
    vec![
        ("frontal", pose([0.0, 0.0, 0.5], 0.0, 0.0, 0.0)),
        ("tilt_x35", pose([0.0, 0.0, 0.5], 35.0, 0.0, 0.0)),
        ("tilt_y35", pose([0.0, 0.0, 0.5], 0.0, 35.0, 0.0)),
        ("oblique", pose([0.03, -0.02, 0.55], 25.0, -25.0, 15.0)),
        ("far", pose([0.0, 0.0, 1.0], 20.0, 10.0, 5.0)),
    ]
}

/// The board as calib-targets prints it (P3-3): the job draws its cells, and
/// its points are the ground truth. `analytic_images` draws the same squares by
/// the print's parity: dark where column + row is even, counted from the print's
/// top-left, which is the target's −X/+Y corner (`etendue_synth::board`).
fn board() -> Result<etendue_synth::board::BoardLayout> {
    let [cols, rows] = SQUARES;
    let geometry = etendue_scene::TargetGeometry::Board {
        board: vision_calibration_dataset::TargetSpec::Chessboard {
            rows: rows - 1,
            cols: cols - 1,
            square_size_m: SQUARE_M,
        },
    };
    etendue_synth::board::layout(&geometry)?.ok_or_else(|| anyhow!("a chessboard has a layout"))
}

/// 8-bit luminance, scaled so the image's brightest luminance maps to 240.
fn gray8(image: &LinearImage, srgb: bool) -> Vec<u8> {
    let lum: Vec<f32> = image
        .rgb
        .chunks_exact(3)
        .map(|p| 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2])
        .collect();
    let max = lum.iter().copied().fold(0.0_f32, f32::max).max(1e-6);
    lum.iter()
        .map(|&y| {
            let v = (y / max).clamp(0.0, 1.0);
            let v = if srgb { srgb_encode(v) } else { v };
            (v * 240.0).round() as u8
        })
        .collect()
}

#[derive(Default)]
struct Stats {
    visible: usize,
    errors: Vec<[f64; 2]>,
    /// The error of every visible corner in ground-truth order, `None` if
    /// unmatched.
    per_corner: Vec<Option<[f64; 2]>>,
}

impl Stats {
    fn rms(&self) -> f64 {
        let n = self.errors.len().max(1) as f64;
        (self
            .errors
            .iter()
            .map(|e| e[0] * e[0] + e[1] * e[1])
            .sum::<f64>()
            / n)
            .sqrt()
    }
    fn mean(&self) -> [f64; 2] {
        let n = self.errors.len().max(1) as f64;
        [
            self.errors.iter().map(|e| e[0]).sum::<f64>() / n,
            self.errors.iter().map(|e| e[1]).sum::<f64>() / n,
        ]
    }
    fn max(&self) -> f64 {
        self.errors
            .iter()
            .map(|e| e[0].hypot(e[1]))
            .fold(0.0, f64::max)
    }
}

/// Match each visible ground-truth pixel to its nearest detection.
fn score(truth: &[[f64; 2]], found: &[CornerDescriptor], stats: &mut Stats) {
    stats.visible += truth.len();
    for t in truth {
        let nearest = found
            .iter()
            .map(|c| [f64::from(c.x) - t[0], f64::from(c.y) - t[1]])
            .min_by(|a, b| a[0].hypot(a[1]).total_cmp(&b[0].hypot(b[1])));
        let matched = nearest.filter(|e| e[0].hypot(e[1]) <= MATCH_PX);
        stats.errors.extend(matched);
        stats.per_corner.push(matched);
    }
}

const REFINERS: [&str; 3] = ["center_of_mass", "forstner", "saddle_point"];

fn detector(refiner: &str) -> Result<Detector> {
    let refiner = match refiner {
        "forstner" => ChessRefiner::forstner(),
        "saddle_point" => ChessRefiner::saddle_point(),
        _ => ChessRefiner::center_of_mass(),
    };
    Ok(Detector::new(
        DetectorConfig::chess()
            .with_threshold(CHESS_THRESHOLD)
            .with_chess(|c| c.refiner = refiner),
    )?)
}

/// Per-corner errors of one detector setup: `(sRGB, refiner)` → errors.
type Errors = Vec<((bool, &'static str), Vec<Option<[f64; 2]>>)>;

/// Detect on `images` with every encoding and refiner and report one table
/// row each (with the per-pose RMS), tracking the best passing row in `best`.
fn evaluate(
    label: &str,
    images: &[LinearImage],
    truth: &[Vec<[f64; 2]>],
    line: &mut impl FnMut(String),
    best: &mut Option<(f64, String)>,
) -> Result<Errors> {
    let mut out = Errors::new();
    for srgb in [false, true] {
        for refiner in REFINERS {
            let mut det = detector(refiner)?;
            let mut stats = Stats::default();
            let mut per_pose = Vec::new();
            for (image, truth) in images.iter().zip(truth) {
                let found = det.detect_u8(&gray8(image, srgb), image.width, image.height)?;
                let mut pose = Stats::default();
                score(truth, &found, &mut pose);
                per_pose.push(format!("{:.3}", pose.rms()));
                stats.visible += pose.visible;
                stats.errors.extend(pose.errors);
                stats.per_corner.extend(pose.per_corner);
            }
            let rms = stats.rms();
            let mean = stats.mean();
            let complete = stats.errors.len() == stats.visible;
            let pass = complete && rms <= GATE_PX;
            let encoding = if srgb { "sRGB" } else { "linear" };
            line(format!(
                "| {label} | {encoding} | {refiner} | {}/{} | ({:+.4}, {:+.4}) | {rms:.4} | {:.4} | {} | {} |",
                stats.errors.len(),
                stats.visible,
                mean[0],
                mean[1],
                stats.max(),
                per_pose.join(" "),
                if pass { "PASS" } else { "FAIL" }
            ));
            if pass && best.as_ref().is_none_or(|(b, _)| rms < *b) {
                *best = Some((rms, format!("s | σ = {label}, {encoding}, {refiner}")));
            }
            out.push(((srgb, refiner), stats.per_corner));
        }
    }
    Ok(out)
}

/// RMS and max of the per-corner difference between two runs of the same
/// detector (corners matched in both only), and how many corners that is.
fn difference(a: &[Option<[f64; 2]>], b: &[Option<[f64; 2]>]) -> (f64, f64, usize) {
    let d: Vec<f64> = a
        .iter()
        .zip(b)
        .filter_map(|(a, b)| Some((*a)?).zip(*b))
        .map(|(a, b)| (a[0] - b[0]).hypot(a[1] - b[1]))
        .collect();
    let n = d.len().max(1) as f64;
    (
        (d.iter().map(|x| x * x).sum::<f64>() / n).sqrt(),
        d.iter().copied().fold(0.0, f64::max),
        d.len(),
    )
}

/// Separable Gaussian blur of `image` (a lens PSF stand-in), clamped at the
/// border. `sigma` in pixels; `0` returns the image unchanged.
fn blur(image: &LinearImage, sigma: f64) -> LinearImage {
    if sigma <= 0.0 {
        return image.clone();
    }
    let r = (3.0 * sigma).ceil() as i64;
    let kernel: Vec<f32> = (-r..=r)
        .map(|k| (-(k * k) as f64 / (2.0 * sigma * sigma)).exp() as f32)
        .collect();
    let norm: f32 = kernel.iter().sum();
    let (w, h) = (i64::from(image.width), i64::from(image.height));
    let pass = |src: &[f32], horizontal: bool| -> Vec<f32> {
        let mut out = vec![0.0_f32; src.len()];
        for y in 0..h {
            for x in 0..w {
                for c in 0..3 {
                    let mut acc = 0.0_f32;
                    for (k, weight) in (-r..=r).zip(&kernel) {
                        let (sx, sy) = if horizontal {
                            ((x + k).clamp(0, w - 1), y)
                        } else {
                            (x, (y + k).clamp(0, h - 1))
                        };
                        acc += weight * src[(3 * (sy * w + sx) + c) as usize];
                    }
                    out[(3 * (y * w + x) + c) as usize] = acc / norm;
                }
            }
        }
        out
    };
    let rgb = pass(&pass(&image.rgb, true), false);
    LinearImage {
        width: image.width,
        height: image.height,
        rgb,
    }
}

/// Radiance the Blender scene gives each surface under the unit environment:
/// background, light square, dark square (the script's albedos).
const RADIANCE: [f32; 3] = [1.0, 0.85, 0.03];
const ANALYTIC_SUBSAMPLES: u32 = 8;

/// The board of each pose, area-sampled on an `n × n` grid per pixel through
/// `model`'s back-projection: exact geometry, no renderer.
fn analytic_images(
    model: &vision_calibration_core::CameraModel,
    poses: &[(&str, Isometry3<f64>)],
    n: u32,
) -> Vec<LinearImage> {
    let [w, h] = RESOLUTION;
    let [cols, rows] = SQUARES;
    let (bw, bh) = (f64::from(cols) * SQUARE_M, f64::from(rows) * SQUARE_M);
    let inv: Vec<Isometry3<f64>> = poses.iter().map(|(_, p)| p.inverse()).collect();
    let off = PIXEL_CENTRE.offset();
    let threads = std::thread::available_parallelism().map_or(4, usize::from);
    let chunk = (h as usize).div_ceil(threads);
    // Per pose, per pixel: summed radiance.
    let mut sums = vec![vec![0.0_f32; (w * h) as usize]; poses.len()];
    let bands: Vec<Vec<Vec<f32>>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|t| {
                let inv = &inv;
                scope.spawn(move || {
                    let rows_here = (t * chunk)..((t + 1) * chunk).min(h as usize);
                    let mut out = vec![vec![0.0_f32; rows_here.len() * w as usize]; inv.len()];
                    for (jj, j) in rows_here.clone().enumerate() {
                        for i in 0..w as usize {
                            for b in 0..n {
                                for a in 0..n {
                                    let u =
                                        i as f64 + off - 0.5 + (f64::from(a) + 0.5) / f64::from(n);
                                    let v =
                                        j as f64 + off - 0.5 + (f64::from(b) + 0.5) / f64::from(n);
                                    let ray =
                                        model.backproject_pixel(&nalgebra::Point2::new(u, v)).point;
                                    let d = ray;
                                    for (k, iso) in inv.iter().enumerate() {
                                        // Ray and origin in the target frame; hit z = 0.
                                        let o = iso.translation.vector;
                                        let dir = iso.rotation * d;
                                        let lambda = -o.z / dir.z;
                                        let value = if lambda > 0.0 {
                                            // Print coordinates: from the top-left,
                                            // whose top edge is the target's +Y.
                                            let x = o.x + lambda * dir.x + bw / 2.0;
                                            let y = bh / 2.0 - (o.y + lambda * dir.y);
                                            if (0.0..bw).contains(&x) && (0.0..bh).contains(&y) {
                                                let c = (x / SQUARE_M) as u32;
                                                let r = (y / SQUARE_M) as u32;
                                                if (r + c).is_multiple_of(2) {
                                                    RADIANCE[2]
                                                } else {
                                                    RADIANCE[1]
                                                }
                                            } else {
                                                RADIANCE[0]
                                            }
                                        } else {
                                            RADIANCE[0]
                                        };
                                        out[k][jj * w as usize + i] += value;
                                    }
                                }
                            }
                        }
                    }
                    out
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("analytic band"))
            .collect()
    });
    for (t, band) in bands.into_iter().enumerate() {
        let start = t * chunk * w as usize;
        for (k, values) in band.into_iter().enumerate() {
            sums[k][start..start + values.len()].copy_from_slice(&values);
        }
    }
    let scale = 1.0 / (n * n) as f32;
    sums.into_iter()
        .map(|lum| LinearImage {
            width: w,
            height: h,
            rgb: lum.iter().flat_map(|&y| [y * scale; 3]).collect(),
        })
        .collect()
}

pub fn g4_2(args: &ProbeArgs) -> Result<()> {
    let root = std::env::current_dir()?;
    let (exe, version) =
        checked_blender(args.blender.as_deref(), args.allow_blender_version, &root)?;
    let params = camera();
    let model = params.build()?;
    let board = board()?;
    let points = board.points;
    let poses = poses();
    let [cols, rows] = SQUARES;
    let vis = VisibilitySpec {
        margin_px: MARGIN_PX,
    };
    // Ground truth per pose, visible corners only.
    let truth: Vec<Vec<[f64; 2]>> = poses
        .iter()
        .map(|(_, pose)| {
            project_points(
                &model,
                RESOLUTION,
                &Isometry3::identity(),
                pose,
                &points,
                &vis,
                PIXEL_CENTRE.edge(),
            )
            .into_iter()
            .filter(|g| g.visible())
            .filter_map(|g| g.pixel)
            .collect()
        })
        .collect();

    let mut report = String::new();
    let mut line = |s: String| {
        println!("{s}");
        report.push_str(&s);
        report.push('\n');
    };
    line(format!(
        "G4.2 corner bias: Blender {version}, {} samples per target pixel, {}×{} squares of {} mm, \
         {} poses, pixel centre {PIXEL_CENTRE:?}",
        args.samples,
        cols,
        rows,
        SQUARE_M * 1e3,
        poses.len()
    ));
    line(
        "| s | PSF σ px | encoding | refiner | matched | mean (x, y) px | RMS px | max px | RMS per pose px | G4.2 |"
            .into(),
    );
    line("|---|---|---|---|---|---|---|---|---|---|".into());

    let mut best: Option<(f64, String)> = None;
    let mut rendered = Vec::new();
    for s in SUPERSAMPLING {
        let spec = CanonicalSpec {
            supersample: s,
            ..CanonicalSpec::default()
        };
        let canonical = CanonicalCamera::cover(&params, RESOLUTION, &spec, PIXEL_CENTRE)?;
        let lut = remap_lut(&params, RESOLUTION, &canonical)?;
        // Same samples per target pixel at every s.
        let samples = ((f64::from(args.samples) / (s * s)).ceil() as u32).max(16);
        let job = RenderJob {
            version: JOB_VERSION,
            render: RenderSettings {
                samples,
                seed: 0,
                device: Device::Gpu,
                ambient: 1.0,
                clip: [0.01, 10.0],
            },
            meshes: vec![],
            boards: vec![JobBoard {
                id: "board".into(),
                frame: "board".into(),
                cells: board.cells.clone(),
                pass_index: TARGET_PASS_INDEX,
            }],
            lights: vec![],
            spheres: vec![],
            cameras: vec![JobCamera {
                id: "cam".into(),
                frame: "camera".into(),
                width: canonical.resolution[0],
                height: canonical.resolution[1],
                focal_px: canonical.focal_px(),
            }],
            shots: poses
                .iter()
                .map(|(name, pose)| JobShot {
                    capture: (*name).into(),
                    sample: 0,
                    poses: [
                        ("camera".to_owned(), row_major(&Isometry3::identity())),
                        ("board".to_owned(), row_major(pose)),
                    ]
                    .into(),
                    outputs: vec![JobOutput {
                        camera: "cam".into(),
                        path: format!("exr/{name}.exr"),
                    }],
                })
                .collect(),
        };
        let dir = args.output.join(format!("s{s}"));
        let t = std::time::Instant::now();
        run_blender(&exe, &job, &dir)?;
        eprintln!(
            "  s = {s}: canonical {}×{}, {samples} samples, {:.0} s",
            canonical.resolution[0],
            canonical.resolution[1],
            t.elapsed().as_secs_f64()
        );
        let images: Vec<LinearImage> = poses
            .iter()
            .map(|(name, _)| {
                let render = read_exr_combined(&dir.join(format!("exr/{name}.exr")))
                    .with_context(|| format!("pose {name}"))?;
                Ok(remap_image_box(&render, &lut, s.ceil() as u32))
            })
            .collect::<Result<_>>()?;
        for sigma in PSF_SIGMA_PX {
            let blurred: Vec<LinearImage> = images.iter().map(|i| blur(i, sigma)).collect();
            let errors = evaluate(
                &format!("{s} | {sigma}"),
                &blurred,
                &truth,
                &mut line,
                &mut best,
            )?;
            rendered.push((s, sigma, errors));
        }
    }
    // Reference: the same board area-sampled exactly through the camera model
    // (no renderer). What the detector does on it is detector bias alone.
    let t = std::time::Instant::now();
    let reference = analytic_images(&model, &poses, ANALYTIC_SUBSAMPLES);
    eprintln!(
        "  analytic reference: {ANALYTIC_SUBSAMPLES}² subsamples, {:.0} s",
        t.elapsed().as_secs_f64()
    );
    let mut none = None;
    let mut exact = Vec::new();
    for sigma in PSF_SIGMA_PX {
        let blurred: Vec<LinearImage> = reference.iter().map(|i| blur(i, sigma)).collect();
        let errors = evaluate(
            &format!("analytic | {sigma}"),
            &blurred,
            &truth,
            &mut line,
            &mut none,
        )?;
        exact.push((sigma, errors));
    }

    // The renderer's own contribution: the same detector on the Blender render
    // and on the exact image, corner by corner.
    line(String::new());
    line("Render vs exact image, same detector (linear), per-corner difference:".into());
    line("| s | PSF σ px | refiner | corners | RMS px | max px |".into());
    line("|---|---|---|---|---|---|".into());
    for (s, sigma, errors) in &rendered {
        let reference = &exact
            .iter()
            .find(|(x, _)| x == sigma)
            .expect("every σ has a reference")
            .1;
        for ((srgb, refiner), e) in errors {
            if *srgb {
                continue;
            }
            let r = &reference
                .iter()
                .find(|(k, _)| *k == (false, *refiner))
                .expect("same setups")
                .1;
            let (rms, max, n) = difference(e, r);
            line(format!(
                "| {s} | {sigma} | {refiner} | {n} | {rms:.4} | {max:.4} |"
            ));
        }
    }

    match &best {
        Some((rms, what)) => line(format!(
            "result: PASS at {what} (RMS {rms:.4} px, gate {GATE_PX} px)"
        )),
        None => line(format!(
            "result: FAIL (no configuration within {GATE_PX} px)"
        )),
    }
    std::fs::create_dir_all(&args.output)?;
    std::fs::write(args.output.join("g4_2.md"), report)?;

    // For P4-4 (G4.3): the scene and the Blender detections, so the web probe
    // renders the same frames and compares corner by corner.
    let blender: serde_json::Map<String, serde_json::Value> = rendered
        .iter()
        .filter(|(_, sigma, _)| *sigma == 0.0)
        .map(|(s, _, errors)| {
            let per_refiner: serde_json::Map<String, serde_json::Value> = errors
                .iter()
                .filter(|((srgb, _), _)| !srgb)
                .map(|((_, refiner), e)| ((*refiner).to_owned(), serde_json::json!(e)))
                .collect();
            (format!("{s}"), serde_json::Value::Object(per_refiner))
        })
        .collect();
    let detections = serde_json::json!({
        "resolution": RESOLUTION,
        "camera": params,
        "squares": SQUARES,
        "square_m": SQUARE_M,
        "radiance": RADIANCE,
        "chess_threshold": CHESS_THRESHOLD,
        "margin_px": MARGIN_PX,
        "match_px": MATCH_PX,
        "poses": poses
            .iter()
            .map(|(name, pose)| serde_json::json!({ "name": name, "camera_se3_target": pose }))
            .collect::<Vec<_>>(),
        "truth": truth,
        "errors": blender,
    });
    std::fs::write(
        args.output.join("detections.json"),
        serde_json::to_string_pretty(&detections)? + "\n",
    )?;
    Ok(())
}
