//! Gate R1 (P20a): `etendue real calibrate` equals calibration-rs's own run of the same manifest.
//!
//! For each dataset (`kuka_1` and `DS8`, or the ids in `ETENDUE_R1_DATASETS`, resolved as
//! `etendue real calibrate` resolves them from the workspace root), this runs calibration-rs's
//! command line (`vcal run --redetect`, the binary in `VCAL`, default
//! `../calibration-rs/target/release/vcal`, built from the tag etendue pins) and etendue's real
//! calibration, both detecting afresh, and compares bit for bit:
//! - etendue's kept export (`result.json`) with vcal's `calibration.json`, every number;
//! - the report's estimate (intrinsics, distortion, `cam_se3_rig`, hand-eye), every residual
//!   (observed, projected, error), the mean, and the σ, with vcal's export;
//! - the quality verdict with vcal's `quality.json`, and the views used with vcal's count.
//!
//! The data is local (DS8's images are not in git), so this is ignored:
//! `cargo test --release -p etendue-cli --test r1 -- --ignored --nocapture`.

use std::path::{Path, PathBuf};
use std::process::Command;

use etendue_cli::calibrate::Report;
use serde_json::Value;
use vision_calibration_core::{DistortionParams, Iso3};
use vision_calibration_pipeline::common::{CalibratedCameras, HandeyeSolution};
use vision_calibration_pipeline::rig_handeye::RigHandeyeExport;
use vision_calibration_pipeline::single_cam_handeye::{
    SingleCamHandeyeExport, SingleCamHandeyeSolution,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Pairs of numbers that must be equal, and where they came from.
#[derive(Default)]
struct Tally {
    compared: usize,
    differ: usize,
    /// The first few differences.
    mismatches: Vec<String>,
    max_abs: f64,
}

impl Tally {
    fn eq(&mut self, what: &str, ours: f64, theirs: f64) {
        self.compared += 1;
        if ours.to_bits() != theirs.to_bits() {
            self.differ += 1;
            self.max_abs = self.max_abs.max((ours - theirs).abs());
            if self.mismatches.len() < 10 {
                self.mismatches.push(format!(
                    "{what}: etendue {ours:e}, calibration-rs {theirs:e}"
                ));
            }
        }
    }

    fn all(&mut self, what: &str, ours: &[f64], theirs: &[f64]) {
        assert_eq!(ours.len(), theirs.len(), "{what}: lengths differ");
        for (k, (a, b)) in ours.iter().zip(theirs).enumerate() {
            self.eq(&format!("{what}[{k}]"), *a, *b);
        }
    }

    fn iso(&mut self, what: &str, ours: &Iso3, theirs: &Iso3) {
        self.all(
            what,
            ours.to_homogeneous().as_slice(),
            theirs.to_homogeneous().as_slice(),
        );
    }

    /// Every number of two JSON documents of the same shape.
    fn json(&mut self, at: &str, ours: &Value, theirs: &Value) {
        match (ours, theirs) {
            (Value::Number(a), Value::Number(b)) => {
                self.eq(at, a.as_f64().unwrap(), b.as_f64().unwrap());
            }
            (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
                for (k, (x, y)) in a.iter().zip(b).enumerate() {
                    self.json(&format!("{at}[{k}]"), x, y);
                }
            }
            (Value::Object(a), Value::Object(b))
                if a.keys().collect::<Vec<_>>() == b.keys().collect::<Vec<_>>() =>
            {
                for (k, x) in a {
                    self.json(&format!("{at}.{k}"), x, &b[k]);
                }
            }
            _ if ours == theirs => {}
            _ => {
                self.differ += 1;
                self.mismatches.push(format!("{at}: the documents differ"));
            }
        }
    }
}

fn read(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// vcal's export, as calibration-rs's own type, and its hand-eye.
fn export_of(v: &Value) -> (Box<dyn CalibratedCameras>, Option<Iso3>, f64) {
    match v["kind"].as_str() {
        Some("single_cam_handeye") => {
            let e: SingleCamHandeyeExport = serde_json::from_value(v.clone()).unwrap();
            let he = match e.handeye {
                SingleCamHandeyeSolution::EyeInHand {
                    gripper_se3_camera, ..
                } => gripper_se3_camera,
                SingleCamHandeyeSolution::EyeToHand {
                    camera_se3_base, ..
                } => camera_se3_base,
            };
            let mean = e.mean_reproj_error;
            (Box::new(e), Some(he), mean)
        }
        Some("rig_handeye") => {
            let e: RigHandeyeExport = serde_json::from_value(v.clone()).unwrap();
            let he = match e.handeye {
                HandeyeSolution::EyeInHand {
                    gripper_se3_rig, ..
                } => gripper_se3_rig,
                HandeyeSolution::EyeToHand { rig_se3_base, .. } => rig_se3_base,
            };
            let mean = e.mean_reproj_error;
            (Box::new(e), Some(he), mean)
        }
        other => panic!("R1 compares hand-eye exports; vcal wrote {other:?}"),
    }
}

/// The report against vcal's export, quality report and views used.
fn compare(report: &Report, vcal: &Value, quality: &Value, used: usize, t: &mut Tally) {
    let (export, handeye, mean) = export_of(vcal);
    let params = export.camera_params();
    let cam_se3_rig = export.cam_se3_rig();
    assert_eq!(report.estimate.cameras.len(), params.len());
    for (i, (cam, p)) in report.estimate.cameras.iter().zip(&params).enumerate() {
        let k = &p.intrinsics;
        t.all(
            &format!("cam{i}.intrinsics"),
            &cam.intrinsics,
            &[k.fx, k.fy, k.cx, k.cy],
        );
        let DistortionParams::BrownConrady5 { params: d } = &p.distortion else {
            panic!("camera {i}: not Brown-Conrady");
        };
        t.all(
            &format!("cam{i}.distortion"),
            &cam.distortion,
            &[d.k1, d.k2, d.k3, d.p1, d.p2],
        );
        t.iso(
            &format!("cam{i}.cam_se3_rig"),
            &cam.cam_se3_ref,
            &cam_se3_rig[i],
        );
    }
    match (&report.estimate.handeye, &handeye) {
        (Some(a), Some(b)) => t.iso("handeye", a, b),
        (a, b) => assert_eq!(a.is_some(), b.is_some(), "hand-eye"),
    }
    // Every residual, in calibration-rs's order.
    let theirs: Vec<_> = export
        .diagnostics()
        .per_feature_residuals
        .target
        .iter()
        .filter(|r| r.projected_px.is_some())
        .collect();
    let r = &report.residuals;
    assert_eq!(r.features.len(), theirs.len(), "residual count");
    for (k, (f, g)) in r.features.iter().zip(&theirs).enumerate() {
        t.all(&format!("observed[{k}]"), &f.observed_px, &g.observed_px);
        t.all(
            &format!("projected[{k}]"),
            &f.projected_px,
            &g.projected_px.unwrap(),
        );
        t.eq(&format!("error[{k}]"), r.errors_px[k], g.error_px.unwrap());
    }
    t.eq("mean_px", r.mean_px, mean);
    // σ: the covariance's diagonal.
    let cov = export
        .parameter_uncertainty()
        .covariance()
        .expect("calibration-rs's covariance");
    let u = report.uncertainty.as_ref().expect("the report's σ");
    assert_eq!(u.labels, cov.labels);
    let sigma: Vec<f64> = (0..cov.labels.len())
        .map(|i| cov.covariance[i][i].max(0.0).sqrt())
        .collect();
    t.all("sigma", &u.sigma, &sigma);
    // The quality report, every field.
    t.json(
        "quality",
        &serde_json::to_value(&report.quality).unwrap(),
        quality,
    );
    let summary = report.real.as_ref().expect("a real report");
    assert_eq!(summary.usable_views, used, "views used");
}

/// `vcal run --redetect` of `manifest` into `out`: the number of views it used.
fn vcal(manifest: &Path, out: &Path) -> usize {
    let bin = std::env::var_os("VCAL").map_or_else(
        || root().join("../calibration-rs/target/release/vcal"),
        PathBuf::from,
    );
    let output = Command::new(&bin)
        .arg("run")
        .arg(manifest)
        .arg("--redetect")
        .arg("-o")
        .arg(out)
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "running {} ({e}): build calibration-rs's CLI at the pinned tag \
                 (`cargo build --release -p vision-calibration-cli`) and set VCAL",
                bin.display()
            )
        });
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.code().is_some_and(|c| c == 0 || c == 3),
        "vcal: {text}"
    );
    // "<topology>: N of M views used"
    text.lines()
        .find_map(|l| {
            let (_, rest) = l.split_once(": ")?;
            rest.strip_suffix(" views used")?
                .split_once(" of ")?
                .0
                .parse()
                .ok()
        })
        .expect("vcal's views used")
}

#[test]
#[ignore = "local data (kuka_1 in calibration-rs's LFS, DS8 downloaded): see docs/measurements/p20a_r1.md"]
fn r1_real_calibration_equals_calibration_rs() {
    let ids = std::env::var("ETENDUE_R1_DATASETS").unwrap_or_else(|_| "kuka_1,DS8".into());
    let scratch = std::env::temp_dir().join(format!("etendue-r1-{}", std::process::id()));
    for id in ids.split(',') {
        let source = etendue_cli::real::resolve(id, &root()).unwrap();
        let out = scratch.join(id);
        let _ = std::fs::remove_dir_all(&out);
        let used = vcal(source.manifest(), &out.join("vcal"));
        let run = etendue_cli::real::calibrate(&source, &out.join("etendue")).unwrap();
        let report = &run.reports[0];
        let mut t = Tally::default();
        t.json(
            "export",
            &report.export,
            &read(&out.join("vcal/calibration.json")),
        );
        compare(
            report,
            &read(&out.join("vcal/calibration.json")),
            &read(&out.join("vcal/quality.json")),
            used,
            &mut t,
        );
        let s = report.real.as_ref().unwrap();
        println!(
            "R1 {id}: {} of {} views, {} points, reprojection mean {:.4} px, rms per axis {:.4} px; \
             {} numbers compared, {} differ, max |Δ| {:e}; verdict {}",
            s.usable_views,
            s.total_views,
            report.residuals.count,
            report.residuals.mean_px,
            report.residuals.rms_axis_px,
            t.compared,
            t.differ,
            t.max_abs,
            report
                .quality
                .as_ref()
                .map_or(Value::Null, |q| serde_json::to_value(q.verdict).unwrap()),
        );
        assert!(t.differ == 0, "R1 {id}: {:#?}", t.mismatches);
    }
    let _ = std::fs::remove_dir_all(&scratch);
}
