/**
 * A calibration run's parameter statistics over trials: each parameter's per-trial values (for a
 * camera's intrinsics, distortion and tilt, read from each trial's estimate as `param_stats`
 * reads them), their histogram, and how their spread compares with calibration-rs's predicted σ.
 */

import type { CalibrationReport } from "../kernel/native";

/** The parameters of a camera that each trial's estimate holds, by their `param_stats` name. */
const CAMERA_PARAMS: Record<string, (c: CalibrationReport["estimate"]["cameras"][number]) => number | undefined> = {
  fx: (c) => c.intrinsics[0],
  fy: (c) => c.intrinsics[1],
  cx: (c) => c.intrinsics[2],
  cy: (c) => c.intrinsics[3],
  k1: (c) => c.distortion[0],
  k2: (c) => c.distortion[1],
  k3: (c) => c.distortion[2],
  p1: (c) => c.distortion[3],
  p2: (c) => c.distortion[4],
  tilt_x: (c) => (c.tilt[0] * 180) / Math.PI,
  tilt_y: (c) => (c.tilt[1] * 180) / Math.PI,
};

/**
 * Each trial's value of the parameter `label` (`cam_left fx`, `cam_right k1`, …), in the units of
 * its statistics; `null` for a parameter the estimates do not hold directly (the hand-eye's,
 * which are errors in its tangent).
 */
export function trialValues(reports: readonly CalibrationReport[], label: string): number[] | null {
  const at = label.lastIndexOf(" ");
  if (at < 0) return null;
  const [camera, name] = [label.slice(0, at), label.slice(at + 1)];
  const read = CAMERA_PARAMS[name];
  if (!read) return null;
  const values: number[] = [];
  for (const r of reports) {
    const c = r.estimate.cameras.find((e) => e.id === camera);
    const v = c ? read(c) : undefined;
    if (v !== undefined && Number.isFinite(v)) values.push(v);
  }
  return values.length > 0 ? values : null;
}

/** A histogram: `bins` equal bins over `[lo, hi]`, their counts. */
export interface Bins {
  lo: number;
  hi: number;
  counts: number[];
}

/**
 * `values` in `bins` equal bins over `[lo, hi]` (by default their range, widened to include
 * `include`); a value on a bin edge goes into the upper bin, `hi` into the last. A range of
 * zero width is widened symmetrically so the values sit in the middle.
 */
export function binValues(values: readonly number[], bins: number, include: readonly number[] = []): Bins {
  const all = [...values, ...include].filter((v) => Number.isFinite(v));
  let lo = Math.min(...all);
  let hi = Math.max(...all);
  if (all.length === 0) [lo, hi] = [0, 1];
  if (hi <= lo) {
    const pad = lo === 0 ? 1 : Math.abs(lo) * 1e-6;
    [lo, hi] = [lo - pad, hi + pad];
  }
  const n = Math.max(1, Math.round(bins));
  const counts = Array.from({ length: n }, () => 0);
  const width = (hi - lo) / n;
  for (const v of values) {
    if (!Number.isFinite(v) || v < lo || v > hi) continue;
    counts[Math.min(n - 1, Math.floor((v - lo) / width))]!++;
  }
  return { lo, hi, counts };
}

/**
 * How a parameter's spread over trials compares with calibration-rs's predicted σ: `ok` with a
 * ratio std/σ within `[0.7, 1.4]`, `low` or `high` outside it, `null` without a σ. With few
 * trials the ratio itself is uncertain (about 1/√(2(n − 1)) relative).
 */
export type SigmaAgreement = "ok" | "low" | "high";

export const SIGMA_RATIO = { low: 0.7, high: 1.4 } as const;

export function sigmaAgreement(std: number, sigma: number | undefined): SigmaAgreement | null {
  if (sigma === undefined || !(sigma > 0) || !Number.isFinite(std)) return null;
  const ratio = std / sigma;
  if (ratio < SIGMA_RATIO.low) return "low";
  return ratio > SIGMA_RATIO.high ? "high" : "ok";
}
