import type { HeadMap } from "@etendue/wasm";
import { describe, expect, it, vi } from "vitest";

// Only the map's arrays are under test, not the kernel.
vi.mock("@etendue/wasm", () => ({}));

const { profile } = await import("./profile");

/** A 4 × 3 map whose value at (i, j) is `10 j + i`; sample (0, 1) is out of view. */
function map(beam: boolean): HeadMap {
  const [across, along] = [4, 3];
  const v = Float64Array.from({ length: across * along }, (_, k) => 10 * Math.floor(k / across) + (k % across));
  const visible = Uint8Array.from({ length: across * along }, (_, k) => (k === across ? 0 : 1));
  return {
    across,
    along,
    visible,
    blur_px: v,
    beam_px: beam ? v.map(() => 0) : v.map(() => NaN),
    depth_m_per_px: v.map((x) => x * 1e-6),
    lateral_m_per_px: v.map((x) => x * 2e-6),
  } as unknown as HeadMap;
}

describe("the profile along the laser", () => {
  it("takes the line's centre and the worst in view, row by row from the near end", () => {
    const p = profile(map(true), [0.2, 0.4]);
    expect(p.range_m).toEqual([0.2, 0.30000000000000004, 0.4]);
    // An even line: the mean of its middle two samples.
    expect(p.blur_px).toEqual([1.5, 11.5, 21.5]);
    expect(p.worst_blur_px).toEqual([3, 13, 23]);
    // The stripe is the beam and the blur, root sum of squares.
    expect(p.stripe_px).toEqual(p.blur_px);
    expect(p.depth_m_per_px[1]).toBeCloseTo(11.5e-6, 15);
    expect(p.worst_lateral_m_per_px[2]).toBeCloseTo(46e-6, 15);
  });

  it("leaves out what is out of view, and the stripe without a known waist", () => {
    const m = map(false);
    m.visible[5] = 0; // a middle sample of row 1
    const p = profile(m, [0.2, 0.4]);
    expect(p.blur_px[1]).toBeNaN();
    expect(p.worst_blur_px[1]).toBe(13);
    expect(p.stripe_px.every(Number.isNaN)).toBe(true);
    expect(p.worst_stripe_px.every(Number.isNaN)).toBe(true);
  });
});
