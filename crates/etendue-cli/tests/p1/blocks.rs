//! Where the halves' system difference comes from, block by block (P20c2): the system difference
//! the halves would have if they differed in one block of the cell only (each camera's intrinsics
//! and distortion, the rig, the hand-eye's rotation, its translation), against the planner's
//! prediction from that block's covariance alone. The blocks' differences need not add up to the
//! whole: where they compensate each other, the whole is smaller.

use etendue_cli::calibrate::cell;
use etendue_cli::real::SplitCalibrations;
use etendue_synth::dataset::GroundTruth;
use etendue_synth::plan::{EstimatedCell, NoiseModel, predict, system_difference};
use nalgebra::Isometry3;

/// A block: its name, and whether a planner label is in it.
type Block = (&'static str, fn(&str) -> bool);

/// The blocks, by the planner's labels.
const BLOCKS: [Block; 4] = [
    ("intrinsics", |l| l.starts_with("cam") && !l.contains("se3")),
    ("rig", |l| l.contains("cam_se3_rig")),
    ("hand-eye rotation", |l| {
        l.starts_with("gripper_se3_rig.r") || l.starts_with("rig_se3_base.r")
    }),
    ("hand-eye translation", |l| {
        l.starts_with("gripper_se3_rig.t") || l.starts_with("rig_se3_base.t")
    }),
];

/// `a` with block `b` of `other`.
fn with_block(a: &EstimatedCell, other: &EstimatedCell, block: usize) -> EstimatedCell {
    let mut c = a.clone();
    for (i, (params, cam_se3_ref)) in c.cameras.iter_mut().enumerate() {
        match block {
            0 => *params = other.cameras[i].0.clone(),
            1 => *cam_se3_ref = other.cameras[i].1,
            _ => {}
        }
    }
    match block {
        2 => c.handeye = Isometry3::from_parts(a.handeye.translation, other.handeye.rotation),
        3 => c.handeye = Isometry3::from_parts(other.handeye.translation, a.handeye.rotation),
        _ => {}
    }
    c
}

/// Prints the measured and predicted RMS (over the splits) of each block's system difference and
/// of the whole, for each of `models`.
pub fn print(gt: &GroundTruth, cal: &SplitCalibrations, models: &[(&str, NoiseModel)]) {
    let n = BLOCKS.len();
    let rms = |v: &[f64]| -> Vec<String> {
        v.iter()
            .map(|x| format!("{:.3}", (x / cal.splits.len() as f64).sqrt()))
            .collect()
    };
    let mut measured = vec![0.0; n + 1];
    for (_, ra, rb) in &cal.splits {
        let (a, b) = (cell(gt, ra).unwrap(), cell(gt, rb).unwrap());
        for (k, m) in measured.iter_mut().take(n).enumerate() {
            *m += system_difference(gt, &a, &with_block(&a, &b, k))
                .unwrap()
                .powi(2);
        }
        measured[n] += system_difference(gt, &a, &b).unwrap().powi(2);
    }
    let names: Vec<&str> = BLOCKS.iter().map(|b| b.0).chain(["all"]).collect();
    println!(
        "Halves' system difference by block (mm, RMS over the splits):\n\n| | {} |\n|---{}|",
        names.join(" | "),
        "|---".repeat(names.len())
    );
    println!("| measured | {} |", rms(&measured).join(" | "));
    for (name, model) in models {
        let p = predict(gt, model).unwrap();
        let labels = p.labels().to_vec();
        let mut predicted = vec![0.0; n + 1];
        for (split, ..) in &cal.splits {
            let cov = p.covariance(&split.a).unwrap() + p.covariance(&split.b).unwrap();
            for (k, (_, member)) in BLOCKS.iter().enumerate() {
                let mut c = cov.clone();
                for x in 0..labels.len() {
                    for y in 0..labels.len() {
                        if !(member(&labels[x]) && member(&labels[y])) {
                            c[(x, y)] = 0.0;
                        }
                    }
                }
                predicted[k] += p.system_sigma_of(&labels, &c).powi(2);
            }
            predicted[n] += p.system_sigma_of(&labels, &cov).powi(2);
        }
        println!("| {name} | {} |", rms(&predicted).join(" | "));
    }
    println!();
}
