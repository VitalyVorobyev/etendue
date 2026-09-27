/**
 * What the camera view draws of a target, in the target frame (metres, the `z = 0` plane):
 * its outline and, for a checkerboard, the square grid — the same layout
 * `@vitavision/three`'s `TargetBoard` draws (columns along X, centred). Polylines are
 * densely sampled so their projections show lens distortion.
 */

import type { TargetSpec } from "@etendue/wasm";

/** Squares of a board, columns along X; `null` for a plain surface or an unknown layout. */
export function checkerOf(target: TargetSpec): { cols: number; rows: number } | null {
  const g = target.geometry;
  if (g.type !== "board") return null;
  const b = g.board;
  if (b.kind === "chessboard") return { cols: b.cols + 1, rows: b.rows + 1 };
  if (b.kind === "charuco") return { cols: b.cols, rows: b.rows };
  return null;
}

/** Polylines on the target, each a flat `[x0, y0, 0, x1, …]` array in the target frame. */
export function targetPolylines(
  extent: readonly [number, number],
  checker: { cols: number; rows: number } | null,
  samplesPerLine = 24,
): Float64Array[] {
  const [w, h] = extent;
  const line = (x0: number, y0: number, x1: number, y1: number) => {
    const out = new Float64Array((samplesPerLine + 1) * 3);
    for (let i = 0; i <= samplesPerLine; i++) {
      const s = i / samplesPerLine;
      out.set([x0 + (x1 - x0) * s, y0 + (y1 - y0) * s, 0], 3 * i);
    }
    return out;
  };
  const lines: Float64Array[] = [];
  const cols = checker?.cols ?? 1;
  const rows = checker?.rows ?? 1;
  for (let c = 0; c <= cols; c++) {
    const x = -w / 2 + (w * c) / cols;
    lines.push(line(x, -h / 2, x, h / 2));
  }
  for (let r = 0; r <= rows; r++) {
    const y = -h / 2 + (h * r) / rows;
    lines.push(line(-w / 2, y, w / 2, y));
  }
  return lines;
}
