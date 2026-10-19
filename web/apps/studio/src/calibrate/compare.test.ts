import type { Iso3Schema } from "@etendue/wasm";
import { composeIso3 } from "@vitavision/three";
import { describe, expect, it } from "vitest";

import { exaggerate, exaggeratePlane, fanInCamera, ontoPlane } from "./compare";

const angle = (q: readonly number[]) => 2 * Math.atan2(Math.hypot(q[0]!, q[1]!, q[2]!), Math.abs(q[3]!));

describe("the estimate beside the truth", () => {
  // 0.01 rad about (1, 2, 2)/3, and 1 mm along x.
  const a = 0.005;
  const offset = {
    rotation: [(Math.sin(a) * 1) / 3, (Math.sin(a) * 2) / 3, (Math.sin(a) * 2) / 3, Math.cos(a)] as [
      number,
      number,
      number,
      number,
    ],
    translation: [1e-3, 0, 0] as [number, number, number],
  };

  it("exaggerates an offset's angle and translation, about the same axis", () => {
    const big = exaggerate(offset, 100);
    expect(angle(big.rotation)).toBeCloseTo(1, 12);
    const axis = big.rotation.slice(0, 3).map((x) => x / Math.sin(0.5));
    expect(axis[0]).toBeCloseTo(1 / 3, 12);
    expect(axis[1]).toBeCloseTo(2 / 3, 12);
    expect(big.translation[0]).toBeCloseTo(0.1, 15);
    // ×1 is the offset itself; ×0 the identity; the quaternion's sign does not matter.
    expect(exaggerate(offset, 1).rotation[3]).toBeCloseTo(offset.rotation[3], 15);
    expect(exaggerate(offset, 0).rotation).toEqual([0, 0, 0, 1]);
    const flipped = { ...offset, rotation: offset.rotation.map((x) => -x) as typeof offset.rotation };
    expect(angle(exaggerate(flipped, 10).rotation)).toBeCloseTo(0.1, 12);
  });

  it("tilts and shifts a plane away from the truth", () => {
    const truth = { normal: [0, 0, 1] as [number, number, number], distance: -0.4 };
    const t = 1e-3;
    const estimate = { normal: [Math.sin(t), 0, Math.cos(t)] as [number, number, number], distance: -0.4001 };
    const big = exaggeratePlane(truth, estimate, 100);
    expect(Math.asin(big.normal[0])).toBeCloseTo(0.1, 12);
    expect(big.normal[1]).toBe(0);
    expect(big.distance).toBeCloseTo(-0.41, 12);
    expect(exaggeratePlane(truth, truth, 1000)).toEqual(truth);
  });

  it("moves points onto a plane and places a fan", () => {
    const plane = { normal: [0, 0, 1] as [number, number, number], distance: -0.4 };
    expect(Array.from(ontoPlane([0.1, 0.2, 0.3], plane))).toEqual([0.1, 0.2, 0.4].map(Math.fround));
    // A laser 0.1 m to the camera's +x, its fan in its own x = 0 plane opening along +z.
    const cameraSe3Laser: Iso3Schema = { rotation: [0, 0, 0, 1], translation: [0.1, 0, 0] };
    const fan = fanInCamera(cameraSe3Laser, 0.3, 0.5, 4);
    expect(fan.length).toBe(3 * 6);
    expect(Array.from(fan.slice(0, 3))).toEqual([0.1, 0, 0].map(Math.fround));
    // The middle of the arc is straight ahead.
    expect(fan[3 * 3]).toBeCloseTo(0.1, 6);
    expect(fan[3 * 3 + 2]).toBeCloseTo(0.5, 6);
    const shifted = composeIso3(cameraSe3Laser, { rotation: [0, 0, 0, 1], translation: [0, 0, 1] }).translation;
    expect(shifted).toEqual([0.1, 0, 1]);
  });
});
