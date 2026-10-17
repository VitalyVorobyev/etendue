//! Tool poses from a file, for a stop-and-shoot scenario
//! (`etendue scenario from-poses`).
//!
//! A poses file is a JSON array; each entry is `base_se3_tool` (the robot's
//! TCP link in its base frame, metres) in one of two forms:
//! - the SE3 wire format, `{"rotation": [qx, qy, qz, qw], "translation": [x, y, z]}`;
//! - a row of `robot_poses.json` as `etendue gt` writes it,
//!   `{"capture"?: id, "tx", "ty", "tz", "qx", "qy", "qz", "qw"}`, whose
//!   `capture` names the capture.

use anyhow::{Context, Result, bail};
use etendue_scene::ScenarioSpec;
use nalgebra::{Isometry3, Quaternion, Translation3, UnitQuaternion};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(untagged)]
enum Entry {
    /// The SE3 wire format, read as plain arrays so its quaternion is checked
    /// and normalised like a row's.
    Wire {
        rotation: [f64; 4],
        translation: [f64; 3],
    },
    Row {
        #[serde(default)]
        capture: Option<String>,
        tx: f64,
        ty: f64,
        tz: f64,
        qx: f64,
        qy: f64,
        qz: f64,
        qw: f64,
    },
}

/// Parse a poses file: `(capture id, base_se3_tool)` per entry.
///
/// # Errors
///
/// If the text is not a JSON array of poses in either form, is empty, or has
/// a non-finite value or a zero quaternion.
pub fn parse(text: &str) -> Result<Vec<(Option<String>, Isometry3<f64>)>> {
    let entries: Vec<Entry> = serde_json::from_str(text)
        .context("expected a JSON array of {rotation, translation} poses or robot_poses rows")?;
    if entries.is_empty() {
        bail!("no poses");
    }
    entries
        .into_iter()
        .enumerate()
        .map(|(i, e)| {
            let (id, [qx, qy, qz, qw], [tx, ty, tz]) = match e {
                Entry::Wire {
                    rotation,
                    translation,
                } => (None, rotation, translation),
                Entry::Row {
                    capture,
                    tx,
                    ty,
                    tz,
                    qx,
                    qy,
                    qz,
                    qw,
                } => (capture, [qx, qy, qz, qw], [tx, ty, tz]),
            };
            if ![qx, qy, qz, qw, tx, ty, tz].iter().all(|v| v.is_finite()) {
                bail!("pose {i}: non-finite value");
            }
            let q = Quaternion::new(qw, qx, qy, qz);
            if q.norm() < 1e-9 {
                bail!("pose {i}: zero quaternion");
            }
            let pose = Isometry3::from_parts(
                Translation3::new(tx, ty, tz),
                UnitQuaternion::from_quaternion(q),
            );
            Ok((id, pose))
        })
        .collect()
}

/// A stop-and-shoot scenario for `robot` from a poses file's text.
///
/// # Errors
///
/// As [`parse`], and if `speed_scale` is not in `(0, 1]` or `dt` is not
/// positive (what `ScenarioSpec::validate` requires, checked here so a file
/// written without a scene is valid too).
pub fn scenario(text: &str, robot: &str, speed_scale: f64, dt: f64) -> Result<ScenarioSpec> {
    if !(speed_scale.is_finite() && speed_scale > 0.0 && speed_scale <= 1.0) {
        bail!("speed scale must be in (0, 1], got {speed_scale}");
    }
    if !(dt.is_finite() && dt > 0.0) {
        bail!("sample period must be positive, got {dt}");
    }
    Ok(ScenarioSpec::from_poses(
        robot,
        &parse(text)?,
        speed_scale,
        dt,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_both_forms() {
        let wire = r#"[{"rotation": [0, 0, 0, 1], "translation": [0.4, 0.1, 0.5]}]"#;
        let poses = parse(wire).unwrap();
        assert_eq!(poses[0].0, None);
        assert_eq!(poses[0].1.translation.vector.x, 0.4);

        // A robot_poses.json row: an unnormalised quaternion is normalised.
        let rows =
            r#"[{"capture": "c7", "tx": 1, "ty": 2, "tz": 3, "qx": 0, "qy": 0, "qz": 0, "qw": 2}]"#;
        let poses = parse(rows).unwrap();
        assert_eq!(poses[0].0.as_deref(), Some("c7"));
        assert!((poses[0].1.rotation.angle()).abs() < 1e-15);

        let spec = scenario(rows, "ur5e", 0.5, 0.01).unwrap();
        assert_eq!(spec.steps.len(), 2);
    }

    #[test]
    fn rejects_bad_input() {
        assert!(parse("[]").is_err());
        assert!(parse(r#"{"not": "an array"}"#).is_err());
        let zero = r#"[{"tx": 1, "ty": 2, "tz": 3, "qx": 0, "qy": 0, "qz": 0, "qw": 0}]"#;
        assert!(parse(zero).is_err());
        let zero_wire = r#"[{"rotation": [0, 0, 0, 0], "translation": [0, 0, 0]}]"#;
        assert!(parse(zero_wire).is_err());
        let one = r#"[{"rotation": [0, 0, 0, 1], "translation": [0, 0, 0]}]"#;
        assert!(scenario(one, "ur5e", 0.0, 0.01).is_err());
        assert!(scenario(one, "ur5e", 3.0, 0.01).is_err());
        assert!(scenario(one, "ur5e", 0.5, 0.0).is_err());
    }

    #[test]
    fn normalises_wire_quaternions() {
        let wire = r#"[{"rotation": [0, 0, 0, 2], "translation": [0, 0, 0]}]"#;
        let q = parse(wire).unwrap()[0].1.rotation;
        assert!((q.coords.norm() - 1.0).abs() < 1e-15);
    }
}
