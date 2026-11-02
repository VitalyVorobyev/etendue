/**
 * Per-view reprojection residuals of a calibration's first trial: the report's `features`
 * grouped by capture and camera, each view's RMS, and what a camera view draws of them (every
 * observed point, its reprojection and its residual magnified, coloured by size).
 */

import type { MeasurePrimitive } from "@vitavision/stage2d";
import type { MeasureTone } from "@vitavision/ui";

import type { FeatureResidual } from "../kernel/native";

/** One view's residuals: a capture seen by a camera. */
export interface ViewResiduals {
  capture: string;
  camera: string;
  /** Points with a residual. */
  count: number;
  /** Root mean square of the residuals' lengths (px): `√(Σ(du² + dv²) / n)`. */
  rms: number;
  /** The largest residual's length (px). */
  max: number;
}

/** A residual's length (px): from the observed point to its reprojection. */
export function residualLength(f: FeatureResidual): number {
  return Math.hypot(f.projected_px[0] - f.observed_px[0], f.projected_px[1] - f.observed_px[1]);
}

/** A key naming one view. */
export const viewKey = (capture: string, camera: string) => `${capture}\u0000${camera}`;

/** The views of `features`, in the order they first appear (the report's: view-major). */
export function groupViews(features: readonly FeatureResidual[]): ViewResiduals[] {
  const views = new Map<string, { capture: string; camera: string; count: number; sum: number; max: number }>();
  for (const f of features) {
    const key = viewKey(f.capture, f.camera);
    let v = views.get(key);
    if (!v) {
      v = { capture: f.capture, camera: f.camera, count: 0, sum: 0, max: 0 };
      views.set(key, v);
    }
    const e = residualLength(f);
    v.count++;
    v.sum += e * e;
    v.max = Math.max(v.max, e);
  }
  return [...views.values()].map(({ capture, camera, count, sum, max }) => ({
    capture,
    camera,
    count,
    rms: Math.sqrt(sum / count),
    max,
  }));
}

/** The residuals of one view. */
export function viewFeatures(features: readonly FeatureResidual[], capture: string, camera: string): FeatureResidual[] {
  return features.filter((f) => f.capture === capture && f.camera === camera);
}

/** The RMS of every residual's length (px). */
export function overallRms(features: readonly FeatureResidual[]): number {
  if (features.length === 0) return 0;
  let sum = 0;
  for (const f of features) sum += residualLength(f) ** 2;
  return Math.sqrt(sum / features.length);
}

/** A residual's size against the run's RMS: within it, up to twice it, or beyond. */
export type ResidualSize = "small" | "medium" | "large";

/** The size of a residual of `length` against `rms` (the first trial's, over every point). */
export function residualSize(length: number, rms: number): ResidualSize {
  if (!(rms > 0) || length <= rms) return "small";
  return length <= 2 * rms ? "medium" : "large";
}

/** One residual as drawn: its observed point, its reprojection, and the magnified vector. */
export interface ResidualMark {
  observed: [number, number];
  projected: [number, number];
  /** The end of the magnified vector: `observed + scale · (projected − observed)`. */
  tip: [number, number];
  size: ResidualSize;
}

/**
 * What a view draws of its residuals at magnification `scale`: each observed point, its
 * reprojection, and the vector from the observed point towards the reprojection times `scale`,
 * sized against `rms`.
 */
export function residualMarks(features: readonly FeatureResidual[], scale: number, rms: number): ResidualMark[] {
  return features.map((f) => {
    const [ox, oy] = f.observed_px;
    const [px, py] = f.projected_px;
    return {
      observed: [ox, oy],
      projected: [px, py],
      tip: [ox + scale * (px - ox), oy + scale * (py - oy)],
      size: residualSize(Math.hypot(px - ox, py - oy), rms),
    };
  });
}

/** The colour of each size: within the RMS, up to twice it, beyond. */
export const SIZE_TONE: Record<ResidualSize, MeasureTone> = { small: "normal", medium: "warn", large: "defect" };

/**
 * `marks` as overlay primitives: each reprojection a small muted cross, each size's vectors one
 * set of segments, and each observed point a dot in its size's colour, on top.
 */
export function residualPrimitives(marks: readonly ResidualMark[]): MeasurePrimitive[] {
  const vectors: Record<ResidualSize, number[]> = { small: [], medium: [], large: [] };
  const out: MeasurePrimitive[] = [];
  for (const m of marks) {
    vectors[m.size].push(m.observed[0], m.observed[1], m.tip[0], m.tip[1]);
    out.push({ kind: "point", x: m.projected[0], y: m.projected[1], cross: true, tone: "muted" });
  }
  for (const size of ["small", "medium", "large"] as const) {
    if (vectors[size].length > 0) out.push({ kind: "segments", points: vectors[size], tone: SIZE_TONE[size] });
  }
  for (const m of marks) {
    out.push({ kind: "point", x: m.observed[0], y: m.observed[1], radius: 2.5, tone: SIZE_TONE[m.size] });
  }
  return out;
}
