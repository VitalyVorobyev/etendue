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

    /// Every workflow, in the order a procedure usually runs them.
    pub const ALL: [Self; 6] = [
        Self::PlanarIntrinsics,
        Self::RigExtrinsics,
        Self::SingleCamHandeye,
        Self::RigHandeye,
        Self::LaserlineDevice,
        Self::RigHandeyeLaserline,
    ];

    /// Whether a stage of this workflow takes the option `key` (a [`FREEDOM_KEYS`] name). The one
    /// statement of which option applies where: validation, the CLI and the studio all read it.
    #[must_use]
    pub fn takes(self, key: &str) -> bool {
        let robot_prior = self.handeye() && self != Self::LaserlineDevice;
        match key {
            "refine_intrinsics" | "refine_sensor" => self.rig(),
            "refine_rig" => self == Self::RigHandeye,
            "refine_robot_poses"
            | "robot_rot_sigma_rad"
            | "robot_trans_sigma_m"
            | "min_motion_angle_deg" => robot_prior,
            "fix_handeye" | "fix_target" => self == Self::RigHandeyeLaserline,
            "fix_plane" => self == Self::LaserlineDevice,
            "fix_poses" => matches!(self, Self::PlanarIntrinsics | Self::LaserlineDevice),
            _ => false,
        }
    }

    /// The seed slots ([`SEED_SLOTS`] names) a stage of this workflow has; `sensor` only where the
    /// stage calibrates a tilt.
    #[must_use]
    pub fn seed_slots(self, tilted: bool) -> Vec<&'static str> {
        SEED_SLOTS
            .into_iter()
            .filter(|slot| match *slot {
                "sensor" => tilted,
                "rig" => self.rig(),
                // The joint laser solve takes its hand-eye with its rig: from a `rig_handeye`
                // stage's result (calibration-rs's `JointManualInit`).
                "handeye" => self.handeye() && self != Self::RigHandeyeLaserline,
                "planes" => self.laser(),
                _ => true,
            })
            .collect()
    }

    /// The workflow that calibrates a dataset of `cameras` cameras, with or without lasers, in
    /// one stage: `auto`'s choice by the dataset's shape.
    #[must_use]
    pub fn for_dataset(cameras: usize, lasers: bool) -> Self {
        match (cameras > 1, lasers) {
            (false, false) => Self::SingleCamHandeye,
            (true, false) => Self::RigHandeye,
            (false, true) => Self::LaserlineDevice,
            (true, true) => Self::RigHandeyeLaserline,
        }
    }
}

/// A stage's options ([`Freedoms`] fields), by their document names, in their order.
pub const FREEDOM_KEYS: [&str; 11] = [
    "refine_intrinsics",
    "refine_sensor",
    "refine_rig",
    "refine_robot_poses",
    "robot_rot_sigma_rad",
    "robot_trans_sigma_m",
    "min_motion_angle_deg",
    "fix_handeye",
    "fix_target",
    "fix_plane",
    "fix_poses",
];

/// A stage's seed slots ([`Seeds`] fields), by their document names, in their order.
pub const SEED_SLOTS: [&str; 5] = ["intrinsics", "sensor", "rig", "handeye", "planes"];

/// The kinds of [`Seed`] slot `slot` takes (`nominal`, `truth`, `stage`): a rig only from an
/// earlier stage, since calibration-rs takes the target's pose per view with it.
#[must_use]
pub fn seed_kinds(slot: &str) -> &'static [&'static str] {
    if slot == "rig" {
        &["stage"]
    } else {
        &["nominal", "truth", "stage"]
    }
}

/// The rules of procedures, as one document: what an editor needs to offer only what a stage
/// takes ([`rules`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ProcedureRules {
    /// Per workflow, in [`Problem::ALL`]'s order.
    pub problems: Vec<ProblemRules>,
    /// Per seed slot, the kinds of seed it takes (`nominal`, `truth`, `stage`).
    pub seed_kinds: Vec<SeedSlotRules>,
    /// The workflow that calibrates a dataset of each shape in one stage
    /// ([`Problem::for_dataset`]).
    pub by_shape: Vec<ShapeRule>,
}

/// The one-stage workflow of a dataset's shape.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ShapeRule {
    /// More than one camera.
    pub rig: bool,
    /// With lasers.
    pub lasers: bool,
    pub problem: Problem,
}

/// One workflow's rules.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ProblemRules {
    pub problem: Problem,
    /// Calibrates several cameras at once.
    pub rig: bool,
    /// Solves a hand-eye (needs robot poses).
    pub handeye: bool,
    /// Needs lasers.
    pub laser: bool,
    /// The options ([`Freedoms`] fields) it takes.
    pub freedoms: Vec<String>,
    /// Its seed slots for an untilted camera model, and for a tilted one.
    pub seed_slots: Vec<String>,
    pub seed_slots_tilted: Vec<String>,
}

/// One seed slot's kinds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct SeedSlotRules {
    pub slot: String,
    pub kinds: Vec<String>,
}

/// The rules of procedures ([`ProcedureRules`]).
#[must_use]
pub fn rules() -> ProcedureRules {
    let names = |v: Vec<&str>| v.into_iter().map(str::to_owned).collect();
    ProcedureRules {
        problems: Problem::ALL
            .into_iter()
            .map(|p| ProblemRules {
                problem: p,
                rig: p.rig(),
                handeye: p.handeye(),
                laser: p.laser(),
                freedoms: names(FREEDOM_KEYS.into_iter().filter(|k| p.takes(k)).collect()),
                seed_slots: names(p.seed_slots(false)),
                seed_slots_tilted: names(p.seed_slots(true)),
            })
            .collect(),
        seed_kinds: SEED_SLOTS
            .into_iter()
            .map(|slot| SeedSlotRules {
                slot: slot.to_owned(),
                kinds: names(seed_kinds(slot).to_vec()),
            })
            .collect(),
        by_shape: [(false, false), (true, false), (false, true), (true, true)]
            .into_iter()
            .map(|(rig, lasers)| ShapeRule {
                rig,
                lasers,
                problem: Problem::for_dataset(if rig { 2 } else { 1 }, lasers),
            })
            .collect(),
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

    /// The options set, by their [`FREEDOM_KEYS`] names, in that order.
    #[must_use]
    pub fn set(&self) -> Vec<&'static str> {
        let set = [
            self.refine_intrinsics.is_some(),
            self.refine_sensor.is_some(),
            self.refine_rig.is_some(),
            self.refine_robot_poses.is_some(),
            self.robot_rot_sigma_rad.is_some(),
            self.robot_trans_sigma_m.is_some(),
            self.min_motion_angle_deg.is_some(),
            self.fix_handeye.is_some(),
            self.fix_target.is_some(),
            self.fix_plane.is_some(),
            self.fix_poses.is_some(),
        ];
        FREEDOM_KEYS
            .into_iter()
            .zip(set)
            .filter_map(|(k, s)| s.then_some(k))
            .collect()
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
    /// target's pose per view with them). A `rig_handeye_laserline` stage takes a `rig_handeye`
    /// stage's whole result here (its cameras, rig and hand-eye) as the joint solve's start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rig: Option<Seed>,
    /// The hand-eye transform.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handeye: Option<Seed>,
    /// The laser planes, one per camera in its frame (laser workflows): the design's (nominal or
    /// truth: in a simulation the design is the truth) or an earlier stage's estimate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planes: Option<Seed>,
}

impl Seeds {
    fn all(&self) -> impl Iterator<Item = (&'static str, &Seed)> {
        [
            ("intrinsics", &self.intrinsics),
            ("sensor", &self.sensor),
            ("rig", &self.rig),
            ("handeye", &self.handeye),
            ("planes", &self.planes),
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
    /// The structural checks a library file's procedure gets (ids, seeds naming earlier stages,
    /// freedoms that apply to their workflow), on a procedure that does not come from one (a
    /// run's procedure, passed by value).
    ///
    /// # Errors
    ///
    /// Every problem found.
    pub fn validate(&self) -> Result<(), crate::ValidationError> {
        let mut issues = Issues::default();
        self.check(&mut issues, "procedure");
        issues.finish()
    }

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
            let slots = s.problem.seed_slots(true);
            for (key, seed) in s.seed.all() {
                let at = format!("{at}.seed.{key}");
                if let Seed::Stage(id) = seed
                    && !seen.contains(&id.as_str())
                {
                    issues.push(at.clone(), format!("`{id}` is not an earlier stage"));
                }
                let kind = match seed {
                    Seed::Nominal => "nominal",
                    Seed::Truth => "truth",
                    Seed::Stage(_) => "stage",
                };
                if !slots.contains(&key) {
                    issues.push(
                        at,
                        format!("`{}` has no {key} to seed", problem_name(s.problem)),
                    );
                } else if !seed_kinds(key).contains(&kind) {
                    issues.push(
                        at,
                        "a rig seeds only from an earlier stage (its per-view target poses come with it)",
                    );
                }
            }
            for key in s.free.set() {
                if !s.problem.takes(key) {
                    issues.push(
                        format!("{at}.free.{key}"),
                        format!("does not apply to `{}`", problem_name(s.problem)),
                    );
                }
            }
            let f = &s.free;
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
    fn every_option_has_its_key_and_the_rules_cover_every_workflow() {
        let all: Freedoms = serde_json::from_str(
            r#"{ "refine_intrinsics": true, "refine_sensor": true, "refine_rig": true,
                 "refine_robot_poses": true, "robot_rot_sigma_rad": 0.01, "robot_trans_sigma_m": 0.001,
                 "min_motion_angle_deg": 5.0, "fix_handeye": true, "fix_target": true,
                 "fix_plane": true, "fix_poses": [0] }"#,
        )
        .unwrap();
        assert_eq!(all.set(), FREEDOM_KEYS);
        // The keys are the document's field names, every one of them.
        let doc = serde_json::to_value(&all).unwrap();
        let fields: Vec<&str> = doc
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let mut keys = FREEDOM_KEYS.to_vec();
        keys.sort_unstable();
        assert_eq!(fields, keys);
        let r = rules();
        assert_eq!(r.problems.len(), Problem::ALL.len());
        assert_eq!(r.seed_kinds.len(), SEED_SLOTS.len());
        // Every option applies to some workflow.
        for k in FREEDOM_KEYS {
            assert!(Problem::ALL.iter().any(|p| p.takes(k)), "{k}");
        }
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
