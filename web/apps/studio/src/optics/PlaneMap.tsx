import type { HeadMap } from "@etendue/wasm";
import { SegmentedControl } from "@vitavision/ui";
import { useEffect, useMemo, useRef, useState } from "react";

import { stripeWidth } from "./analysis";

/** What the map shows. */
export type Metric = "blur" | "stripe" | "depth" | "lateral" | "triangulation";

const METRICS: { value: Metric; label: string; unit: string; of: (m: HeadMap, i: number) => number }[] = [
  { value: "blur", label: "Blur", unit: "px", of: (m, i) => m.blur_px[i]! },
  { value: "stripe", label: "Stripe", unit: "px", of: stripeWidth },
  { value: "depth", label: "dZ/px", unit: "µm", of: (m, i) => m.depth_m_per_px[i]! * 1e6 },
  { value: "lateral", label: "Lateral/px", unit: "µm", of: (m, i) => m.lateral_m_per_px[i]! * 1e6 },
  { value: "triangulation", label: "Angle", unit: "°", of: (m, i) => (m.triangulation_rad[i]! * 180) / Math.PI },
];

/** Viridis, sampled: perceptually even, readable for colour-blind viewers. */
const VIRIDIS: [number, number, number][] = [
  [68, 1, 84],
  [72, 40, 120],
  [62, 74, 137],
  [49, 104, 142],
  [38, 130, 142],
  [31, 158, 137],
  [53, 183, 121],
  [109, 205, 89],
  [180, 222, 44],
  [253, 231, 37],
];

function colour(t: number): [number, number, number] {
  const x = Math.min(1, Math.max(0, t)) * (VIRIDIS.length - 1);
  const i = Math.min(VIRIDIS.length - 2, Math.floor(x));
  const f = x - i;
  const [a, b] = [VIRIDIS[i]!, VIRIDIS[i + 1]!];
  return [a[0] + f * (b[0] - a[0]), a[1] + f * (b[1] - a[1]), a[2] + f * (b[2] - a[2])];
}

const fmt = (x: number) => (Math.abs(x) >= 100 ? x.toFixed(0) : Math.abs(x) >= 10 ? x.toFixed(1) : x.toFixed(2));

/**
 * The laser plane face on, over the head's window: across the line (horizontal) by range along
 * the laser (vertical, the far end up). Each sample is coloured by the chosen metric; samples the
 * camera does not see are hatched; the blur limit is drawn as a contour.
 */
export function PlaneMap({
  map,
  range,
  width,
  blurLimitPx,
}: {
  map: HeadMap;
  /** The window along the laser (m). */
  range: [number, number];
  /** Its width along the line (m). */
  width: number;
  blurLimitPx: number;
}) {
  const [metric, setMetric] = useState<Metric>("blur");
  const [hover, setHover] = useState<{ a: number; b: number; value: number | null } | null>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  // The stripe's width needs the beam's waist.
  const metrics = useMemo(
    () => (map.beam_px.some(Number.isFinite) ? METRICS : METRICS.filter((m) => m.value !== "stripe")),
    [map],
  );
  const spec = metrics.find((m) => m.value === metric) ?? METRICS[0]!;
  const { across, along } = map;
  const [lo, hi] = useMemo(() => {
    let [min, max] = [Infinity, -Infinity];
    for (let i = 0; i < across * along; i++) {
      if (!map.visible[i]) continue;
      const v = spec.of(map, i);
      if (Number.isFinite(v)) [min, max] = [Math.min(min, v), Math.max(max, v)];
    }
    return Number.isFinite(min) ? [min, max > min ? max : min + 1e-9] : [0, 1];
  }, [map, spec, across, along]);

  useEffect(() => {
    const c = canvas.current;
    const ctx = c?.getContext("2d");
    if (!c || !ctx) return;
    c.width = across;
    c.height = along;
    const image = ctx.createImageData(across, along);
    for (let j = 0; j < along; j++) {
      for (let i = 0; i < across; i++) {
        const k = j * across + i;
        // The nearest range at the bottom.
        const o = ((along - 1 - j) * across + i) * 4;
        if (!map.visible[k]) {
          const stripe = (i + j) % 4 < 2;
          image.data.set(stripe ? [150, 150, 150, 90] : [150, 150, 150, 40], o);
          continue;
        }
        const [r, g, b] = colour((spec.of(map, k) - lo) / (hi - lo));
        image.data.set([r, g, b, 255], o);
        // The blur limit: where neighbours straddle it.
        const blur = map.blur_px[k]!;
        const right = i + 1 < across ? map.blur_px[k + 1]! : blur;
        const up = j + 1 < along ? map.blur_px[k + across]! : blur;
        if ((blur - blurLimitPx) * (right - blurLimitPx) < 0 || (blur - blurLimitPx) * (up - blurLimitPx) < 0) {
          image.data.set([255, 255, 255, 255], o);
        }
      }
    }
    ctx.putImageData(image, 0, 0);
  }, [map, spec, lo, hi, across, along, blurLimitPx]);

  const point = (e: React.PointerEvent<HTMLCanvasElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    const fx = (e.clientX - r.left) / r.width;
    const fy = 1 - (e.clientY - r.top) / r.height;
    const i = Math.min(across - 1, Math.max(0, Math.round(fx * (across - 1))));
    const j = Math.min(along - 1, Math.max(0, Math.round(fy * (along - 1))));
    const k = j * across + i;
    setHover({
      a: (fx - 0.5) * width,
      b: range[0] + fy * (range[1] - range[0]),
      value: map.visible[k] ? spec.of(map, k) : null,
    });
  };

  return (
    <div className="flex h-full min-h-0 flex-col gap-2 p-2" data-testid="plane-map">
      <div className="flex items-center gap-3">
        <SegmentedControl
          aria-label="Map metric"
          value={spec.value}
          options={metrics.map((m) => ({ value: m.value, label: m.label }))}
          onValueChange={(v) => {
            const m = METRICS.find((x) => x.value === v);
            if (m) setMetric(m.value);
          }}
        />
        <div className="flex min-w-0 flex-1 items-center gap-2 font-mono text-[11px] text-fg-muted tabular-nums">
          <span>{fmt(lo)}</span>
          <div
            className="h-2 min-w-12 flex-1 rounded-sm"
            style={{
              background: `linear-gradient(to right, ${VIRIDIS.map((c) => `rgb(${c.join(",")})`).join(",")})`,
            }}
          />
          <span>
            {fmt(hi)} {spec.unit}
          </span>
        </div>
      </div>
      <div className="relative min-h-0 flex-1">
        <canvas
          ref={canvas}
          className="h-full w-full rounded-sm border border-line"
          style={{ imageRendering: "auto" }}
          aria-label={`${spec.label} over the window`}
          role="img"
          onPointerMove={point}
          onPointerLeave={() => setHover(null)}
        />
        <div className="pointer-events-none absolute bottom-1 left-1 rounded-sm bg-surface/85 px-1.5 py-0.5 font-mono text-[11px] text-fg tabular-nums">
          {hover
            ? `${(hover.a * 1000).toFixed(1)} mm across, ${(hover.b * 1000).toFixed(1)} mm out: ${
                hover.value === null ? "not seen" : `${fmt(hover.value)} ${spec.unit}`
              }`
            : `${(width * 1000).toFixed(0)} mm across × ${(range[0] * 1000).toFixed(0)}–${(range[1] * 1000).toFixed(0)} mm along the laser · white: ${blurLimitPx} px blur`}
        </div>
      </div>
    </div>
  );
}
