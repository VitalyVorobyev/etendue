import type { BakedScenario } from "@etendue/wasm";
import { LineChart, type Series } from "@vitavision/charts";
import type { Playhead } from "@vitavision/workbench";
import { useMemo } from "react";

import { useThrottledPlayhead } from "../kernel/useThrottled";

const MAX_POINTS = 400;
const DEG = 180 / Math.PI;

/**
 * Joint positions of one robot over the scenario (degrees; prismatic joints would be metres —
 * none in the current robot set), with the playhead as a vertical rule.
 */
export function JointChart({ baked, robot, playhead }: { baked: BakedScenario; robot: string; playhead: Playhead }) {
  const r = baked.robots.findIndex((x) => x.id === robot);
  const series = useMemo<Series[]>(() => {
    if (r < 0) return [];
    const stride = Math.max(1, Math.ceil(baked.samples.length / MAX_POINTS));
    return baked.robots[r]!.joint_names.map((name, j) => {
      const points: { x: number; y: number }[] = [];
      for (let k = 0; k < baked.samples.length; k += stride) {
        const s = baked.samples[k]!;
        points.push({ x: s.t, y: s.joint_positions[r]![j]! * DEG });
      }
      return { name, points };
    });
  }, [baked, r]);
  const k = useThrottledPlayhead(playhead, 100);
  const t = baked.samples[Math.min(k, baked.samples.length - 1)]?.t ?? 0;
  if (r < 0) return null;
  return (
    <LineChart
      series={series}
      label={`Joint positions of ${robot}`}
      xLabel="t (s)"
      yLabel="q (°)"
      variant="panel"
      className="h-full"
      underlay={(x, y) => (
        <line
          x1={x.project(t)}
          x2={x.project(t)}
          y1={y.project(y.domain[0])}
          y2={y.project(y.domain[1])}
          className="stroke-signal"
          strokeWidth={1.5}
          data-testid="joint-chart-playhead"
        />
      )}
    />
  );
}
