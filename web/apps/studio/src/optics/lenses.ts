/**
 * The optimiser's last step: every lens of the library on the head, each optimised the same way
 * (tilt, focus and f-number for the smallest worst blur over the window), then ranked: the most
 * of the window seen first, then the smallest worst blur.
 */

import { type LensPart, headOptimise } from "@etendue/wasm";

import { analyse } from "./analysis";
import { type HeadParams, toDesign, withLens, withSetting } from "./design";

export interface LensCandidate {
  /** The lens's library reference and name. */
  ref: string;
  name: string;
  /** The head with this lens at its optimum. */
  params: HeadParams;
  /** Share of the window in view, and the worst blur over it (px). */
  coverage: number;
  worstBlurPx: number;
  /** The coarsest depth resolution over the window (m per px): what a shorter lens pays. */
  worstDepthPerPx: number;
}

/** A lens that does not fit the head (it cannot focus that close, say), and why. */
export interface LensMiss {
  ref: string;
  name: string;
  reason: string;
}

const message = (e: unknown) => (e instanceof Error ? e.message.split("\n")[0]! : String(e));

export function searchLenses(
  p: HeadParams,
  lenses: readonly { ref: string; name: string; lens: LensPart }[],
  tiltFree: boolean,
): { ranked: LensCandidate[]; misses: LensMiss[] } {
  const ranked: LensCandidate[] = [];
  const misses: LensMiss[] = [];
  for (const { ref, name, lens } of lenses) {
    try {
      const q = withLens(p, name, lens);
      const optimum = headOptimise(toDesign(q), {
        grid: [9, 9],
        f_number_range: [q.lens.min_f_number, 22],
        tilt_free: tiltFree,
      });
      const params = withSetting(q, optimum.best);
      const a = analyse(params);
      if (typeof a === "string") {
        misses.push({ ref, name, reason: a });
        continue;
      }
      ranked.push({
        ref,
        name,
        params,
        coverage: a.stats.coverage,
        worstBlurPx: a.stats.worstBlurPx,
        worstDepthPerPx: a.stats.depthPerPx[1],
      });
    } catch (e) {
      misses.push({ ref, name, reason: message(e) });
    }
  }
  // Coverage to a tenth of a percent: a sample at the window's edge does not outrank a sharper lens.
  const seen = (c: LensCandidate) => Math.round(c.coverage * 1000);
  ranked.sort((a, b) => seen(b) - seen(a) || a.worstBlurPx - b.worstBlurPx);
  return { ranked, misses };
}
