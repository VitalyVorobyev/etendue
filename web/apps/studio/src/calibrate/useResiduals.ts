/**
 * The camera view's residual layer: a calibration's first-trial residuals of a camera at a
 * capture, magnified and coloured by size, for `CameraView`'s `residualsAt`.
 */

import type { BakedScenario } from "@etendue/wasm";
import type { MeasurePrimitive } from "@vitavision/stage2d";
import { useCallback, useMemo } from "react";

import type { CalibrationReport, FeatureResidual } from "../kernel/native";
import { overallRms, residualMarks, residualPrimitives, viewKey } from "./residuals";

export type ResidualsAt = (camera: string, k: number) => { marks: MeasurePrimitive[]; points: number } | null;

/**
 * `report`'s residuals of a camera at sample `k` of `baked` times `scale`, or `undefined` when
 * the report has none (no run shown, or a run without per-point residuals). At a capture the
 * view did not observe, nothing is drawn and the count is 0.
 */
export function useResiduals(
  report: CalibrationReport | null,
  baked: BakedScenario | null,
  scale: number,
): ResidualsAt | undefined {
  const features = report?.residuals.features;
  const byView = useMemo(() => {
    const map = new Map<string, FeatureResidual[]>();
    for (const f of features ?? []) {
      const key = viewKey(f.capture, f.camera);
      const list = map.get(key);
      if (list) list.push(f);
      else map.set(key, [f]);
    }
    return map;
  }, [features]);
  const rms = useMemo(() => overallRms(features ?? []), [features]);
  const at = useCallback<ResidualsAt>(
    (camera, k) => {
      const capture = baked?.samples[k]?.capture?.id;
      if (capture === undefined) return null;
      const view = byView.get(viewKey(capture, camera)) ?? [];
      return { marks: residualPrimitives(residualMarks(view, scale, rms)), points: view.length };
    },
    [baked, byView, rms, scale],
  );
  return features && features.length > 0 && baked ? at : undefined;
}
