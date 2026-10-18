/**
 * `@etendue/wasm` — the etendue kernel in WebAssembly: scene validation, scenario baking
 * (kinematics), and camera projection. All math is the Rust crates'
 * (`etendue-kinematics`, `vision-calibration-core`); this package only marshals.
 *
 * ```ts
 * import init, { EtendueScene } from "@etendue/wasm";
 * await init();
 * const scene = new EtendueScene(sceneSpec, [{ id: "ur5e", manifest, urdf }]);
 * const baked = scene.bake(scenarioSpec);
 * ```
 *
 * @packageDocumentation
 */

import type { InitInput, InitOutput, SyncInitInput } from "./etendue_wasm.js";
import type { CameraParams, CameraSpec, Iso3Schema, LaserSpec, RigLayout, SceneSpec } from "./types/scene.js";
import type { GeneratorSpec, ScenarioSpec } from "./types/scenario.js";
import type { BakedScenario } from "./types/baked-scenario.js";
import type { RobotManifest } from "./types/robot-manifest.js";
import type { MotionReport } from "./types/motion-report.js";

export type * from "./types/scene.js";
export type { AngleSteps, GeneratorSpec, ScenarioSpec, Step, Visibility } from "./types/scenario.js";
export type { BakedScenario, BakedRobot, BakedSample, CaptureEvent } from "./types/baked-scenario.js";
export type * from "./types/motion-report.js";
export type { CaptureExport, CaptureRow } from "./types/captures.js";
export type {
  RobotManifest,
  ManifestJoint,
  ManifestLicense,
  ManifestSource,
  ManifestVisual,
} from "./types/robot-manifest.js";

/**
 * Load the wasm module (browser). Resolves the `.wasm` next to this file unless given
 * `module_or_path`. Call once before constructing an {@link EtendueScene}.
 */
export default function init(input?: { module_or_path?: InitInput | Promise<InitInput> }): Promise<InitOutput>;

/** Load the wasm module synchronously from its bytes or a compiled module (Node, workers). */
export function initSync(input: { module: SyncInitInput }): InitOutput;

/** An SE(3) in the wire form `{rotation: [qx, qy, qz, qw], translation: [tx, ty, tz]}`. */
export type Iso3 = Iso3Schema;

/** The contents of one robot's asset files, for the scene robot `id`. */
export interface RobotSource {
  /** The scene robot this model is for (`SceneSpec.robots[].id`). */
  id: string;
  /** The parsed `robot.json`. */
  manifest: RobotManifest;
  /** The URDF text the manifest points to. */
  urdf: string;
}

/** Which image coordinate names a pixel's centre: `i` (`"integer"`) or `i + ½` (`"half"`).
 * Decided by probe P4-2 (etendue ADR 0004); until then every caller states it. */
export type PixelCentre = "integer" | "half";

/** How to choose a canonical render camera (etendue-synth `CanonicalSpec`). */
export interface CanonicalSpec {
  /** Canonical pixels per target pixel at the image centre, ≥ 1. Default 1. */
  supersample?: number;
  /** Extra field of view as a fraction of each half-extent. Default 0.02. */
  margin?: number;
  /** Scan spacing in target pixels for the coverage search. Default 8. */
  scan_step_px?: number;
}

/** The canonical render camera (etendue-synth `CanonicalCamera`): a centred, square-pixel pinhole. */
export interface CanonicalCamera {
  /** calibration-rs parameters: pinhole, no distortion, identity sensor, `fx = fy`, `skew = 0`. */
  params: CameraParams;
  /** `[width, height]` in canonical pixels. */
  resolution: [number, number];
  pixel_centre: PixelCentre;
}

/** A camera's canonical render camera and remap LUT. */
export interface Remap {
  canonical: CanonicalCamera;
  /**
   * Row-major, two values per target pixel: the canonical-image coordinate to sample for that
   * pixel (`NaN` where none). `2 · width · height` floats, the RG32F texture layout.
   */
  lut: Float32Array;
}

/** Why a generated candidate pose was kept or not. */
export type CandidateStatus =
  | { status: "ok" }
  | { status: "unreachable" }
  | { status: "near_limit"; joint: string; margin: number }
  | { status: "near_singular"; sigma: number }
  | { status: "not_visible"; camera: string; visible: number; total: number };

/** One candidate sensor placement (etendue-synth `poses::Candidate`). */
export interface Candidate {
  radius_m: number;
  theta_rad: number;
  phi_rad: number;
  roll_rad: number;
  /** Where the sensor would be. */
  world_se3_sensor: Iso3;
  /** The tool pose that places it, in the robot's base frame. */
  base_se3_tool: Iso3;
  /** The joint configuration, when the robot reaches it. */
  q?: number[];
  /** Found only from an IK restart seed: possibly another branch than its neighbours. */
  ik_restart?: boolean;
  status: CandidateStatus;
  /** Its capture id, when kept. */
  capture?: string;
}

/** What `EtendueScene.generatePoses` produced. */
export interface GeneratedPoses {
  candidates: Candidate[];
  scenario: ScenarioSpec;
}

/** A target surface as triangles (`EtendueScene.targetMesh`). */
export interface TargetMesh {
  /** Vertex positions, xyz, metres, in the target frame. */
  positions: Float32Array;
  /** Vertex normals, xyz, outward. */
  normals: Float32Array;
  /** Triangles, three vertex indices each, counter-clockwise from outside. */
  indices: Uint32Array;
  /** Per triangle: 1 ink, 0 paper. */
  dark: Uint8Array;
}

/** A target feature point (etendue-synth `BoardPoint`). */
export interface TargetPoint {
  /** On the surface: `[x, y]` of a planar target; a PuzzlePole's unrolled `[z, arc]`. */
  position_m: [number, number];
  /** In the target frame, for a target that is not planar. */
  xyz_m?: [number, number, number];
  /** The outward normal there, for a target that is not planar. */
  normal?: [number, number, number];
  /** calib-targets' grid label. */
  grid?: [number, number];
  /** calib-targets' point id. */
  id?: number;
}

/** A PuzzlePole circumference calib-targets supports, with its seam row. */
export interface PolePeriod {
  squares: number;
  start_row: number;
}

/** The PuzzlePole circumferences calib-targets supports (`circumference_squares`, `start_row`). */
export function poleCircumferences(): PolePeriod[];

/** One validation problem, located by document path (e.g. `cameras[1].params`). */
export interface Issue {
  path: string;
  message: string;
}

/** What every function of this package throws. */
export interface EtendueError extends Error {
  /**
   * `parse` — a document is not valid JSON or does not match its schema;
   * `invalid` — it parsed but failed validation (see `issues`);
   * `kinematics` — a robot model, IK, or scenario step failed;
   * `input` — bad call arguments.
   */
  kind: "parse" | "invalid" | "kinematics" | "input";
  /** For `invalid`: which document (`scene`, `scenario`, `robot \`id\` manifest`). */
  document?: string;
  /** For `invalid`: every problem found. */
  issues?: Issue[];
}

/**
 * The cameras (`<rigId>_cam<i>`) and lasers (`<rigId>_laser<i>`, each linked to its camera) of a
 * rig layout, attached to rig `rigId`: mono, stereo, or a ring of cameras around the rig's +Z axis
 * converging on the working point (etendue-scene `RigLayout::expand`).
 *
 * @throws {@link EtendueError} `kind: "invalid"` for a layout that does not validate.
 */
export function expandRig(layout: RigLayout | string, rigId: string): { cameras: CameraSpec[]; lasers: LaserSpec[] };

/** Whether `error` was thrown by this package. */
export function isEtendueError(error: unknown): error is EtendueError;

/**
 * A loaded, validated scene: the scene, one robot model per scene robot, and one
 * calibration-rs camera model per camera. Holds wasm memory; call {@link EtendueScene.free}
 * (or use `using`) when done.
 */
export class EtendueScene {
  /**
   * Parse and validate `scene` and its robots, as `etendue validate` does. Documents may be
   * objects or JSON text.
   *
   * @throws {@link EtendueError}
   */
  constructor(scene: SceneSpec | string, robots: RobotSource[] | string);

  /**
   * Replace the scene, keeping the robot models (no URDF is re-parsed): the edit path of an
   * editor. The new scene must name the same robot ids; to add or remove a robot, construct a
   * new session. On error the session is unchanged.
   *
   * @throws {@link EtendueError}
   */
  updateScene(scene: SceneSpec | string): void;

  /**
   * Compile and bake a scenario: `world_se3_frame` for every scene frame at every sample.
   * A scenario with no steps bakes to one sample at the robots' `initial_q`. With
   * `derivatives`, every sample also records joint velocities and accelerations.
   *
   * @throws {@link EtendueError}
   */
  bake(scenario: ScenarioSpec | string, options?: { derivatives?: boolean }): BakedScenario;

  /**
   * How every step of a scenario uses the robots' limits: per joint the peak velocity,
   * acceleration and jerk as fractions of the limits, the binding limit, the smallest
   * joint-limit margin and Jacobian singular value, how a `ptp_pose` found its configuration,
   * and warnings (`etendue export --report`).
   *
   * @throws {@link EtendueError}
   */
  motionReport(scenario: ScenarioSpec | string): MotionReport;

  /**
   * The scenario with every `ptp_pose`'s joint configuration (`q`) stored, so that baking it
   * again can never choose another IK branch.
   *
   * @throws {@link EtendueError}
   */
  resolveScenario(scenario: ScenarioSpec | string): ScenarioSpec;

  /**
   * Robot `robotId`'s tool pose (in its base frame) and joints at every capture, as CSV
   * (`capture, t, x, y, z, qx, qy, qz, qw, rx, ry, rz, <joints>`; SI units, rotation as quaternion
   * and rotation vector) or JSON (`captures.schema.json`).
   *
   * @throws {@link EtendueError}
   */
  exportCaptures(scenario: ScenarioSpec | string, robotId: string, format: "csv" | "json"): string;

  /**
   * Run a pose generator — a scenario's `generator`, e.g. `{type: "sphere", robot, look, target,
   * radii_m, theta, phi, …}` — on this scene: every candidate pose (where the sensor would be, the
   * tool pose, the joint configuration, and whether it was kept and why not) and the stop-and-shoot
   * scenario of the kept ones, ordered into a short tour.
   *
   * @throws {@link EtendueError}
   */
  generatePoses(spec: GeneratorSpec | string, dt?: number): GeneratedPoses;

  /**
   * Robot `robotId`'s timed joint trajectory as CSV: `t`, the capture id on capture samples, then
   * positions, velocities and accelerations of every joint.
   *
   * @throws {@link EtendueError}
   */
  exportTrajectory(scenario: ScenarioSpec | string, robotId: string): string;

  /**
   * Project world points (flat `[x0, y0, z0, x1, …]`, metres) through camera `cameraId`
   * placed at `worldSe3Camera`. Returns flat pixels `[u0, v0, …]`; a point the camera cannot
   * image yields `NaN, NaN`. Points outside the image are returned as projected.
   *
   * @throws {@link EtendueError} `kind: "input"` for an unknown camera or a ragged array.
   */
  projectPoints(cameraId: string, worldSe3Camera: Iso3, xyzWorld: Float64Array | readonly number[]): Float64Array;

  /**
   * Back-project pixels (flat `[u0, v0, …]`) of camera `cameraId` to viewing rays, returned
   * as camera-frame points on the `z = 1` plane (flat `[x0, y0, 1, …]`). Inverse intrinsics,
   * sensor and iterative undistortion — calibration-rs's `backproject_pixel`. Use it to draw
   * a camera's true field of view without camera math of your own.
   *
   * @throws {@link EtendueError} `kind: "input"` for an unknown camera or an odd-length array.
   */
  backprojectPixels(cameraId: string, uv: Float64Array | readonly number[]): Float64Array;

  /**
   * Extent `[x, y]` in metres of target `targetId`, centred on its origin (for a board:
   * columns along local X, rows along local Y, pattern area without margin), or `undefined`
   * when the geometry does not determine it (a puzzleboard).
   *
   * @throws {@link EtendueError} `kind: "input"` for an unknown target.
   */
  targetExtent(targetId: string): [number, number] | undefined;

  /**
   * The surface of target `targetId` as triangles in its frame: the pattern calib-targets prints
   * (chessboard, ChArUco, marker board, PuzzleBoard, the PuzzlePole cylinder) or a plain paper
   * quad. Positions and normals are xyz triples; `dark` is per triangle (ink 1, paper 0).
   *
   * @throws {@link EtendueError} `kind: "input"` for an unknown target or an undetermined surface.
   */
  targetMesh(targetId: string): TargetMesh;

  /**
   * Target `targetId`'s feature points (inner corners, with calib-targets' grid labels and ids),
   * in its frame; none for a target calib-targets does not print.
   *
   * @throws {@link EtendueError} `kind: "input"` for an unknown or invalid target.
   */
  targetPoints(targetId: string): TargetPoint[];

  /**
   * Target `targetId`'s feature points as camera `cameraId` sees them, both placed in the world:
   * flat `[u0, v0, …]`, `NaN, NaN` for a point the camera does not see (behind it, outside the
   * image, or on a side of the target facing away).
   *
   * @throws {@link EtendueError} `kind: "input"` for an unknown camera or target.
   */
  projectTarget(cameraId: string, worldSe3Camera: Iso3, targetId: string, worldSe3Target: Iso3): Float64Array;

  /**
   * Laser `laserId`'s stripe on target `targetId` as camera `cameraId` sees it, all placed in the
   * world: flat `[u0, v0, …]` along the stripe (1 mm apart on the target), `NaN, NaN` where the
   * camera does not see it or the stripe breaks. Empty if the laser misses the target.
   *
   * @throws {@link EtendueError} `kind: "input"` for an unknown camera, laser or target.
   */
  laserStripe(
    cameraId: string,
    worldSe3Camera: Iso3,
    laserId: string,
    worldSe3Laser: Iso3,
    targetId: string,
    worldSe3Target: Iso3,
  ): Float64Array;

  /**
   * The canonical render camera of camera `cameraId` and its remap LUT (etendue ADR 0004):
   * render the canonical pinhole, then sample it at `lut[pixel]` for each target pixel to get
   * the target camera's image — distortion, skew and Scheimpflug geometry included.
   *
   * @throws {@link EtendueError} `kind: "input"` for an unknown camera or a bad spec.
   */
  remap(cameraId: string, spec: CanonicalSpec | undefined, pixelCentre: PixelCentre): Remap;

  /** Release the wasm memory. */
  free(): void;
  [Symbol.dispose](): void;
}
