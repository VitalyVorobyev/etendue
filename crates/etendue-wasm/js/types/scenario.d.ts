/**
 * DO NOT EDIT: generated from schemas/*.schema.json by `bun run generate:types` (in web/).
 * The source of truth is the `etendue-scene` Rust types.
 */

/**
 * How a scenario's poses were generated, tagged on `type`.
 */
export type GeneratorSpec = {
  /**
   * Sphere centre in the target frame, metres; default the target's
   * centre (a PuzzlePole's axis midpoint).
   *
   * @minItems 3
   * @maxItems 3
   */
  centre_m?: [number, number, number] | null;
  /**
   * Smallest distance of any joint from its limits (rad or m).
   */
  joint_margin_rad?: number;
  /**
   * The sensor frame: a rig, a camera, or any frame rigidly carried by
   * the robot's tool.
   */
  look: string;
  /**
   * The sensor axis relative to `look` (identity by default).
   */
  look_se3_sensor?: Iso3Schema | null;
  /**
   * Smallest singular value of the tool Jacobian.
   */
  min_sigma?: number;
  phi: AngleSteps;
  /**
   * The sphere's pole (θ = 0) in the target frame; default +Z, the
   * board's normal (for a PuzzlePole, +X, the seam's outward normal).
   *
   * @minItems 3
   * @maxItems 3
   */
  pole?: [number, number, number] | null;
  /**
   * Sphere radii, metres.
   */
  radii_m: number[];
  require?: Visibility;
  /**
   * Go back to the start configuration at the end.
   */
  return_home?: boolean;
  /**
   * The robot that carries the sensor.
   */
  robot: string;
  /**
   * Rolls of the sensor about its axis, radians (default `[0]`).
   */
  roll_rad?: number[];
  /**
   * Fraction of the joint limits the moves use, in `(0, 1]`.
   */
  speed_scale?: number;
  /**
   * The target looked at.
   */
  target: string;
  theta: AngleSteps;
  type: "sphere";
};
/**
 * One scenario step, tagged on `type`.
 */
export type Step =
  | {
      /**
       * Target joint positions, manifest joint order.
       */
      q: number[];
      /**
       * Robot id.
       */
      robot: string;
      /**
       * Fraction of the joint velocity, acceleration and jerk limits, in
       * `(0, 1]`.
       */
      speed_scale?: number;
      type: "ptp_joints";
    }
  | {
      base_se3_tool: Iso3Schema;
      /**
       * The joint configuration that reaches `base_se3_tool`, when it has
       * been chosen (a pose generator, `resolve`): the move then goes
       * there without inverse kinematics, so baking can never switch to
       * another IK branch. It must reach the pose (checked when the
       * scenario is compiled).
       */
      q?: number[] | null;
      /**
       * Robot id.
       */
      robot: string;
      /**
       * Fraction of the joint velocity, acceleration and jerk limits, in
       * `(0, 1]`.
       */
      speed_scale?: number;
      type: "ptp_pose";
    }
  | {
      base_se3_tool: Iso3Schema;
      /**
       * Robot id.
       */
      robot: string;
      /**
       * Fraction of the joint velocity, acceleration and jerk limits, in
       * `(0, 1]`.
       */
      speed_scale?: number;
      type: "lin";
    }
  | {
      /**
       * Capture id; defaults to `cap_NNN` (running index over captures).
       */
      id?: string | null;
      type: "capture";
    }
  | {
      /**
       * Duration in seconds, `≥ 0`.
       */
      duration_s: number;
      type: "wait";
    };

/**
 * A motion scenario, version 1.
 */
export interface ScenarioSpec {
  /**
   * Free-text description.
   */
  description?: string | null;
  /**
   * Baking sample period in seconds, `> 0`.
   */
  dt: number;
  /**
   * The generator the steps were made by, kept so it can be edited and
   * run again; the steps are the source of truth.
   */
  generator?: GeneratorSpec | null;
  /**
   * The program, executed in order.
   */
  steps: Step[];
  /**
   * Format version; must be `1`.
   */
  version: number;
}
/**
 * JSON Schema proxy for [`Iso3`] (`nalgebra::Isometry3<f64>`).
 *
 * `nalgebra` does not implement [`schemars::JsonSchema`], so `*Export` and
 * parameter types that embed [`Iso3`] annotate the field with
 * `#[cfg_attr(feature = "schemars", schemars(with = "Iso3Schema"))]`
 * (or `Vec<Iso3Schema>` / `Option<Iso3Schema>`). This proxy mirrors the exact
 * serde wire format of `Isometry3`:
 * `{ "rotation": [qx, qy, qz, qw], "translation": [tx, ty, tz] }`.
 *
 * It exists only to describe that shape to `schemars`; it is never
 * constructed at runtime.
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
/**
 * Azimuths about the pole.
 */
export interface AngleSteps {
  /**
   * Last angle, radians.
   */
  max_rad: number;
  /**
   * First angle, radians.
   */
  min_rad: number;
  /**
   * Number of angles, ≥ 1.
   */
  steps: number;
}
/**
 * Which cameras must see the target, and how much of it.
 */
export interface Visibility {
  /**
   * The cameras; all of the scene's when absent.
   */
  cameras?: string[] | null;
  /**
   * Image border margin, pixels.
   */
  margin_px?: number;
  /**
   * Fraction of the target's points each must see, in `(0, 1]`.
   */
  min_visible_fraction?: number;
}
