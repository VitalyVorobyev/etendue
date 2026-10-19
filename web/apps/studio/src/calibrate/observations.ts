/**
 * The Calibrate workspace's preview of what the calibration observes: the kernel's
 * `observe` of the open scene and scenario, the same ground truth and noise stream as
 * `etendue calibrate`, so the camera view shows the very points a run of that seed uses.
 * Recomputed a moment after the noise stops changing (a laser rig's observations take tens of
 * milliseconds and megabytes).
 */

import type { ObservationNoise, Observations } from "@etendue/wasm";
import { useEffect, useState } from "react";

import type { Baked } from "../kernel/etendue";

const SETTLE_MS = 150;

interface Computed {
  current: Baked;
  noise: ObservationNoise;
  observations: Observations | null;
  error: string | null;
}

/**
 * Observations of `current` with `noise`, or nothing while `noise` is null (or there is no
 * scenario). The previous result stays until the next one is ready, unless the scene changed.
 */
export function useObservations(
  current: Baked | null,
  noise: ObservationNoise | null,
): { observations: Observations | null; error: string | null } {
  const [computed, setComputed] = useState<Computed | null>(null);
  useEffect(() => {
    const scenario = current?.loaded.scenario;
    if (!current || !scenario || !noise) return;
    const timer = setTimeout(() => {
      try {
        const observations = current.session.observe(scenario, noise);
        setComputed({ current, noise, observations, error: null });
      } catch (e) {
        setComputed({ current, noise, observations: null, error: e instanceof Error ? e.message : String(e) });
      }
    }, SETTLE_MS);
    return () => clearTimeout(timer);
  }, [current, noise]);
  if (!current || !noise || computed?.current !== current) return { observations: null, error: null };
  return { observations: computed.observations, error: computed.error };
}
