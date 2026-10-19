/**
 * The head against range along its laser: the laser line on the sensor, or charts of the blur
 * and stripe width (px) and of the resolution (µm per px) from the window's near end to its far
 * end, on the line's centre and the worst across it.
 */

import type { HeadMap } from "@etendue/wasm";
import { LineChart, type Scale, type Series } from "@vitavision/charts";
import { SegmentedControl } from "@vitavision/ui";
import { type ReactNode, useMemo, useState } from "react";

import { profile } from "./profile";
import { SensorStripes } from "./SensorStripes";

type View = "sensor" | "blur" | "resolution";

const VIEWS: { value: View; label: string }[] = [
  { value: "sensor", label: "Sensor" },
  { value: "blur", label: "Blur" },
  { value: "resolution", label: "Resolution" },
];

/** The blur limit across the chart, when it is in the y range. */
const limit =
  (px: number) =>
  (x: Scale, y: Scale): ReactNode =>
    px >= y.domain[0] && px <= y.domain[1] ? (
      <line
        x1={x.project(x.domain[0])}
        x2={x.project(x.domain[1])}
        y1={y.project(px)}
        y2={y.project(px)}
        className="stroke-signal"
        strokeDasharray="4 3"
        strokeWidth={1}
      />
    ) : null;

const line = (x: number[], y: number[], scale = 1) => x.map((r, j) => ({ x: r * 1000, y: y[j]! * scale }));

export function RangeViews({
  map,
  range,
  resolution,
  blurLimitPx,
  focus,
}: {
  map: HeadMap;
  /** The window along the laser (m). */
  range: [number, number];
  resolution: [number, number];
  blurLimitPx: number;
  /** Where the laser is focused, along it (m), when its waist is known. */
  focus: number | null;
}) {
  const [view, setView] = useState<View>("sensor");
  const p = useMemo(() => profile(map, range), [map, range]);
  const stripe = p.stripe_px.some(Number.isFinite);
  const series = useMemo<Series[]>(() => {
    if (view === "blur")
      return [
        { name: "blur, line centre", points: line(p.range_m, p.blur_px) },
        { name: "blur, worst", points: line(p.range_m, p.worst_blur_px) },
        ...(stripe
          ? [
              { name: "stripe, line centre", points: line(p.range_m, p.stripe_px) },
              { name: "stripe, worst", points: line(p.range_m, p.worst_stripe_px) },
            ]
          : []),
      ];
    return [
      { name: "depth / px, line centre", points: line(p.range_m, p.depth_m_per_px, 1e6) },
      { name: "depth / px, worst", points: line(p.range_m, p.worst_depth_m_per_px, 1e6) },
      { name: "lateral / px, line centre", points: line(p.range_m, p.lateral_m_per_px, 1e6) },
      { name: "lateral / px, worst", points: line(p.range_m, p.worst_lateral_m_per_px, 1e6) },
    ];
  }, [view, p, stripe]);
  const inWindow = focus !== null && focus >= range[0] && focus <= range[1];
  return (
    <div className="flex h-full min-h-0 flex-col gap-2 p-2" data-testid="range-views">
      <SegmentedControl
        aria-label="Along the laser"
        value={view}
        options={VIEWS}
        onValueChange={(v) => {
          const next = VIEWS.find((x) => x.value === v);
          if (next) setView(next.value);
        }}
      />
      <div className="min-h-0 flex-1">
        {view === "sensor" ? (
          <SensorStripes map={map} range={range} resolution={resolution} />
        ) : (
          <LineChart
            series={series}
            label={view === "blur" ? "Blur and stripe width against range" : "Resolution against range"}
            xLabel="range along the laser (mm)"
            yLabel={view === "blur" ? "px" : "µm / px"}
            variant="fluid"
            className="h-full"
            markers={inWindow ? [{ position: focus * 1000, tone: "defect", label: "laser focus" }] : undefined}
            {...(view === "blur" ? { underlay: limit(blurLimitPx) } : {})}
          />
        )}
      </div>
    </div>
  );
}
