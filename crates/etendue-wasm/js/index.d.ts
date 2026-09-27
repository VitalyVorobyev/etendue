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
import type { SceneSpec, Iso3Schema } from "./types/scene.js";
import type { ScenarioSpec } from "./types/scenario.js";
import type { BakedScenario } from "./types/baked-scenario.js";
import type { RobotManifest } from "./types/robot-manifest.js";

export type * from "./types/scene.js";
export type { ScenarioSpec, Step } from "./types/scenario.js";
export type { BakedScenario, BakedRobot, BakedSample, CaptureEvent } from "./types/baked-scenario.js";
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
export default function init(
  input?: { module_or_path?: InitInput | Promise<InitInput> },
): Promise<InitOutput>;

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
   * Compile and bake a scenario: `world_se3_frame` for every scene frame at every sample.
   * A scenario with no steps bakes to one sample at the robots' `initial_q`.
   *
   * @throws {@link EtendueError}
   */
  bake(scenario: ScenarioSpec | string): BakedScenario;

  /**
   * Project world points (flat `[x0, y0, z0, x1, …]`, metres) through camera `cameraId`
   * placed at `worldSe3Camera`. Returns flat pixels `[u0, v0, …]`; a point the camera cannot
   * image yields `NaN, NaN`. Points outside the image are returned as projected.
   *
   * @throws {@link EtendueError} `kind: "input"` for an unknown camera or a ragged array.
   */
  projectPoints(
    cameraId: string,
    worldSe3Camera: Iso3,
    xyzWorld: Float64Array | readonly number[],
  ): Float64Array;

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

  /** Release the wasm memory. */
  free(): void;
  [Symbol.dispose](): void;
}
