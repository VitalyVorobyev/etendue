/**
 * DO NOT EDIT: generated from schemas/*.schema.json by `bun run generate:types` (in web/).
 * The source of truth is the `etendue-scene` Rust types.
 */

/**
 * A scenario sampled at a fixed period.
 */
export interface BakedScenario {
  /**
   * Sample period in seconds.
   */
  dt: number;
  /**
   * Frame names, topological order, `"world"` first. `world_se3_frame`
   * in every sample is index-aligned with this list.
   */
  frames: string[];
  /**
   * Robots whose joint positions each sample records, in scene order.
   */
  robots: BakedRobot[];
  /**
   * Samples at `t = k · dt`.
   */
  samples: BakedSample[];
  /**
   * Format version; `1`.
   */
  version: number;
}
/**
 * A robot's joint naming, for [`BakedSample::joint_positions`].
 */
export interface BakedRobot {
  /**
   * Robot id.
   */
  id: string;
  /**
   * Joint names in `q` order.
   */
  joint_names: string[];
}
/**
 * One sample.
 */
export interface BakedSample {
  /**
   * Set on the stationary sample at which images are captured.
   */
  capture?: CaptureEvent | null;
  /**
   * Joint positions per robot, aligned with [`BakedScenario::robots`].
   * For display only.
   */
  joint_positions: number[][];
  /**
   * Time in seconds.
   */
  t: number;
  /**
   * Pose of every frame in the world, aligned with
   * [`BakedScenario::frames`].
   */
  world_se3_frame: Iso3Schema[];
}
/**
 * A capture marker.
 */
export interface CaptureEvent {
  /**
   * Capture id (unique within the scenario).
   */
  id: string;
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
