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
import type {
  CameraParams,
  CameraSpec,
  Iso3Schema,
  LaserSpec,
  OpticsSpec,
  RigLayout,
  SceneSpec,
} from "./types/scene.js";
import type { GeneratorSpec, ScenarioSpec } from "./types/scenario.js";
import type { BakedScenario } from "./types/baked-scenario.js";
import type { RobotManifest } from "./types/robot-manifest.js";
import type { MotionReport } from "./types/motion-report.js";

export type * from "./types/scene.js";
export type { AngleSteps, GeneratorSpec, ScenarioSpec, Step, Visibility } from "./types/scenario.js";
export type { BakedScenario, BakedRobot, BakedSample, CaptureEvent } from "./types/baked-scenario.js";
export type * from "./types/motion-report.js";
export type {
  LibraryItem,
  HeadDesign,
  HeadWindow,
  SensorPart,
  LensPart,
  LaserPart,
  TargetPart,
  RigDesign,
} from "./types/library.js";
export type { ProjectManifest, ProjectDefaults } from "./types/project.js";
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

/** The lens's points on a camera body's axis: metres from the image-plane datum, `+z` toward the object. */
export interface CardinalPoints {
  /** The sensor pivot and image-plane datum: `0`. */
  sensor: number;
  /** H′. */
  rear_principal: number;
  /** H. */
  front_principal: number;
  /** E, the centre of projection. */
  entrance_pupil: number;
  /** E′. */
  exit_pupil: number;
  /** F′. */
  rear_focal: number;
  /** F. */
  front_focal: number;
}

/** What a camera's optics give (ADR 0008). */
export interface DerivedOptics {
  /** The equivalent pinhole about the entrance pupil: what the camera's `params` must hold. */
  params: CameraParams;
  /** The projection frame (entrance pupil) in the camera's body frame. */
  mech_se3_projection: Iso3Schema;
  cardinal_points: CardinalPoints;
  /** H′ to the sensor at this focus (m). */
  image_distance_m: number;
  /** H to the plane in focus (m). */
  object_distance_m: number;
  /** Lateral magnification at this focus. */
  magnification: number;
  /** Exit over entrance pupil diameter. */
  pupil_magnification: number;
  entrance_pupil_diameter_m: number;
  exit_pupil_diameter_m: number;
  /** `N · (1 + m / m_p)`. */
  working_f_number: number;
}

/**
 * What `optics` give a camera built as `params`: its principal point and distortion are kept,
 * its focal lengths, skew and sensor tilt derived. A camera with optics must store the returned
 * `params` (`etendue-core::from_scene`).
 *
 * @throws {@link EtendueError} `kind: "input"` for optics that are not physical.
 */
export function deriveOptics(optics: OpticsSpec | string, params: CameraParams | string): DerivedOptics;

/**
 * Parse and check a library item (`library/<folder>/<id>.json`, ADR 0009): its version first,
 * then its fields, its single part, and a camera design's params against its optics.
 *
 * @throws {@link EtendueError} `kind: "invalid"` with every problem found.
 */
export function validateLibraryItem(text: string | object): import("./types/library.js").LibraryItem;

/** A 2-D point or direction of a head's section, `[s, z]` (m): `z` along the camera axis. */
export type SectionPoint = [number, number];

/** A line of a head's section. */
export interface SectionLine {
  point: SectionPoint;
  direction: SectionPoint;
}

/** One traced ray of a head's section: object → entrance pupil → exit pupil → sensor. */
export interface SectionRay {
  kind: "chief" | "marginal";
  object: SectionPoint;
  entrance: SectionPoint;
  exit: SectionPoint;
  /** The object's image, which every ray from it passes through. */
  image: SectionPoint;
  on_sensor: SectionPoint;
}

/**
 * A head's Scheimpflug drawing, in the plane through the camera axis perpendicular to the laser
 * plane (`etendue_core::head::Section`). Coordinates `[s, z]` in metres, in the camera's body
 * frame.
 */
export interface HeadSection {
  /** The section's `s` axis in the camera's body frame. */
  s_axis: [number, number, number];
  cardinal_points: CardinalPoints;
  entrance_radius: number;
  exit_radius: number;
  /** The sensor where the section cuts it. */
  sensor: [SectionPoint, SectionPoint];
  /** The plane of best focus's trace. */
  focus: SectionLine;
  laser_origin: SectionPoint;
  laser_central: SectionPoint;
  laser_trace: SectionLine;
  /** The window's range on the laser trace. */
  window: [SectionPoint, SectionPoint] | null;
  /** The hinge rule: these share a height when the Scheimpflug condition holds. */
  sensor_at_rear_principal: SectionPoint | null;
  focus_at_front_principal: SectionPoint | null;
  /** Rays from the entrance pupil through the sensor's ends. */
  field: [SectionLine, SectionLine];
  rays: SectionRay[];
  /** Near and far polylines where the blur reaches the limit. */
  depth_of_field: [SectionPoint[], SectionPoint[]] | null;
}

/** A head's samples over its window: one value per sample, `across` × `along`, row by row from the nearest range. */
export interface HeadMap {
  across: number;
  along: number;
  /** 1 where the camera sees the point (in front, on the sensor, within the fan). */
  visible: Uint8Array;
  /** Geometric blur. */
  coc_px: Float64Array;
  /** Diffraction blur (the Airy diameter). */
  airy_px: Float64Array;
  /** Both, root sum of squares. */
  blur_px: Float64Array;
  /** Metres per pixel along the laser line. */
  lateral_m_per_px: Float64Array;
  /** Depth per pixel of stripe displacement across the line. */
  depth_m_per_px: Float64Array;
  triangulation_rad: Float64Array;
}

/** A robot's TCP and how far from singular it is. */
export interface RobotTcp {
  base_se3_tcp: Iso3Schema;
  /** Yoshikawa's √det(J Jᵀ): 0 at a singularity. */
  manipulability: number;
}

/** Overrides of the IK solver's defaults. */
export interface IkOptions {
  max_iterations?: number;
  /** Converged within this TCP position error (m); default 1e-10. */
  tol_translation?: number;
  /** … and this rotation error (rad); default 1e-10. */
  tol_rotation?: number;
  /** Largest joint change per step (rad or m); default 0.3. */
  max_step?: number;
  /** Further attempts from spread seeds when the seed's fails; default 16. */
  restarts?: number;
}

/** What IK found. */
export interface IkAnswer {
  converged: boolean;
  /** The solution, or the seed when it did not converge. */
  q: number[];
  iterations: number;
  translation_error: number | null;
  rotation_error: number | null;
  /** At `q`. */
  manipulability: number;
}

/** A box in a rig's frame and its voxels. */
export interface CoverageGrid {
  /** Minimum corner (m). */
  min: [number, number, number];
  /** Maximum corner (m). */
  max: [number, number, number];
  /** Voxels along x, y and z (1–512 each). */
  n: [number, number, number];
}

/** What counts as seeing a voxel. */
export interface CoverageCriteria {
  /** The most total blur (geometric and diffraction) a camera may have there (px); default 2. */
  max_blur_px?: number;
  /** The Airy disc's wavelength for a camera without a laser of its own (nm); default 550. */
  wavelength_nm?: number;
}

/** A rig's coverage: one count per voxel, x fastest, then y, then z. */
export interface RigCoverage {
  grid: CoverageGrid;
  /** How many cameras see the voxel sharply. */
  views: Uint8Array;
  /** How many (laser, camera) pairs see their own laser line in the voxel. */
  lit: Uint8Array;
}

/**
 * What a rig covers over `grid` (in the rig's frame): per voxel, the cameras that see it and the
 * pairs that measure it. Physical cameras see within `max_blur_px`; pinhole cameras have no blur.
 *
 * @throws {@link EtendueError} for a layout that does not validate or a bad grid.
 */
export function rigCoverage(
  layout: import("./types/scene.js").RigLayout | string,
  grid: CoverageGrid,
  criteria?: CoverageCriteria,
): RigCoverage;

/** A camera setting of a head. */
export interface HeadSetting {
  /** Physical sensor tilt `[τx, τy]` (rad). */
  tilt: [number, number];
  /** From the image-plane datum (m). */
  focus_distance_m: number;
  f_number: number;
}

/** What the head optimiser found. */
export interface HeadOptimum {
  start: HeadSetting;
  start_worst_blur_px: number;
  best: HeadSetting;
  worst_blur_px: number;
  iterations: number;
  converged: boolean;
}

type HeadDesignDoc = import("./types/library.js").HeadDesign;

/**
 * A head's samples over a window (the design's when `window` is null): blur, resolution and
 * triangulation at every point of an `across` × `along` grid in its laser plane.
 *
 * @throws {@link EtendueError} for a head without optics or without a window.
 */
export function headMap(
  design: HeadDesignDoc | string,
  window: import("./types/library.js").HeadWindow | string | null,
  across: number,
  along: number,
): HeadMap;

/**
 * A head's Scheimpflug drawing: rays traced from the laser's central ray at `probes` (m), and the
 * depth of field for `blur_limit_px` when given.
 *
 * @throws {@link EtendueError}
 */
export function headSection(
  design: HeadDesignDoc | string,
  options?: { probes?: number[]; blur_limit_px?: number },
): HeadSection;

/**
 * The setting that puts a head's plane of best focus on its laser plane (the Scheimpflug
 * condition) at its f-number.
 *
 * @throws {@link EtendueError} when the laser plane contains the camera's axis.
 */
export function headScheimpflug(design: HeadDesignDoc | string): HeadSetting;

/**
 * The tilt (when free), focus and f-number (within `f_number_range`) that minimise the worst blur
 * over the design's window while the camera sees all of it. Starts from the Scheimpflug condition
 * and never ends worse.
 *
 * @throws {@link EtendueError}
 */
export function headOptimise(
  design: HeadDesignDoc | string,
  search: { grid: [number, number]; f_number_range?: [number, number]; tilt_free: boolean },
): HeadOptimum;

/**
 * `design` with its camera at `setting`: the optics' tilt, focus and f-number set, and the
 * camera's params derived again.
 *
 * @throws {@link EtendueError}
 */
export function headApply(design: HeadDesignDoc | string, setting: HeadSetting): HeadDesignDoc;

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
   * Where camera `cameraId`'s projection frame (its entrance pupil) sits in its body's frame;
   * `undefined` for a camera without optics. The session works in projection frames: a baked
   * camera frame is the projection frame, so its body is at `baked · mechSe3Projection⁻¹`.
   *
   * @throws {@link EtendueError} `kind: "input"` for an unknown camera.
   */
  mechSe3Projection(cameraId: string): Iso3Schema | undefined;

  /** Robot `robotId`'s links (URDF names), in the order `robotLinkPoses` gives them. */
  robotLinks(robotId: string): string[];

  /**
   * `base_se3_link` of every link of robot `robotId` at joints `q`: seven values per link,
   * `[qx, qy, qz, qw, tx, ty, tz]`, in the order of `robotLinks`.
   *
   * @throws {@link EtendueError} for an unknown robot, or `q` of the wrong length or outside the limits.
   */
  robotLinkPoses(robotId: string, q: Float64Array | readonly number[]): Float64Array;

  /** Robot `robotId`'s TCP in its base frame at `q`, and its manipulability (0 at a singularity). */
  robotTcp(robotId: string, q: Float64Array | readonly number[]): RobotTcp;

  /**
   * Joints that put robot `robotId`'s TCP at `baseSe3Tcp` (its base frame), from `seed`: damped
   * least squares. Pass `{ restarts: 0 }` while dragging so the arm moves continuously. A pose
   * it does not reach gives `converged: false` and the seed.
   */
  robotIk(
    robotId: string,
    baseSe3Tcp: Iso3Schema,
    seed: Float64Array | readonly number[],
    options?: IkOptions,
  ): IkAnswer;

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
