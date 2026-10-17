import type { BakedScenario, ManifestJoint } from "@etendue/wasm";
import { LineChart, type Series } from "@vitavision/charts";
import type { Playhead } from "@vitavision/workbench";
import { useMemo } from "react";

import { useThrottledPlayhead } from "../kernel/useThrottled";

const MAX_POINTS = 400;
const DEG = 180 / Math.PI;

/** What the chart plots per joint. */
export type JointQuantity = "position" | "velocity" | "acceleration";

const PLURAL: Record<JointQuantity, string> = {
  position: "positions",
  velocity: "velocities",
  acceleration: "accelerations",
};

/**
 * One robot's joints over the scenario, with the playhead as a vertical rule: positions in
 * degrees, or velocities and accelerations as a percentage of each joint's limit (±100 % is the
 * limit, drawn as rules). Prismatic joints would be metres — none in the robot library.
 */
export function JointChart({
  baked,
  robot,
  joints,
  quantity,
  playhead,
  height,
}: {
  baked: BakedScenario;
  robot: string;
  /** The robot's manifest joints, for the limits. */
  joints: readonly ManifestJoint[];
  quantity: JointQuantity;
  playhead: Playhead;
  /** Plot height in CSS pixels; the chart then takes its container's width. */
  height?: number | undefined;
}) {
  const r = baked.robots.findIndex((x) => x.id === robot);
  const series = useMemo<Series[]>(() => {
    if (r < 0) return [];
    const stride = Math.max(1, Math.ceil(baked.samples.length / MAX_POINTS));
    return baked.robots[r]!.joint_names.map((name, j) => {
      const limit = joints[j];
      const value = (k: number): number | null => {
        const s = baked.samples[k]!;
        if (quantity === "position") return s.joint_positions[r]![j]! * DEG;
        const values = quantity === "velocity" ? s.joint_velocities : s.joint_accelerations;
        const max = quantity === "velocity" ? limit?.max_velocity : limit?.max_acceleration;
        const v = values?.[r]?.[j];
        return v === undefined || max === undefined ? null : (100 * v) / max;
      };
      const points: { x: number; y: number }[] = [];
      // Keep each stride's extreme, so a short peak is not lost between plotted samples.
      for (let k = 0; k < baked.samples.length; k += stride) {
        let best: number | null = null;
        for (let i = k; i < Math.min(k + stride, baked.samples.length); i++) {
          const v = value(i);
          if (v !== null && (best === null || Math.abs(v) > Math.abs(best))) best = v;
        }
        if (best !== null) points.push({ x: baked.samples[k]!.t, y: best });
      }
      return { name, points };
    });
  }, [baked, r, joints, quantity]);
  const k = useThrottledPlayhead(playhead, 100);
  const t = baked.samples[Math.min(k, baked.samples.length - 1)]?.t ?? 0;
  if (r < 0) return null;
  const relative = quantity !== "position";
  return (
    <LineChart
      series={series}
      label={`Joint ${PLURAL[quantity]} of ${robot}`}
      xLabel="t (s)"
      yLabel={relative ? `${quantity === "velocity" ? "q̇" : "q̈"} (% of limit)` : "q (°)"}
      {...(relative ? { yDomain: [-105, 105] as [number, number] } : {})}
      variant={height === undefined ? "panel" : "fluid"}
      height={height}
      className="h-full"
      underlay={(x, y) => (
        <>
          {relative &&
            [-100, 100].map((v) => (
              <line
                key={v}
                x1={x.project(x.domain[0])}
                x2={x.project(x.domain[1])}
                y1={y.project(v)}
                y2={y.project(v)}
                className="stroke-warn"
                strokeDasharray="4 3"
                strokeWidth={1}
              />
            ))}
          <line
            x1={x.project(t)}
            x2={x.project(t)}
            y1={y.project(y.domain[0])}
            y2={y.project(y.domain[1])}
            className="stroke-signal"
            strokeWidth={1.5}
            data-testid="joint-chart-playhead"
          />
        </>
      )}
    />
  );
}
