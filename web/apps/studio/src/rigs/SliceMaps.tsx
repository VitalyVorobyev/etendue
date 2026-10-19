/**
 * Two slices through the rig's coverage: the side (x–z through the rig axis) and the top (x–y at
 * the working distance). Each voxel is coloured by how many cameras see it; voxels a pair
 * measures are marked in the laser's colour.
 */

import { useEffect, useRef } from "react";

import type { RigAnalysis } from "./analysis";

/** One colour per count, 1 to 8 views, cool to warm (viridis into plasma), as RGB. */
const VIEW_RGB: [number, number, number][] = [
  [59, 82, 139],
  [33, 145, 140],
  [94, 201, 98],
  [181, 222, 43],
  [253, 231, 37],
  [252, 165, 10],
  [241, 96, 93],
  [225, 51, 66],
];
const css = ([r, g, b]: [number, number, number]) => `rgb(${r},${g},${b})`;
/** One CSS colour per count. */
export const VIEW_COLOURS = VIEW_RGB.map(css);
/** Voxels a (laser, camera) pair measures. */
export const MEASURED = css([228, 87, 46]);

type Slice = { w: number; h: number; at: (u: number, v: number) => number };

function Slice({ slice, label, caption, testId }: { slice: Slice; label: string; caption: string; testId: string }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const { w, h, at } = slice;
  useEffect(() => {
    const c = canvas.current;
    const ctx = c?.getContext("2d");
    if (!c || !ctx) return;
    c.width = w;
    c.height = h;
    ctx.clearRect(0, 0, w, h);
    for (let v = 0; v < h; v++) {
      for (let u = 0; u < w; u++) {
        const code = at(u, v);
        if (code === 0) continue;
        const count = code & 0xff;
        const lit = code >> 8;
        ctx.fillStyle = lit > 0 ? MEASURED : VIEW_COLOURS[Math.min(VIEW_COLOURS.length, count) - 1]!;
        ctx.fillRect(u, v, 1, 1);
      }
    }
  }, [w, h, at]);
  return (
    <figure className="flex min-h-0 min-w-0 flex-col gap-1">
      <figcaption className="flex items-baseline justify-between gap-2 text-xs">
        <span className="font-medium">{label}</span>
        <span className="truncate text-fg-muted">{caption}</span>
      </figcaption>
      <div className="relative min-h-0 flex-1 rounded border border-line bg-surface-sunken">
        <canvas
          ref={canvas}
          data-testid={testId}
          aria-label={`${label}: ${caption}`}
          className="absolute inset-0 h-full w-full object-contain"
          style={{ imageRendering: "pixelated" }}
        />
      </div>
    </figure>
  );
}

export function SliceMaps({ analysis }: { analysis: RigAnalysis }) {
  const { grid, coverage, layout } = analysis;
  const [nx, ny, nz] = grid.n;
  const code = (i: number, j: number, k: number) => {
    const index = i + nx * (j + ny * k);
    return coverage.views[index]! | (coverage.lit[index]! << 8);
  };
  const dz = (grid.max[2] - grid.min[2]) / nz;
  const kWork = Math.min(nz - 1, Math.max(0, Math.floor((layout.working_distance_m - grid.min[2]) / dz)));
  const jMid = Math.floor(ny / 2);
  const side: Slice = { w: nx, h: nz, at: (u, v) => code(u, jMid, v) };
  const top: Slice = { w: nx, h: ny, at: (u, v) => code(u, ny - 1 - v, kWork) };
  const mm = (m: number) => `${(m * 1000).toFixed(0)} mm`;
  return (
    <div className="grid h-full min-h-0 grid-cols-2 gap-3 p-3" data-testid="rig-slices">
      <Slice
        slice={side}
        label="Side"
        caption={`x across, ${mm(grid.min[2])} (top) to ${mm(grid.max[2])} down the axis`}
        testId="rig-slice-side"
      />
      <Slice
        slice={top}
        label="Top"
        caption={`x–y at ${mm(grid.min[2] + (kWork + 0.5) * dz)}`}
        testId="rig-slice-top"
      />
    </div>
  );
}

/** The legend: a swatch per view count up to the rig's cameras, and the measured mark. */
export function CoverageLegend({ cameras, pairs }: { cameras: number; pairs: number }) {
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-fg-muted">
      {Array.from({ length: Math.min(cameras, VIEW_COLOURS.length) }, (_, i) => (
        <span key={i} className="inline-flex items-center gap-1">
          <span className="inline-block size-2.5 rounded-sm" style={{ background: VIEW_COLOURS[i] }} />
          {i + 1} {i === 0 ? "view" : "views"}
        </span>
      ))}
      {pairs > 0 && (
        <span className="inline-flex items-center gap-1">
          <span className="inline-block size-2.5 rounded-sm" style={{ background: MEASURED }} />
          measured
        </span>
      )}
    </div>
  );
}
