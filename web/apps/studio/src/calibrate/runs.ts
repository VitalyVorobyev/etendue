/**
 * Kept calibration runs (ADR 0009 `runs/<name>/`): how a run's folder is named, and how a run is
 * described in the list of them.
 */

import type { CalibrateOptions } from "../kernel/native";

const two = (n: number) => String(n).padStart(2, "0");

/** The last segment of a path. */
const base = (path: string) =>
  path
    .replace(/[\\/]+$/, "")
    .split(/[\\/]/)
    .at(-1) ?? path;

/** A path compared as the same folder whatever its separators or trailing slash. */
export const samePath = (a: string, b: string) =>
  a.replace(/\\/g, "/").replace(/\/+$/, "") === b.replace(/\\/g, "/").replace(/\/+$/, "");

/**
 * A run folder's name: the dataset folder's, the local time, and what it calibrated, e.g.
 * `closed_loop_ur5e-20261019-181522-noise-s1x8`. Names sort by dataset, then time.
 */
export function runName(dataset: string, options: CalibrateOptions, at: Date): string {
  const when = `${at.getFullYear()}${two(at.getMonth() + 1)}${two(at.getDate())}-${two(at.getHours())}${two(at.getMinutes())}${two(at.getSeconds())}`;
  const trials = (options.trials ?? 1) > 1 ? `x${options.trials}` : "";
  const what = options.detected ? "detected" : options.noise ? `noise-s${options.noise.seed ?? 0}${trials}` : "exact";
  const camera = options.camera ? `-${options.camera}` : "";
  return `${base(dataset)}-${when}-${what}${camera}`.replace(/[^\w.-]+/g, "_");
}

/** What a run calibrated from, for people: "exact", "detected", or the noise and trials. */
export function describeRun(options: CalibrateOptions): string {
  const camera = options.camera ? `, ${options.camera} alone` : "";
  if (options.detected) return `detected${camera}`;
  const n = options.noise;
  if (!n || Object.entries(n).every(([k, v]) => k === "seed" || !v)) return `exact${camera}`;
  const terms = [
    n.pixel_sigma_px ? `σ ${n.pixel_sigma_px} px` : null,
    n.outlier_fraction ? `${Math.round(n.outlier_fraction * 100)} % outliers` : null,
    n.dropout_fraction ? `${Math.round(n.dropout_fraction * 100)} % dropped` : null,
    n.laser_sigma_px ? `laser σ ${n.laser_sigma_px} px` : null,
    n.robot_rot_sigma_rad || n.robot_trans_sigma_m ? "robot noise" : null,
  ].filter((t) => t !== null);
  const trials = (options.trials ?? 1) > 1 ? `, ${options.trials} trials` : "";
  return `${terms.join(", ")}, seed ${n.seed ?? 0}${trials}${camera}`;
}
