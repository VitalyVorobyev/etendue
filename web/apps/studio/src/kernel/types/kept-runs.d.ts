/**
 * DO NOT EDIT: generated from schemas/*.schema.json by `bun run generate:types` (in web/).
 * The source of truth is the Rust types (`etendue-scene`, `etendue-cli::calibrate`).
 */

/**
 * Where a stage's starting value of an estimate comes from.
 */
export type Seed =
  | "nominal"
  | "truth"
  | {
      stage: string;
    };
/**
 * A robust loss, as calibration-rs names it.
 */
export type Loss =
  | {
      kind: "none";
    }
  | {
      kind: "huber";
      /**
       * Where the loss turns linear.
       */
      scale: number;
    }
  | {
      kind: "cauchy";
      /**
       * The scale.
       */
      scale: number;
    }
  | {
      kind: "arctan";
      /**
       * The scale.
       */
      scale: number;
    };
/**
 * The calibration-rs workflow a dataset was solved with.
 */
export type Workflow =
  | "single_cam_handeye"
  | "rig_handeye"
  | "rig_handeye_laserline"
  | "laserline_device"
  | "planar_intrinsics"
  | "rig_extrinsics";

/**
 * The runs kept under a folder, and the folders that hold a run this etendue cannot read.
 */
export interface KeptRuns {
  /**
   * By name.
   */
  runs: StoredRun[];
  /**
   * By name: each folder with a run config that did not read, and why.
   */
  unreadable: UnreadableRun[];
}
/**
 * A kept run, read back.
 */
export interface StoredRun {
  config: RunConfig;
  /**
   * Its folder's name.
   */
  name: string;
  run: Run;
}
/**
 * What a kept run calibrated, and how.
 */
export interface RunConfig {
  /**
   * The dataset directory, as given.
   */
  dataset: string;
  options: RunOptions;
  version: number;
}
/**
 * How [`run_dir`] calibrates a dataset directory.
 */
export interface RunOptions {
  /**
   * Calibrate this camera of a rig alone.
   */
  camera?: string | null;
  /**
   * Calibrate `etendue detect`'s observations (`features.json`, `laser_features.json`)
   * instead of the ground truth's.
   */
  detected?: boolean;
  noise?: ObservationNoise;
  /**
   * The procedure to run, by value (so a kept run says exactly what ran); `None` for `auto`.
   */
  procedure?: RunProcedure | null;
  /**
   * Trials, with seeds `seed, seed + 1, …` (at least one).
   */
  trials?: number;
}
/**
 * Noise on the analytic observations (`seed` is the first trial's).
 */
export interface ObservationNoise {
  /**
   * Share of seen target points the detector misses, in `[0, 1]`.
   */
  dropout_fraction?: number;
  /**
   * Laser-stripe pixel noise, per axis (px).
   */
  laser_sigma_px?: number;
  /**
   * Share of seen target points that are outliers, in `[0, 1]`.
   */
  outlier_fraction?: number;
  /**
   * An outlier's displacement, per axis (px).
   */
  outlier_sigma_px?: number;
  /**
   * Target-point pixel noise, per axis (px).
   */
  pixel_sigma_px?: number;
  /**
   * The robot's reported TCP rotation error, per rotation-vector component (rad).
   */
  robot_rot_sigma_rad?: number;
  /**
   * The robot's reported TCP translation error, per axis (m).
   */
  robot_trans_sigma_m?: number;
  /**
   * The stream's seed.
   */
  seed?: number;
}
/**
 * A procedure as a run keeps it: its id and its stages.
 */
export interface RunProcedure {
  id: string;
  procedure: ProcedurePart;
}
/**
 * A calibration procedure: stages in order. The last stage's result is the procedure's.
 */
export interface ProcedurePart {
  /**
   * The stages, run in order.
   */
  stages: Stage[];
}
/**
 * One stage of a procedure.
 */
export interface Stage {
  /**
   * The camera a single-camera stage calibrates, by scene id; absent, the dataset's only
   * camera (a stage per camera repeats with each id). Rig stages take every camera.
   */
  camera?: string | null;
  free?: Freedoms;
  /**
   * Its id, unique in the procedure; later stages seed from it by this id.
   */
  id: string;
  /**
   * The camera model and its fixed parameters.
   */
  model?: CameraChoice | null;
  /**
   * The workflow.
   */
  problem:
    | "planar_intrinsics"
    | "rig_extrinsics"
    | "single_cam_handeye"
    | "rig_handeye"
    | "laserline_device"
    | "rig_handeye_laserline";
  seed?: Seeds;
  /**
   * How it solves.
   */
  solver?: SolverChoice | null;
}
/**
 * Further estimates freed or held.
 */
export interface Freedoms {
  /**
   * Hold the hand-eye in the joint laser solve (`rig_handeye_laserline`).
   */
  fix_handeye?: boolean | null;
  /**
   * Hold the laser plane (`laserline_device`).
   */
  fix_plane?: boolean | null;
  /**
   * The target poses (view indices) held at their linear estimate (`planar_intrinsics`,
   * `laserline_device`).
   */
  fix_poses?: number[] | null;
  /**
   * Hold the target's pose in the joint laser solve (`rig_handeye_laserline`).
   */
  fix_target?: boolean | null;
  /**
   * The least robot rotation between views the linear hand-eye uses, degrees.
   */
  min_motion_angle_deg?: number | null;
  /**
   * Refine the intrinsics jointly with the rig and the hand-eye (rig workflows).
   */
  refine_intrinsics?: boolean | null;
  /**
   * Refine the cameras' poses in the rig during the hand-eye solve (`rig_handeye`).
   */
  refine_rig?: boolean | null;
  /**
   * Refine each view's robot pose under a prior (hand-eye workflows).
   */
  refine_robot_poses?: boolean | null;
  /**
   * Refine a Scheimpflug sensor's tilt jointly with the rig (rig workflows).
   */
  refine_sensor?: boolean | null;
  /**
   * The robot-pose prior's rotation σ, radians.
   */
  robot_rot_sigma_rad?: number | null;
  /**
   * The robot-pose prior's translation σ, metres.
   */
  robot_trans_sigma_m?: number | null;
}
/**
 * The camera model of a stage and what of it stays fixed.
 */
export interface CameraChoice {
  fix?: FixLists;
  /**
   * The sensor model.
   */
  sensor?: "design" | "identity" | "scheimpflug";
}
/**
 * Fixed parameters.
 */
export interface FixLists {
  /**
   * `k1`, `k2`, `k3`, `p1`, `p2`.
   */
  distortion?: string[] | null;
  /**
   * `fx`, `fy`, `cx`, `cy`.
   */
  intrinsics?: string[] | null;
  /**
   * `tilt_x`, `tilt_y`.
   */
  sensor?: string[] | null;
}
/**
 * What it starts from.
 */
export interface Seeds {
  /**
   * The hand-eye transform.
   */
  handeye?: Seed | null;
  /**
   * Focal lengths and principal points (and with them the distortion).
   */
  intrinsics?: Seed | null;
  /**
   * The laser planes, one per camera in its frame (laser workflows): the design's (nominal or
   * truth: in a simulation the design is the truth) or an earlier stage's estimate.
   */
  planes?: Seed | null;
  /**
   * The cameras' poses in the rig: from an earlier rig stage only (calibration-rs takes the
   * target's pose per view with them). A `rig_handeye_laserline` stage takes a `rig_handeye`
   * stage's whole result here (its cameras, rig and hand-eye) as the joint solve's start.
   */
  rig?: Seed | null;
  /**
   * Scheimpflug tilts.
   */
  sensor?: Seed | null;
}
/**
 * How a stage solves. Absent fields keep the workflow's defaults.
 */
export interface SolverChoice {
  /**
   * Iteration limit of the non-linear solve.
   */
  max_iters?: number | null;
  /**
   * The robust loss on the reprojection residuals.
   */
  robust_loss?: Loss | null;
}
/**
 * What [`run_dir`] found: every trial's report and, with several, each row's spread.
 */
export interface Run {
  /**
   * With several trials, each estimated parameter's spread ([`param_stats`]).
   */
  param_stats?: ParamStats[] | null;
  reports: Report[];
  row_stats?: RowStats[] | null;
}
/**
 * One estimated parameter over trials, beside its truth and calibration-rs's predicted σ.
 */
export interface ParamStats {
  /**
   * `mean − truth`.
   */
  bias: number;
  /**
   * `cam_left fx`, `hand-eye tx`, …
   */
  label: string;
  mean: number;
  /**
   * calibration-rs's σ for it, the mean over the trials that have one.
   */
  predicted_sigma?: number | null;
  /**
   * Root mean square of `value − truth`.
   */
  rmse: number;
  /**
   * Sample standard deviation over the trials.
   */
  std: number;
  /**
   * Trials it was estimated in.
   */
  trials: number;
  truth: number;
  /**
   * `px`, `deg`, `mm`, or empty (distortion coefficients).
   */
  unit: string;
}
/**
 * A calibration and its comparison with the truth.
 */
export interface Report {
  /**
   * The calibrated cameras.
   */
  cameras: string[];
  estimate: Estimate;
  /**
   * `"G5.1"`, or `"G5.3"` with lasers.
   */
  gate: string;
  /**
   * Whether the gate applies (exact analytic observations).
   */
  gated: boolean;
  laser_pixels: number;
  noise?: ObservationNoise | null;
  /**
   * Whether every gated row is within its limit.
   */
  pass: boolean;
  points: number;
  /**
   * The procedure that ran (ADR 0011): a built-in's id (`auto_rig`, …) or a library item's.
   */
  procedure?: string;
  /**
   * calibration-rs's quality verdict on the estimate (`analysis::quality`).
   */
  quality?: QualityReport | null;
  residuals: Residuals;
  rows: Row[];
  /**
   * What each stage gave, in order (the last is the report's own).
   */
  stages?: StageSummary[];
  /**
   * calibration-rs's predicted uncertainty of the estimate.
   */
  uncertainty?: Predicted | null;
  version: number;
  /**
   * Views given to the solver, target points, laser pixels.
   */
  views: number;
  workflow: Workflow;
}
/**
 * What the calibration found, for drawing beside the truth.
 */
export interface Estimate {
  /**
   * Per calibrated camera: `[fx, fy, cx, cy]`, `[k1, k2, k3, p1, p2]`, `[tilt_x, tilt_y]`.
   */
  cameras: EstimatedCamera[];
  /**
   * The hand-eye transform the workflow solves (`gripper_se3_camera`/`_rig` eye-in-hand,
   * `camera_se3_base`/`rig_se3_base` eye-to-hand), its rig frame the reference camera's.
   */
  handeye?: Iso3Schema | null;
  /**
   * Laser planes in each camera's frame, `n·p + d = 0`.
   */
  laser_planes: (Plane | null)[];
  /**
   * The estimate in the truth's frames, to draw beside it; `None` without a hand-eye or a
   * target pose.
   */
  offsets?: Offsets | null;
}
export interface EstimatedCamera {
  cam_se3_ref: Iso3Schema;
  /**
   * @minItems 5
   * @maxItems 5
   */
  distortion: [number, number, number, number, number];
  id: string;
  /**
   * @minItems 4
   * @maxItems 4
   */
  intrinsics: [number, number, number, number];
  /**
   * @minItems 2
   * @maxItems 2
   */
  tilt: [number, number];
}
/**
 * `cam_se3_ref`: the camera from the reference camera (identity for it).
 */
export interface Iso3Schema {
  /**
   * Unit quaternion `[qx, qy, qz, qw]` (i, j, k, w order).
   *
   * @minItems 4
   * @maxItems 4
   */
  rotation: [number, number, number, number];
  /**
   * Translation `[tx, ty, tz]` in meters.
   *
   * @minItems 3
   * @maxItems 3
   */
  translation: [number, number, number];
}
export interface Plane {
  distance: number;
  /**
   * @minItems 3
   * @maxItems 3
   */
  normal: [number, number, number];
}
/**
 * Where the estimate is relative to the truth. A drawing places each true frame where the
 * scene has it and the estimate at that frame times its offset, at any robot pose.
 */
export interface Offsets {
  /**
   * Per calibrated camera: the estimated camera in the true camera's frame
   * (`true_se3_estimate`, projection frames), through the hand-eye and the rig.
   */
  cameras: Iso3Schema[];
  /**
   * Per calibrated camera, its laser plane: true, then estimated, both in the true camera's
   * frame with unit normals pointing the same way.
   */
  planes: ([Plane, Plane] | null)[];
  target: Iso3Schema;
}
/**
 * The verdict on a calibration result and the checks behind it.
 */
export interface QualityReport {
  /**
   * Every check, the result-wide ones first, then each camera's.
   */
  checks: QualityCheck[];
  /**
   * The worst verdict of the assessed checks; `warn` when no check could
   * be assessed, since then nothing vouches for the result.
   */
  verdict: "pass" | "warn" | "fail";
}
/**
 * One check of a [`QualityReport`].
 */
export interface QualityCheck {
  /**
   * The camera, for a check made per camera; `None` for one about the
   * whole result.
   */
  camera?: number | null;
  /**
   * What the check examines.
   */
  kind:
    | "observability"
    | "convergence"
    | "variance_factor"
    | "projection_sigma"
    | "feature_coverage"
    | "view_outliers"
    | "parameter_correlation"
    | "covariance_linearity";
  /**
   * The measured value; absent for a check not applicable, and for the
   * observability and convergence checks, which read a status.
   */
  measurement?: Measurement | null;
  /**
   * One sentence on the outcome, or on why the check was not applicable.
   */
  message: string;
  /**
   * Its outcome.
   */
  status: "pass" | "warn" | "fail" | "not_applicable";
  /**
   * What the check names: unobservable or at-bound parameter labels,
   * correlated parameter pairs (`fx/cx`), or outlier views (`view 3`).
   */
  subjects?: string[];
}
/**
 * A check's measured value and the limit that decided its status.
 */
export interface Measurement {
  /**
   * The limit it was compared to: the one it crossed, or, for a pass, the
   * nearest one.
   */
  threshold: number;
  /**
   * The unit of both.
   */
  unit: "pixel" | "fraction" | "ratio" | "sigma" | "correlation";
  /**
   * The measured value; for a check over several items (views, parameter
   * pairs), the worst one.
   */
  value: number;
}
/**
 * Reprojection residuals of the target points.
 */
export interface Residuals {
  /**
   * Points with a residual.
   */
  count: number;
  /**
   * Every point's Euclidean error (px), view-major.
   */
  errors_px: number[];
  /**
   * Every point's residual with where it is (the first trial's only, in a run of several).
   */
  features?: FeatureResidual[];
  /**
   * Largest Euclidean error (px).
   */
  max_px: number;
  /**
   * calibration-rs's mean Euclidean error (px).
   */
  mean_px: number;
  /**
   * Mean Euclidean error per camera (px), in the ground truth's camera order.
   */
  per_camera_mean_px: number[];
  /**
   * Root mean square per image axis (px): `√(Σ(du² + dv²) / 2n)`.
   */
  rms_axis_px: number;
}
/**
 * One target point's reprojection residual.
 */
export interface FeatureResidual {
  /**
   * The camera, by id.
   */
  camera: string;
  /**
   * The capture, by id.
   */
  capture: string;
  /**
   * The observed pixel.
   *
   * @minItems 2
   * @maxItems 2
   */
  observed_px: [number, number];
  /**
   * The target point, by its index in the ground truth's target.
   */
  point: number;
  /**
   * The pixel the estimate projects it to.
   *
   * @minItems 2
   * @maxItems 2
   */
  projected_px: [number, number];
}
/**
 * One compared quantity: its error, and the gate's limit if it has one.
 */
export interface Row {
  label: string;
  limit?: number | null;
  value: number;
}
/**
 * One stage of a procedure's run.
 */
export interface StageSummary {
  cameras: string[];
  id: string;
  mean_reproj_px: number;
  points: number;
  views: number;
  workflow: Workflow;
}
/**
 * calibration-rs's predicted uncertainty of the estimate (its covariance's diagonal).
 */
export interface Predicted {
  /**
   * The parameters held fixed (the σ are conditional on them).
   */
  fixed: string[];
  /**
   * calibration-rs's parameter labels (`cam0.fx`, `gripper_se3_rig.tx`, …), `camN` being the
   * report's `cameras[N]`.
   */
  labels: string[];
  /**
   * The reprojection noise σ the residuals imply (px).
   */
  noise_px?: number | null;
  /**
   * The σ of each, in calibration-rs's units (px, radians, metres).
   */
  sigma: number[];
}
/**
 * Statistics of a row over trials.
 */
export interface RowStats {
  label: string;
  limit?: number | null;
  max: number;
  mean: number;
  std: number;
}
/**
 * A kept run that does not read: a newer etendue's, or damaged.
 */
export interface UnreadableRun {
  /**
   * Its folder's name.
   */
  name: string;
  reason: string;
}
