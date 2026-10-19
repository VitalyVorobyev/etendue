/**
 * A head's figures against range along its laser, row by row of the map (the nearest range
 * first): on the line's centre, and the worst over the part of the line in view.
 */

import type { HeadMap } from "@etendue/wasm";

import { stripeWidth } from "./analysis";

export interface Profile {
  /** Range along the laser's central ray (m), per row. */
  range_m: number[];
  /** On the line's centre; `NaN` where it is out of view (or the beam's waist is unknown). */
  blur_px: number[];
  stripe_px: number[];
  depth_m_per_px: number[];
  lateral_m_per_px: number[];
  /** The largest over the row's samples in view; `NaN` where none is. */
  worst_blur_px: number[];
  worst_stripe_px: number[];
  worst_depth_m_per_px: number[];
  worst_lateral_m_per_px: number[];
}

/** The profile of `map` over the window `range` (m along the laser). */
export function profile(map: HeadMap, range: [number, number]): Profile {
  const { across, along } = map;
  // The line's centre: its middle sample, or the mean of the middle two.
  const centre = across % 2 === 1 ? [(across - 1) / 2] : [across / 2 - 1, across / 2];
  const of: Record<"blur" | "stripe" | "depth" | "lateral", (k: number) => number> = {
    blur: (k) => map.blur_px[k]!,
    stripe: (k) => stripeWidth(map, k),
    depth: (k) => map.depth_m_per_px[k]!,
    lateral: (k) => map.lateral_m_per_px[k]!,
  };
  const out: Profile = {
    range_m: [],
    blur_px: [],
    stripe_px: [],
    depth_m_per_px: [],
    lateral_m_per_px: [],
    worst_blur_px: [],
    worst_stripe_px: [],
    worst_depth_m_per_px: [],
    worst_lateral_m_per_px: [],
  };
  for (let j = 0; j < along; j++) {
    out.range_m.push(range[0] + (j / Math.max(1, along - 1)) * (range[1] - range[0]));
    const row = (i: number) => j * across + i;
    const mid = (f: (k: number) => number) => {
      const v = centre.map((i) => (map.visible[row(i)] ? f(row(i)) : NaN));
      return v.every(Number.isFinite) ? v.reduce((a, b) => a + b, 0) / v.length : NaN;
    };
    const worst = (f: (k: number) => number) => {
      let w = NaN;
      for (let i = 0; i < across; i++) {
        const v = map.visible[row(i)] ? f(row(i)) : NaN;
        if (Number.isFinite(v) && !(v <= w)) w = v;
      }
      return w;
    };
    out.blur_px.push(mid(of.blur));
    out.stripe_px.push(mid(of.stripe));
    out.depth_m_per_px.push(mid(of.depth));
    out.lateral_m_per_px.push(mid(of.lateral));
    out.worst_blur_px.push(worst(of.blur));
    out.worst_stripe_px.push(worst(of.stripe));
    out.worst_depth_m_per_px.push(worst(of.depth));
    out.worst_lateral_m_per_px.push(worst(of.lateral));
  }
  return out;
}
