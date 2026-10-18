//! Exports of a baked scenario for a robot controller or a spreadsheet: the
//! tool pose and joint configuration at every capture, and the timed joint
//! trajectory.
//!
//! Frame composition only: `base_se3_tool = world_se3_base⁻¹ · world_se3_tool`
//! from the baked frames. Units are SI: metres, radians, seconds.

use std::fmt::Write as _;

use nalgebra::Isometry3;
use serde::{Deserialize, Serialize};

use crate::BakedScenario;

/// The pose sequence of one robot: its tool pose and joints at every
/// capture, version 1.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CaptureExport {
    /// Format version; `1`.
    pub version: u32,
    /// Robot id.
    pub robot: String,
    /// The frame tool poses are given in (the manifest's `base_link`).
    pub base_link: String,
    /// The tool frame (the manifest's `tcp_link`).
    pub tcp_link: String,
    /// Joint names, in `q` order.
    pub joint_names: Vec<String>,
    /// One entry per capture, in time order.
    pub captures: Vec<CaptureRow>,
}

/// One capture.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CaptureRow {
    /// Capture id.
    pub capture: String,
    /// Time, seconds.
    pub t: f64,
    /// Tool pose in the robot's base frame.
    #[cfg_attr(
        feature = "schemars",
        schemars(with = "vision_calibration_core::Iso3Schema")
    )]
    pub base_se3_tool: Isometry3<f64>,
    /// Joint positions (rad or m).
    pub q: Vec<f64>,
}

/// Why an export is not possible.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ExportError {
    /// The baked scenario has no robot of that id.
    #[error("no robot `{0}` in the baked scenario")]
    UnknownRobot(String),
    /// A frame the export needs is not baked.
    #[error("frame `{0}` is not in the baked scenario")]
    UnknownFrame(String),
    /// The trajectory needs a bake with derivatives.
    #[error("the bake has no joint velocities and accelerations (bake with derivatives)")]
    NoDerivatives,
}

/// `v` without a negative zero (`-0` reads as a sign error in a table).
fn num(v: f64) -> f64 {
    v + 0.0
}

fn frame(baked: &BakedScenario, name: &str) -> Result<usize, ExportError> {
    baked
        .frames
        .iter()
        .position(|f| f == name)
        .ok_or_else(|| ExportError::UnknownFrame(name.to_owned()))
}

fn robot(baked: &BakedScenario, id: &str) -> Result<usize, ExportError> {
    baked
        .robots
        .iter()
        .position(|r| r.id == id)
        .ok_or_else(|| ExportError::UnknownRobot(id.to_owned()))
}

/// The tool pose and joints of robot `id` at every capture of `baked`.
/// `base_link` and `tcp_link` are the robot manifest's.
///
/// # Errors
///
/// [`ExportError`] if the robot or its frames are not baked.
pub fn captures(
    baked: &BakedScenario,
    id: &str,
    base_link: &str,
    tcp_link: &str,
) -> Result<CaptureExport, ExportError> {
    let r = robot(baked, id)?;
    let base = frame(baked, &format!("{id}/{base_link}"))?;
    let tool = frame(baked, &format!("{id}/{tcp_link}"))?;
    let captures = baked
        .samples
        .iter()
        .filter_map(|s| {
            s.capture.as_ref().map(|c| CaptureRow {
                capture: c.id.clone(),
                t: s.t,
                base_se3_tool: s.world_se3_frame[base].inverse() * s.world_se3_frame[tool],
                q: s.joint_positions[r].clone(),
            })
        })
        .collect();
    Ok(CaptureExport {
        version: 1,
        robot: id.to_owned(),
        base_link: base_link.to_owned(),
        tcp_link: tcp_link.to_owned(),
        joint_names: baked.robots[r].joint_names.clone(),
        captures,
    })
}

impl CaptureExport {
    /// As CSV, one row per capture: `capture, t, x, y, z` (m), the rotation
    /// as a unit quaternion `qx, qy, qz, qw` and as a rotation vector
    /// `rx, ry, rz` (axis × angle, rad), then one column per joint.
    #[must_use]
    pub fn to_csv(&self) -> String {
        let mut out = String::from("capture,t,x,y,z,qx,qy,qz,qw,rx,ry,rz");
        for name in &self.joint_names {
            let _ = write!(out, ",{name}");
        }
        out.push('\n');
        for row in &self.captures {
            let t = row.base_se3_tool.translation.vector;
            let q = row.base_se3_tool.rotation.quaternion().coords;
            let r = row.base_se3_tool.rotation.scaled_axis();
            let _ = write!(out, "{},{}", row.capture, row.t);
            for v in [t.x, t.y, t.z, q.x, q.y, q.z, q.w, r.x, r.y, r.z]
                .into_iter()
                .chain(row.q.iter().copied())
            {
                let _ = write!(out, ",{}", num(v));
            }
            out.push('\n');
        }
        out
    }
}

/// Robot `id`'s timed joint trajectory as CSV: one row per sample, `t`, the
/// capture id on capture samples, then `q_<joint>`, `qd_<joint>` and
/// `qdd_<joint>` for every joint.
///
/// # Errors
///
/// [`ExportError`] if the robot is not baked or the bake has no derivatives.
pub fn trajectory_csv(baked: &BakedScenario, id: &str) -> Result<String, ExportError> {
    let r = robot(baked, id)?;
    let names = &baked.robots[r].joint_names;
    let mut out = String::from("t,capture");
    for prefix in ["q", "qd", "qdd"] {
        for name in names {
            let _ = write!(out, ",{prefix}_{name}");
        }
    }
    out.push('\n');
    for s in &baked.samples {
        let (Some(qd), Some(qdd)) = (&s.joint_velocities, &s.joint_accelerations) else {
            return Err(ExportError::NoDerivatives);
        };
        let _ = write!(
            out,
            "{},{}",
            s.t,
            s.capture.as_ref().map_or("", |c| c.id.as_str())
        );
        for v in s.joint_positions[r].iter().chain(&qd[r]).chain(&qdd[r]) {
            let _ = write!(out, ",{}", num(*v));
        }
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BakedRobot, BakedSample, CaptureEvent};

    fn baked(derivatives: bool) -> BakedScenario {
        let base = Isometry3::translation(1.0, 0.0, 0.0);
        let tool = |x: f64| base * Isometry3::translation(x, 0.2, 0.3);
        let sample = |t: f64, x: f64, capture: Option<&str>| BakedSample {
            t,
            world_se3_frame: vec![Isometry3::identity(), base, tool(x)],
            joint_positions: vec![vec![x, -0.0]],
            joint_velocities: derivatives.then(|| vec![vec![0.5, 0.0]]),
            joint_accelerations: derivatives.then(|| vec![vec![0.0, 0.0]]),
            capture: capture.map(|id| CaptureEvent { id: id.into() }),
        };
        BakedScenario {
            version: 1,
            dt: 0.5,
            frames: vec!["world".into(), "arm/base".into(), "arm/tool0".into()],
            robots: vec![BakedRobot {
                id: "arm".into(),
                joint_names: vec!["j1".into(), "j2".into()],
            }],
            samples: vec![sample(0.0, 0.0, None), sample(0.5, 0.1, Some("a"))],
        }
    }

    #[test]
    fn captures_are_tool_poses_in_the_base_frame() {
        let rows = captures(&baked(false), "arm", "base", "tool0").unwrap();
        assert_eq!(rows.captures.len(), 1);
        let row = &rows.captures[0];
        assert_eq!(row.capture, "a");
        let t = row.base_se3_tool.translation.vector;
        assert!((t.x - 0.1).abs() < 1e-15 && (t.y - 0.2).abs() < 1e-15);
        let csv = rows.to_csv();
        let mut lines = csv.lines();
        assert_eq!(
            lines.next(),
            Some("capture,t,x,y,z,qx,qy,qz,qw,rx,ry,rz,j1,j2")
        );
        let cells: Vec<&str> = lines.next().unwrap().split(',').collect();
        assert_eq!(cells.len(), 14);
        assert_eq!(cells[13], "0", "no negative zero");
        assert!(matches!(
            captures(&baked(false), "arm", "base", "flange"),
            Err(ExportError::UnknownFrame(_))
        ));
    }

    #[test]
    fn the_trajectory_needs_derivatives() {
        assert_eq!(
            trajectory_csv(&baked(false), "arm"),
            Err(ExportError::NoDerivatives)
        );
        let csv = trajectory_csv(&baked(true), "arm").unwrap();
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines[0], "t,capture,q_j1,q_j2,qd_j1,qd_j2,qdd_j1,qdd_j2");
        assert_eq!(lines[2], "0.5,a,0.1,0,0.5,0,0,0");
        assert!(matches!(
            trajectory_csv(&baked(true), "x"),
            Err(ExportError::UnknownRobot(_))
        ));
    }
}
