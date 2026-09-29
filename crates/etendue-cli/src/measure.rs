//! Gate measurements that need Blender (PLAN §6: local only).
//!
//! `etendue measure g4-1` — probe P4-2: render small emissive spheres at known
//! 3D points through the canonical camera and the remap LUT, and compare each
//! blob's intensity-weighted centroid with the calibration-rs projection of
//! the sphere's centre. Gate G4.1: ≤ 0.01 px. Run under both pixel-centre
//! conventions; each is read out in its own convention.

use std::path::PathBuf;

use anyhow::{Result, anyhow};
use etendue_synth::images::{LinearImage, read_exr_combined, remap_image_box};
use etendue_synth::job::{
    Device, JOB_VERSION, JobCamera, JobOutput, JobShot, JobSphere, RenderJob, RenderSettings,
    build_job, row_major,
};
use etendue_synth::{CanonicalCamera, CanonicalSpec, PixelCentre, remap_lut};
use nalgebra::{Isometry3, Point2};
use vision_calibration_core::{
    BrownConrady5, CameraParams, DistortionParams, FxFyCxCySkew, IntrinsicsParams,
    ProjectionParams, ScheimpflugParams, SensorParams,
};

use etendue_cli::Loaded;
use etendue_cli::render::{PIXEL_CENTRE, meshes, run_blender};

/// Options of `etendue measure g4-1`.
pub struct ProbeArgs {
    pub output: PathBuf,
    pub samples: u32,
    pub supersample: f64,
    pub blender: Option<PathBuf>,
    pub allow_blender_version: bool,
}

const RESOLUTION: [u32; 2] = [1280, 1024];
const DEPTH_M: f64 = 0.5;
/// ≈ 4.6 px image radius: large enough that a smooth blob's centroid does not lock
/// to the pixel grid; the (N·V)² weighting keeps the off-axis perspective bias
/// of a finite sphere below 1e-3 px.
const RADIUS_M: f64 = 0.001;
const WINDOW_PX: i64 = 10;
const GATE_PX: f64 = 0.01;

fn intrinsics(fx: f64, fy: f64, cx: f64, cy: f64, skew: f64) -> IntrinsicsParams {
    IntrinsicsParams::FxFyCxCySkew {
        params: FxFyCxCySkew {
            fx,
            fy,
            cx,
            cy,
            skew,
        },
    }
}

/// The probe cameras: the examples' distorted camera, and an off-centre,
/// skewed, tilted one that exercises every LUT stage.
fn cameras() -> Vec<(&'static str, CameraParams)> {
    vec![
        (
            "brown",
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
                intrinsics: intrinsics(2318.8, 2318.8, 640.0, 512.0, 0.0),
            },
        ),
        (
            "offcentre_skew_tilt",
            CameraParams {
                projection: ProjectionParams::Pinhole,
                distortion: DistortionParams::BrownConrady5 {
                    params: BrownConrady5 {
                        k1: -0.05,
                        k2: 0.01,
                        k3: 0.0,
                        p1: 2e-4,
                        p2: -1e-4,
                        iters: 8,
                    },
                },
                sensor: SensorParams::Scheimpflug {
                    params: ScheimpflugParams {
                        tilt_x: 2f64.to_radians(),
                        tilt_y: -1f64.to_radians(),
                    },
                },
                intrinsics: intrinsics(2300.0, 2296.0, 612.3, 541.7, 0.6),
            },
        ),
    ]
}

/// Sphere centres: a 7 × 5 grid of pixels (off pixel centres) back-projected to
/// `DEPTH_M` in front of the camera at the world origin.
fn spheres(params: &CameraParams) -> Result<Vec<[f64; 3]>> {
    let model = params.build()?;
    let [w, h] = RESOLUTION.map(f64::from);
    let mut out = Vec::new();
    for r in 0..5 {
        for c in 0..7 {
            let u = 90.0 + (w - 180.0) * f64::from(c) / 6.0 + 0.37;
            let v = 90.0 + (h - 180.0) * f64::from(r) / 4.0 + 0.21;
            let ray = model.backproject_pixel(&Point2::new(u, v)).point;
            let p = ray * (DEPTH_M / ray.z);
            out.push([p.x, p.y, p.z]);
        }
    }
    Ok(out)
}

/// Intensity-weighted centroid of `img` (luminance) in a window around `near`,
/// in the coordinates of `centre` (pixel `i` at `i + offset`).
fn centroid(img: &LinearImage, near: [f64; 2], centre: PixelCentre) -> Option<[f64; 2]> {
    let off = centre.offset();
    let (ci, cj) = (
        (near[0] - off).round() as i64,
        (near[1] - off).round() as i64,
    );
    let (mut sx, mut sy, mut s) = (0.0, 0.0, 0.0);
    for j in cj - WINDOW_PX..=cj + WINDOW_PX {
        for i in ci - WINDOW_PX..=ci + WINDOW_PX {
            if i < 0 || j < 0 || i >= i64::from(img.width) || j >= i64::from(img.height) {
                return None;
            }
            let k = 3 * (j as usize * img.width as usize + i as usize);
            let y = f64::from(img.rgb[k] + img.rgb[k + 1] + img.rgb[k + 2]);
            sx += y * (i as f64 + off);
            sy += y * (j as f64 + off);
            s += y;
        }
    }
    (s > 0.0).then(|| [sx / s, sy / s])
}

pub fn g4_1(args: &ProbeArgs) -> Result<()> {
    let root = std::env::current_dir()?;
    let etendue_cli::render::Blender { exe, version, .. } =
        crate::checked_blender(args.blender.as_deref(), args.allow_blender_version, &root)?;
    let out = &args.output;
    let spec = CanonicalSpec {
        supersample: args.supersample,
        ..CanonicalSpec::default()
    };
    println!(
        "G4.1 convention probe: Blender {version}, {} samples, s = {}, spheres r = {} mm at {} m",
        args.samples,
        args.supersample,
        RADIUS_M * 1e3,
        DEPTH_M
    );
    let mut worst = 0.0_f64;
    for (name, params) in cameras() {
        let centres = spheres(&params)?;
        let model = params.build()?;
        for centre in [PixelCentre::Integer, PixelCentre::Half] {
            let canonical = CanonicalCamera::cover(&params, RESOLUTION, &spec, centre)?;
            let job = RenderJob {
                version: JOB_VERSION,
                render: RenderSettings {
                    samples: args.samples,
                    seed: 0,
                    device: Device::Gpu,
                    ambient: 0.0,
                    clip: [0.01, 10.0],
                },
                meshes: vec![],
                boards: vec![],
                lights: vec![],
                spheres: centres
                    .iter()
                    .enumerate()
                    .map(|(i, c)| JobSphere {
                        id: format!("s{i}"),
                        center: *c,
                        radius: RADIUS_M,
                        emission: 1.0,
                    })
                    .collect(),
                cameras: vec![JobCamera {
                    id: name.into(),
                    frame: "camera".into(),
                    width: canonical.resolution[0],
                    height: canonical.resolution[1],
                    focal_px: canonical.focal_px(),
                }],
                shots: vec![JobShot {
                    capture: "probe".into(),
                    sample: 0,
                    poses: [("camera".to_owned(), row_major(&Isometry3::identity()))].into(),
                    outputs: vec![JobOutput {
                        camera: name.into(),
                        path: format!("exr/{name}.exr"),
                    }],
                }],
            };
            let dir = out.join(format!("{name}-{centre:?}").to_lowercase());
            run_blender(&exe, &job, &dir, &crate::console())?;
            let render = read_exr_combined(&dir.join(format!("exr/{name}.exr")))?;
            let lut = remap_lut(&params, RESOLUTION, &canonical)?;
            let taps = args.supersample.ceil() as u32;
            let image = remap_image_box(&render, &lut, taps);
            let mut errs = Vec::new();
            for c in &centres {
                let p = model
                    .project_point_c(&nalgebra::Vector3::new(c[0], c[1], c[2]))
                    .ok_or_else(|| anyhow!("{name}: a probe sphere does not project"))?;
                let m = centroid(&image, [p.x, p.y], centre)
                    .ok_or_else(|| anyhow!("{name}: no blob near ({:.1}, {:.1})", p.x, p.y))?;
                errs.push(((m[0] - p.x), (m[1] - p.y)));
                if std::env::var_os("ETENDUE_PROBE_VERBOSE").is_some() {
                    println!(
                        "    {name} {centre:?} at ({:8.2}, {:8.2}): err ({:+.4}, {:+.4})",
                        p.x,
                        p.y,
                        m[0] - p.x,
                        m[1] - p.y
                    );
                }
            }
            let n = errs.len() as f64;
            let mean = (
                errs.iter().map(|e| e.0).sum::<f64>() / n,
                errs.iter().map(|e| e.1).sum::<f64>() / n,
            );
            let max = errs.iter().map(|e| e.0.hypot(e.1)).fold(0.0, f64::max);
            let rms = (errs.iter().map(|e| e.0 * e.0 + e.1 * e.1).sum::<f64>() / n).sqrt();
            worst = worst.max(max);
            println!(
                "  {name:<22} {centre:<8?} mean ({:+.4}, {:+.4}) px   rms {rms:.4} px   max {max:.4} px   {}",
                mean.0,
                mean.1,
                if max <= GATE_PX { "PASS" } else { "FAIL" }
            );
        }
    }
    println!(
        "  result: {} (worst {worst:.4} px, gate {GATE_PX} px)",
        if worst <= GATE_PX { "PASS" } else { "FAIL" }
    );
    Ok(())
}

/// P4-5: render the same job twice on each device and report how much the
/// renders differ. `loaded` / `baked` are the scene to render (its first
/// capture, first camera).
pub fn determinism(
    loaded: &Loaded,
    baked: &etendue_scene::BakedScenario,
    args: &ProbeArgs,
) -> Result<()> {
    let etendue_cli::render::Blender { exe, version, .. } = crate::checked_blender(
        args.blender.as_deref(),
        args.allow_blender_version,
        &loaded.dir,
    )?;
    let camera = loaded
        .scene
        .cameras
        .first()
        .ok_or_else(|| anyhow!("the scene has no camera"))?;
    let spec = CanonicalSpec {
        supersample: args.supersample,
        ..CanonicalSpec::default()
    };
    let canonical = CanonicalCamera::cover(&camera.params, camera.resolution, &spec, PIXEL_CENTRE)?;
    let mut job = build_job(
        &loaded.scene,
        baked,
        meshes(loaded)?,
        &[(camera.id.as_str(), &canonical)],
        RenderSettings {
            samples: args.samples,
            ..RenderSettings::default()
        },
    )?;
    job.shots.truncate(1);
    println!(
        "P4-5 determinism: Blender {version}, {} samples, camera `{}`, capture `{}`, canonical {}×{}",
        args.samples,
        camera.id,
        job.shots[0].capture,
        canonical.resolution[0],
        canonical.resolution[1]
    );
    let exr = &job.shots[0].outputs[0].path;
    let mut renders = Vec::new();
    for device in [Device::Gpu, Device::Cpu] {
        for run in 0..2 {
            job.render.device = device;
            let dir = args.output.join(format!("{device:?}-{run}").to_lowercase());
            let t = std::time::Instant::now();
            run_blender(&exe, &job, &dir, &crate::console())?;
            let secs = t.elapsed().as_secs_f64();
            renders.push((device, run, secs, read_exr_combined(&dir.join(exr))?));
        }
    }
    let diff = |a: &LinearImage, b: &LinearImage| -> (f32, f32, usize) {
        let mut max = 0.0_f32;
        let mut sum = 0.0_f64;
        let mut differing = 0usize;
        for (x, y) in a.rgb.iter().zip(&b.rgb) {
            let d = (x - y).abs();
            max = max.max(d);
            sum += f64::from(d);
            if x.to_bits() != y.to_bits() {
                differing += 1;
            }
        }
        (max, (sum / a.rgb.len() as f64) as f32, differing)
    };
    for (device, run, secs, _) in &renders {
        println!("  {device:?} run {run}: {secs:.1} s");
    }
    let mean_level =
        renders[0].3.rgb.iter().map(|&v| f64::from(v)).sum::<f64>() / renders[0].3.rgb.len() as f64;
    println!("  mean radiance {mean_level:.4}");
    for (label, a, b) in [
        ("GPU run 0 vs run 1", 0, 1),
        ("CPU run 0 vs run 1", 2, 3),
        ("GPU vs CPU (run 0)", 0, 2),
    ] {
        let (max, mean, differing) = diff(&renders[a].3, &renders[b].3);
        println!(
            "  {label:<20} max |Δ| {max:.3e}   mean |Δ| {mean:.3e}   differing values {differing} / {}",
            renders[a].3.rgb.len()
        );
    }
    Ok(())
}
