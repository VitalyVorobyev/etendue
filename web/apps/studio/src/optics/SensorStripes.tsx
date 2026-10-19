/**
 * The head's sensor, face on: where the laser line images at several ranges across the window,
 * from the near end (blue) to the far end (yellow). Triangulation is the stripe moving across
 * the image as the range changes; how far it moves per millimetre is the depth resolution. Each
 * stripe is labelled with its range and, when the beam's waist is known, its width at the line's
 * centre.
 */

import type { HeadMap } from "@etendue/wasm";

import { stripeWidth } from "./analysis";

/** Ranges drawn, evenly over the window. */
const STRIPES = 7;

/** Viridis at `t` in [0, 1], as the plane map colours. */
const RAMP: [number, number, number][] = [
  [68, 1, 84],
  [59, 82, 139],
  [33, 145, 140],
  [94, 201, 98],
  [253, 231, 37],
];

function ramp(t: number): string {
  const x = Math.min(1, Math.max(0, t)) * (RAMP.length - 1);
  const i = Math.min(RAMP.length - 2, Math.floor(x));
  const f = x - i;
  const [a, b] = [RAMP[i]!, RAMP[i + 1]!];
  return `rgb(${[0, 1, 2].map((c) => Math.round(a[c]! + f * (b[c]! - a[c]!))).join(",")})`;
}

/** The stripe of row `j`: its visible samples' pixels, as an SVG polyline. */
function stripe(map: HeadMap, j: number): { points: string; mid: number | null; end: [number, number] | null } {
  const out: string[] = [];
  let end: [number, number] | null = null;
  for (let i = 0; i < map.across; i++) {
    const k = j * map.across + i;
    const [u, v] = [map.pixel_u[k]!, map.pixel_v[k]!];
    if (!map.visible[k] || !Number.isFinite(u) || !Number.isFinite(v)) continue;
    out.push(`${u.toFixed(1)},${v.toFixed(1)}`);
    end = [u, v];
  }
  const centre = j * map.across + Math.floor(map.across / 2);
  const w = map.visible[centre] ? stripeWidth(map, centre) : NaN;
  return { points: out.join(" "), mid: Number.isFinite(w) ? w : null, end };
}

export function SensorStripes({
  map,
  range,
  resolution,
}: {
  map: HeadMap;
  /** The window along the laser (m). */
  range: [number, number];
  resolution: [number, number];
}) {
  const [w, h] = resolution;
  const rows = Array.from({ length: STRIPES }, (_, n) => Math.round((n * (map.along - 1)) / (STRIPES - 1)));
  const font = Math.max(w, h) / 40;
  return (
    <div className="flex h-full min-h-0 flex-col gap-2 p-2" data-testid="sensor-stripes">
      <div className="font-mono text-[11px] text-fg-muted">
        The laser line on the sensor, {(range[0] * 1000).toFixed(0)}–{(range[1] * 1000).toFixed(0)} mm out
      </div>
      <svg
        className="min-h-0 w-full flex-1 rounded-sm border border-line bg-canvas"
        viewBox={`${-0.02 * w} ${-0.02 * h} ${1.04 * w} ${1.04 * h}`}
        preserveAspectRatio="xMidYMid meet"
        role="img"
        aria-label="The laser line on the sensor at several ranges"
      >
        <rect
          x={0}
          y={0}
          width={w}
          height={h}
          fill="none"
          className="stroke-line-strong"
          vectorEffect="non-scaling-stroke"
        />
        {rows.map((j, n) => {
          const s = stripe(map, j);
          if (s.points === "") return null;
          // The ramp's darkest quarter does not read on the dark sensor.
          const colour = ramp(0.25 + (0.75 * n) / (STRIPES - 1));
          const b = range[0] + (j / Math.max(1, map.along - 1)) * (range[1] - range[0]);
          return (
            <g key={j} data-range-mm={(b * 1000).toFixed(0)}>
              <polyline
                points={s.points}
                fill="none"
                stroke={colour}
                strokeWidth={2}
                vectorEffect="non-scaling-stroke"
              />
              {s.end && (
                <text
                  x={Math.min(s.end[0] + font * 0.4, w - font * 4)}
                  y={s.end[1]}
                  fontSize={font}
                  dominantBaseline="middle"
                  fill={colour}
                  className="font-mono"
                >
                  {(b * 1000).toFixed(0)} mm{s.mid !== null ? ` · ${s.mid.toFixed(1)} px` : ""}
                </text>
              )}
            </g>
          );
        })}
      </svg>
    </div>
  );
}
