/**
 * DO NOT EDIT: generated from schemas/*.schema.json by `bun run generate:types` (in web/).
 * The source of truth is the `etendue-scene` Rust types.
 */

/**
 * The pose sequence of one robot: its tool pose and joints at every
 * capture, version 1.
 */
export interface CaptureExport {
  /**
   * The frame tool poses are given in (the manifest's `base_link`).
   */
  base_link: string;
  /**
   * One entry per capture, in time order.
   */
  captures: CaptureRow[];
  /**
   * Joint names, in `q` order.
   */
  joint_names: string[];
  /**
   * Robot id.
   */
  robot: string;
  /**
   * The tool frame (the manifest's `tcp_link`).
   */
  tcp_link: string;
  /**
   * Format version; `1`.
   */
  version: number;
}
/**
 * One capture.
 */
export interface CaptureRow {
  base_se3_tool: Iso3Schema;
  /**
   * Capture id.
   */
  capture: string;
  /**
   * Joint positions (rad or m).
   */
  q: number[];
  /**
   * Time, seconds.
   */
  t: number;
}
/**
 * Tool pose in the robot's base frame.
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
