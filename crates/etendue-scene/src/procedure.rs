//! Calibration procedures (ADR 0011): an ordered list of stages, each one calibration-rs workflow
//! with what it holds fixed, what it frees, how it solves and what it starts from.
//!
//! A procedure is a library item (`library/procedures/<id>.json`), so the bank, the built-ins and a
//! project all hold them. The names of parameters (`fx`, `k3`, `tilt_y`) and of robust losses are
//! calibration-rs's own; etendue passes them through, and calibration-rs checks them when a stage
//! runs.
//!
//! ```json
//! { "stages": [
//!     { "id": "intrinsics", "problem": "planar_intrinsics" },
//!     { "id": "handeye", "problem": "single_cam_handeye",
//!       "seed": { "intrinsics": { "stage": "intrinsics" } },
//!       "free": { "refine_robot_poses": true } } ] }
//! ```

use serde::{Deserialize, Serialize};

use crate::validate::Issues;

/// A calibration-rs workflow (`vision-calibration-pipeline`'s problem modules).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Problem {
    /// One camera's intrinsics from planar views, no robot.
    PlanarIntrinsics,
    /// A rig's cameras and extrinsics from planar views, no robot.
    RigExtrinsics,
    /// One camera and its hand-eye.
    SingleCamHandeye,
    /// A rig and its hand-eye.
    RigHandeye,
    /// One camera with its laser (a triangulation head): intrinsics, sensor tilt, plane; the
    /// hand-eye solved on the same views.
    LaserlineDevice,
    /// A rig with lasers and its hand-eye, jointly.
    RigHandeyeLaserline,
}

impl Problem {
    /// Whether the workflow solves a hand-eye (needs robot poses).
    #[must_use]
    pub fn handeye(self) -> bool {
        !matches!(self, Self::PlanarIntrinsics | Self::RigExtrinsics)
    }

    /// Whether it calibrates several cameras at once.
    #[must_use]
    pub fn rig(self) -> bool {
        matches!(
            self,
            Self::RigExtrinsics | Self::RigHandeye | Self::RigHandeyeLaserline
        )
    }

    /// Whether it needs lasers.
    #[must_use]
    pub fn laser(self) -> bool {
        matches!(self, Self::LaserlineDevice | Self::RigHandeyeLaserline)
    }
}

/// The sensor model a stage calibrates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum SensorChoice {
    /// What the design has: Scheimpflug when a camera's sensor tilts, identity otherwise.
    #[default]
    Design,
    /// An untilted sensor.
    Identity,
    /// A tilted (Scheimpflug) sensor.
    Scheimpflug,
}

/// The parameters held fixed in the solve, by calibration-rs's names per slot. A slot left out
/// keeps calibration-rs's default for the model.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct FixLists {
    /// `fx`, `fy`, `cx`, `cy`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intrinsics: Option<Vec<String>>,
    /// `k1`, `k2`, `k3`, `p1`, `p2`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distortion: Option<Vec<String>>,
    /// `tilt_x`, `tilt_y`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sensor: Option<Vec<String>>,
}

impl FixLists {
    fn is_empty(&self) -> bool {
        self.intrinsics.is_none() && self.distortion.is_none() && self.sensor.is_none()
    }
}

/// The camera model of a stage and what of it stays fixed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CameraChoice {
    /// The sensor model.
    #[serde(default)]
    pub sensor: SensorChoice,
    /// Fixed parameters.
    #[serde(default, skip_serializing_if = "FixLists::is_empty")]
    pub fix: FixLists,
}

/// What else a stage estimates or holds, beyond the camera. Each is calibration-rs's option of
/// the same meaning; one that does not apply to the stage's workflow is refused. Absent, the
/// workflow's default holds.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Freedoms {
    /// Refine the intrinsics jointly with the rig and the hand-eye (rig workflows).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refine_intrinsics: Option<bool>,
    /// Refine a Scheimpflug sensor's tilt jointly with the rig (rig workflows).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refine_sensor: Option<bool>,
    /// Refine the cameras' poses in the rig during the hand-eye solve (`rig_handeye`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refine_rig: Option<bool>,
    /// Refine each view's robot pose under a prior (hand-eye workflows).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refine_robot_poses: Option<bool>,
    /// The robot-pose prior's rotation σ, radians.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub robot_rot_sigma_rad: Option<f64>,
    /// The robot-pose prior's translation σ, metres.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub robot_trans_sigma_m: Option<f64>,
    /// The least robot rotation between views the linear hand-eye uses, degrees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_motion_angle_deg: Option<f64>,
    /// Hold the hand-eye in the joint laser solve (`rig_handeye_laserline`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix_handeye: Option<bool>,
    /// Hold the target's pose in the joint laser solve (`rig_handeye_laserline`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix_target: Option<bool>,
    /// Hold the laser plane (`laserline_device`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix_plane: Option<bool>,
    /// The target poses (view indices) held at their linear estimate (`planar_intrinsics`,
    /// `laserline_device`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix_poses: Option<Vec<usize>>,
}

impl Freedoms {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// A robust loss, as calibration-rs names it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Loss {
    /// Least squares.
    None,
    /// Huber, with its scale (pixels for reprojection).
    Huber {
        /// Where the loss turns linear.
        scale: f64,
    },
    /// Cauchy, with its scale.
    Cauchy {
        /// The scale.
        scale: f64,
    },
    /// Arctan, with its scale.
    Arctan {
        /// The scale.
        scale: f64,
    },
}

/// How a stage solves. Absent fields keep the workflow's defaults.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct SolverChoice {
    /// Iteration limit of the non-linear solve.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_iters: Option<usize>,
    /// The robust loss on the reprojection residuals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub robust_loss: Option<Loss>,
}

/// Where a stage's starting value of an estimate comes from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Seed {
    /// The design, as a user would enter it from datasheets: the focal length rounded to 100 px,
    /// the principal point at the image centre, no distortion, the tilt about x rounded to a
    /// whole degree.
    Nominal,
    /// The truth (synthetic datasets only; the report says so).
    Truth,
    /// An earlier stage's estimate, by the stage's id.
    Stage(String),
}

/// What a stage starts from. Absent, the workflow's own linear initialisation.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Seeds {
    /// Focal lengths and principal points (and with them the distortion).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intrinsics: Option<Seed>,
    /// Scheimpflug tilts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sensor: Option<Seed>,
    /// The cameras' poses in the rig: from an earlier rig stage only (calibration-rs takes the
    /// target's pose per view with them).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rig: Option<Seed>,
    /// The hand-eye transform.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handeye: Option<Seed>,
}

impl Seeds {
    fn all(&self) -> impl Iterator<Item = (&'static str, &Seed)> {
        [
            ("intrinsics", &self.intrinsics),
            ("sensor", &self.sensor),
            ("rig", &self.rig),
            ("handeye", &self.handeye),
        ]
        .into_iter()
        .filter_map(|(k, s)| s.as_ref().map(|s| (k, s)))
    }

    fn is_empty(&self) -> bool {
        self.all().next().is_none()
    }
}

/// One stage of a procedure.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Stage {
    /// Its id, unique in the procedure; later stages seed from it by this id.
    pub id: String,
    /// The workflow.
    pub problem: Problem,
    /// The camera a single-camera stage calibrates, by scene id; absent, the dataset's only
    /// camera (a stage per camera repeats with each id). Rig stages take every camera.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera: Option<String>,
    /// The camera model and its fixed parameters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<CameraChoice>,
    /// Further estimates freed or held.
    #[serde(default, skip_serializing_if = "Freedoms::is_default")]
    pub free: Freedoms,
    /// How it solves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solver: Option<SolverChoice>,
    /// What it starts from.
    #[serde(default, skip_serializing_if = "Seeds::is_empty")]
    pub seed: Seeds,
}

/// A calibration procedure: stages in order. The last stage's result is the procedure's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ProcedurePart {
    /// The stages, run in order.
    pub stages: Vec<Stage>,
}

impl ProcedurePart {
    /// Structural checks: ids, seeds naming earlier stages, freedoms that apply to their workflow.
    pub(crate) fn check(&self, issues: &mut Issues, path: &str) {
        if self.stages.is_empty() {
            issues.push(format!("{path}.stages"), "needs at least one stage");
        }
        let mut seen: Vec<&str> = Vec::new();
        for (i, s) in self.stages.iter().enumerate() {
            let at = format!("{path}.stages[{i}]");
            if !crate::frame::is_valid_id(&s.id) {
                issues.push(format!("{at}.id"), format!("`{}` is not a valid id", s.id));
            } else if seen.contains(&s.id.as_str()) {
                issues.push(format!("{at}.id"), format!("`{}` is used twice", s.id));
            }
            if s.camera.is_some() && s.problem.rig() {
                issues.push(
                    format!("{at}.camera"),
                    "a rig stage calibrates every camera",
                );
            }
            for (key, seed) in s.seed.all() {
                if let Seed::Stage(id) = seed
                    && !seen.contains(&id.as_str())
                {
                    issues.push(
                        format!("{at}.seed.{key}"),
                        format!("`{id}` is not an earlier stage"),
                    );
                }
            }
            if !s.problem.rig() && s.seed.rig.is_some() {
                issues.push(format!("{at}.seed.rig"), "only a rig stage has a rig");
            }
            if matches!(s.seed.rig, Some(Seed::Nominal | Seed::Truth)) {
                issues.push(
                    format!("{at}.seed.rig"),
                    "a rig seeds only from an earlier stage (its per-view target poses come with it)",
                );
            }
            if !s.problem.handeye() && s.seed.handeye.is_some() {
                issues.push(
                    format!("{at}.seed.handeye"),
                    "this workflow has no hand-eye",
                );
            }
            let f = &s.free;
            let only = |set: bool, ok: bool, name: &str, issues: &mut Issues| {
                if set && !ok {
                    issues.push(
                        format!("{at}.free.{name}"),
                        format!("does not apply to `{}`", problem_name(s.problem)),
                    );
                }
            };
            let p = s.problem;
            only(
                f.refine_intrinsics.is_some(),
                p.rig() && p.handeye(),
                "refine_intrinsics",
                issues,
            );
            only(f.refine_sensor.is_some(), p.rig(), "refine_sensor", issues);
            only(
                f.refine_rig.is_some(),
                p == Problem::RigHandeye,
                "refine_rig",
                issues,
            );
            only(
                f.refine_robot_poses.is_some()
                    || f.robot_rot_sigma_rad.is_some()
                    || f.robot_trans_sigma_m.is_some(),
                p.handeye() && p != Problem::LaserlineDevice,
                "refine_robot_poses",
                issues,
            );
            only(
                f.min_motion_angle_deg.is_some(),
                p.handeye() && p != Problem::LaserlineDevice,
                "min_motion_angle_deg",
                issues,
            );
            only(
                f.fix_handeye.is_some(),
                p == Problem::RigHandeyeLaserline,
                "fix_handeye",
                issues,
            );
            only(
                f.fix_target.is_some(),
                p == Problem::RigHandeyeLaserline,
                "fix_target",
                issues,
            );
            only(
                f.fix_plane.is_some(),
                p == Problem::LaserlineDevice,
                "fix_plane",
                issues,
            );
            only(
                f.fix_poses.is_some(),
                matches!(p, Problem::PlanarIntrinsics | Problem::LaserlineDevice),
                "fix_poses",
                issues,
            );
            for (name, v) in [
                ("robot_rot_sigma_rad", f.robot_rot_sigma_rad),
                ("robot_trans_sigma_m", f.robot_trans_sigma_m),
            ] {
                if let Some(v) = v {
                    issues.check_positive(&format!("{at}.free.{name}"), v);
                }
            }
            if let Some(SolverChoice {
                max_iters: Some(0), ..
            }) = s.solver
            {
                issues.push(format!("{at}.solver.max_iters"), "must be positive");
            }
            if let Some(SolverChoice {
                robust_loss:
                    Some(Loss::Huber { scale } | Loss::Cauchy { scale } | Loss::Arctan { scale }),
                ..
            }) = s.solver
            {
                issues.check_positive(&format!("{at}.solver.robust_loss.scale"), scale);
            }
            seen.push(&s.id);
        }
    }
}

/// The workflow's name, as in documents.
#[must_use]
pub fn problem_name(p: Problem) -> &'static str {
    match p {
        Problem::PlanarIntrinsics => "planar_intrinsics",
        Problem::RigExtrinsics => "rig_extrinsics",
        Problem::SingleCamHandeye => "single_cam_handeye",
        Problem::RigHandeye => "rig_handeye",
        Problem::LaserlineDevice => "laserline_device",
        Problem::RigHandeyeLaserline => "rig_handeye_laserline",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(text: &str) -> Vec<String> {
        let p: ProcedurePart = serde_json::from_str(text).unwrap();
        let mut issues = Issues::default();
        p.check(&mut issues, "procedure");
        issues
            .finish()
            .err()
            .map(|e| e.issues.into_iter().map(|i| i.path).collect())
            .unwrap_or_default()
    }

    #[test]
    fn a_staged_procedure_checks() {
        let ok = r#"{ "stages": [
            { "id": "intrinsics", "problem": "planar_intrinsics", "model": { "fix": { "distortion": ["k3"] } } },
            { "id": "handeye", "problem": "single_cam_handeye",
              "seed": { "intrinsics": { "stage": "intrinsics" } },
              "free": { "refine_robot_poses": true, "robot_rot_sigma_rad": 0.01 },
              "solver": { "max_iters": 100, "robust_loss": { "kind": "huber", "scale": 1.0 } } } ] }"#;
        assert!(check(ok).is_empty(), "{:?}", check(ok));
    }

    #[test]
    fn seeds_name_earlier_stages_and_freedoms_fit_their_workflow() {
        let bad = r#"{ "stages": [
            { "id": "a", "problem": "single_cam_handeye", "seed": { "intrinsics": { "stage": "b" } },
              "free": { "fix_plane": true, "refine_rig": false } },
            { "id": "a", "problem": "rig_handeye", "camera": "cam_left", "seed": { "handeye": "nominal" } },
            { "id": "c", "problem": "planar_intrinsics", "seed": { "handeye": "truth" },
              "solver": { "max_iters": 0 } } ] }"#;
        assert_eq!(
            check(bad),
            [
                "procedure.stages[0].seed.intrinsics",
                "procedure.stages[0].free.refine_rig",
                "procedure.stages[0].free.fix_plane",
                "procedure.stages[1].id",
                "procedure.stages[1].camera",
                "procedure.stages[2].seed.handeye",
                "procedure.stages[2].solver.max_iters",
            ]
        );
        assert!(!check(r#"{ "stages": [] }"#).is_empty());
        assert!(
            serde_json::from_str::<ProcedurePart>(
                r#"{ "stages": [ { "id": "a", "problem": "x" } ] }"#
            )
            .is_err()
        );
    }
}
