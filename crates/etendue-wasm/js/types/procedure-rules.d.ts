/**
 * DO NOT EDIT: generated from schemas/*.schema.json by `bun run generate:types` (in web/).
 * The source of truth is the `etendue-scene` Rust types.
 */

/**
 * A calibration-rs workflow (`vision-calibration-pipeline`'s problem modules).
 */
export type Problem =
  | "planar_intrinsics"
  | "rig_extrinsics"
  | "single_cam_handeye"
  | "rig_handeye"
  | "laserline_device"
  | "rig_handeye_laserline";

/**
 * The rules of procedures, as one document: what an editor needs to offer only what a stage
 * takes ([`rules`]).
 */
export interface ProcedureRules {
  /**
   * The workflow that calibrates a dataset of each shape in one stage
   * ([`Problem::for_dataset`]).
   */
  by_shape: ShapeRule[];
  /**
   * Per workflow, in [`Problem::ALL`]'s order.
   */
  problems: ProblemRules[];
  /**
   * Per seed slot, the kinds of seed it takes (`nominal`, `truth`, `stage`).
   */
  seed_kinds: SeedSlotRules[];
}
/**
 * The one-stage workflow of a dataset's shape.
 */
export interface ShapeRule {
  /**
   * With lasers.
   */
  lasers: boolean;
  problem: Problem;
  /**
   * More than one camera.
   */
  rig: boolean;
}
/**
 * One workflow's rules.
 */
export interface ProblemRules {
  /**
   * The options ([`Freedoms`] fields) it takes.
   */
  freedoms: string[];
  /**
   * Solves a hand-eye (needs robot poses).
   */
  handeye: boolean;
  /**
   * Needs lasers.
   */
  laser: boolean;
  problem: Problem;
  /**
   * Calibrates several cameras at once.
   */
  rig: boolean;
  /**
   * Its seed slots for an untilted camera model, and for a tilted one.
   */
  seed_slots: string[];
  seed_slots_tilted: string[];
}
/**
 * One seed slot's kinds.
 */
export interface SeedSlotRules {
  kinds: string[];
  slot: string;
}
