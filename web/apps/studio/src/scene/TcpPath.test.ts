import type { MotionReport } from "@etendue/wasm";
import { Color } from "three";
import { describe, expect, it } from "vitest";

import { pathColors, stepAt } from "./TcpPath";

const report = {
  duration_s: 3,
  captures: 1,
  steps: [
    { step: 0, kind: "ptp_joints", t0: 0, duration_s: 1, robot: "ur5e" },
    { step: 1, kind: "capture", t0: 1, duration_s: 0.5 },
    {
      step: 2,
      kind: "ptp_pose",
      t0: 1.5,
      duration_s: 1.5,
      robot: "ur5e",
      warnings: [{ type: "near_limit", joint: "j", margin: 0.01 }],
    },
  ],
} as unknown as MotionReport;

describe("the TCP path by step", () => {
  it("finds the step running at a time", () => {
    expect(stepAt(report, 0)).toBe(0);
    expect(stepAt(report, 0.99)).toBe(0);
    expect(stepAt(report, 1)).toBe(1);
    expect(stepAt(report, 2.9)).toBe(2);
    expect(stepAt(null, 1)).toBe(-1);
    // At the capture, the move that led there.
    expect(stepAt(report, 1.2, true)).toBe(0);
  });

  it("colours steps alternately, warnings and the current step apart", () => {
    const warn = new Color(1, 0, 0);
    const current = new Color(0, 1, 0);
    const c = pathColors(new Float32Array([0.5, 1.2, 2]), report, 1, { warn, current });
    const at = (i: number) => Array.from(c.slice(3 * i, 3 * i + 3));
    expect(at(1)).toEqual([0, 1, 0]);
    expect(at(2)).toEqual([1, 0, 0]);
    expect(at(0)).not.toEqual(at(1));
  });
});
