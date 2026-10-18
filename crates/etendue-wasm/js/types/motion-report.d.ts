/**
 * DO NOT EDIT: generated from schemas/*.schema.json by `bun run generate:types` (in web/).
 * The source of truth is the `etendue-scene` Rust types.
 */

/**
 * How a `ptp_pose` step's configuration was found.
 */
export type IkReport =
  | {
      source: "stored";
    }
  | {
      /**
       * `0`: converged from the current configuration; `k`: on restart
       * `k`, from a seed elsewhere in the joint space — likely another IK
       * branch, so the arm may swing far.
       */
      attempt: number;
      /**
       * Iterations, all attempts.
       */
      iterations: number;
      source: "solved";
    };
/**
 * Something about a step that deserves a look, tagged on `type`.
 */
export type MotionWarning =
  | {
      /**
       * The restart that converged.
       */
      attempt: number;
      type: "ik_restart_used";
    }
  | {
      /**
       * Joint name.
       */
      joint: string;
      /**
       * Its smallest distance from a limit (rad or m).
       */
      margin: number;
      type: "near_limit";
    }
  | {
      /**
       * The smallest singular value.
       */
      sigma: number;
      type: "near_singular";
    }
  | {
      /**
       * The largest change (rad or m).
       */
      jump: number;
      type: "large_joint_jump";
    };

/**
 * Feasibility of every step of a scenario, version 1.
 */
export interface MotionReport {
  /**
   * Number of captures.
   */
  captures: number;
  /**
   * Total duration, seconds.
   */
  duration_s: number;
  /**
   * One entry per scenario step, in order.
   */
  steps: StepReport[];
  /**
   * Format version; `1`.
   */
  version: number;
}
/**
 * One step of the program.
 */
export interface StepReport {
  /**
   * The joint and quantity closest to its limit: what sets the step's
   * duration.
   */
  binding?: Binding | null;
  /**
   * Duration, seconds (`0` for a motion that does not move).
   */
  duration_s: number;
  /**
   * How a `ptp_pose` found its joint configuration.
   */
  ik?: IkReport | null;
  /**
   * The step's `type` (`ptp_joints`, `ptp_pose`, `lin`, `capture`, `wait`).
   */
  kind: string;
  /**
   * For a `lin`: the largest joint change between consecutive IK
   * waypoints (rad or m).
   */
  max_joint_jump?: number | null;
  /**
   * Smallest distance of any joint from its position limits over the
   * step (rad or m).
   */
  min_limit_margin?: number | null;
  /**
   * Smallest singular value of the 6×n TCP Jacobian over the step
   * (translation rows in m/rad, rotation rows in rad/rad): near zero at
   * a singularity.
   */
  min_sigma?: number | null;
  /**
   * Per joint of the moving robot: the largest |velocity|, |acceleration|
   * and |jerk| over the step as fractions of the joint's limits.
   */
  peaks?: JointPeak[];
  /**
   * The robot it moves, for a motion.
   */
  robot?: string | null;
  /**
   * Index into the scenario's `steps`.
   */
  step: number;
  /**
   * Start time, seconds.
   */
  t0: number;
  /**
   * What deserves a look.
   */
  warnings?: MotionWarning[];
}
/**
 * Which limit binds a step.
 */
export interface Binding {
  /**
   * Its peak as a fraction of the limit.
   */
  fraction: number;
  /**
   * Joint name.
   */
  joint: string;
  /**
   * The limited quantity.
   */
  quantity: "velocity" | "acceleration" | "jerk";
}
/**
 * Peak use of one joint's limits over a step, as fractions of the limits
 * (`1` = at the limit; `speed_scale` keeps them at or below it).
 */
export interface JointPeak {
  /**
   * max |q̈| / acceleration limit.
   */
  acceleration: number;
  /**
   * max |q⃛| / jerk limit, if the joint has one.
   */
  jerk?: number | null;
  /**
   * Joint name.
   */
  joint: string;
  /**
   * max |q̇| / velocity limit.
   */
  velocity: number;
}
