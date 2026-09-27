/**
 * DO NOT EDIT: generated from schemas/*.schema.json by `bun run generate:types` (in web/).
 * The source of truth is the `etendue-scene` Rust types.
 */

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
       * Fraction of the joint velocity and acceleration limits, in
       * `(0, 1]`.
       */
      speed_scale?: number;
      type: "ptp_joints";
    }
  | {
      base_se3_tool: Iso3Schema;
      /**
       * Robot id.
       */
      robot: string;
      /**
       * Fraction of the joint velocity and acceleration limits, in
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
       * Fraction of the joint velocity and acceleration limits, in
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
   * The program, executed in order.
   */
  steps: Step[];
  /**
   * Format version; must be `1`.
   */
  version: number;
}
/**
 * Target pose of the TCP link in the robot base frame.
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
