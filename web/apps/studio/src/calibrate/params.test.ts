import { describe, expect, it } from "vitest";

import type { CalibrationReport } from "../kernel/native";
import { binValues, sigmaAgreement, trialValues } from "./params";

/** A report whose estimate holds only what `trialValues` reads. */
const trial = (fx: number, k1: number, tiltX = 0): CalibrationReport =>
  ({
    estimate: {
      cameras: [
        {
          id: "cam left",
          intrinsics: [fx, fx + 1, 640, 512],
          distortion: [k1, 0, 0, 0, 0],
          tilt: [tiltX, 0],
          cam_se3_ref: { rotation: [0, 0, 0, 1], translation: [0, 0, 0] },
        },
      ],
      handeye: null,
      laser_planes: [],
      offsets: null,
    },
  }) as unknown as CalibrationReport;

describe("parameter statistics", () => {
  const reports = [trial(2300, -0.08, Math.PI / 180), trial(2310, -0.07), trial(2290, -0.09)];

  it("reads each trial's camera parameter as param_stats does", () => {
    expect(trialValues(reports, "cam left fx")).toEqual([2300, 2310, 2290]);
    expect(trialValues(reports, "cam left fy")).toEqual([2301, 2311, 2291]);
    expect(trialValues(reports, "cam left k1")).toEqual([-0.08, -0.07, -0.09]);
    // Tilts in degrees.
    expect(trialValues(reports, "cam left tilt_x")![0]).toBeCloseTo(1, 12);
  });

  it("has no per-trial values for the hand-eye or an unknown camera", () => {
    expect(trialValues(reports, "hand-eye rx")).toBeNull();
    expect(trialValues(reports, "cam right fx")).toBeNull();
    expect(trialValues(reports, "fx")).toBeNull();
  });

  it("bins values over their range, the maximum in the last bin", () => {
    const b = binValues([0, 1, 2, 3, 4], 4);
    expect([b.lo, b.hi]).toEqual([0, 4]);
    // Edges go up: 1 into the second bin, 4 (the top) into the last.
    expect(b.counts).toEqual([1, 1, 1, 2]);
  });

  it("widens the range to what must show (the truth, the σ band)", () => {
    const b = binValues([1, 2], 3, [-1, 5]);
    expect([b.lo, b.hi]).toEqual([-1, 5]);
    expect(b.counts).toEqual([0, 2, 0]);
  });

  it("widens a range of zero width around the values", () => {
    const b = binValues([7, 7, 7], 5);
    expect(b.lo).toBeLessThan(7);
    expect(b.hi).toBeGreaterThan(7);
    expect(b.counts).toEqual([0, 0, 3, 0, 0]);
    expect(binValues([0], 2).counts).toEqual([0, 1]);
  });

  it("compares the spread with the predicted σ", () => {
    expect(sigmaAgreement(1, 1)).toBe("ok");
    expect(sigmaAgreement(0.7, 1)).toBe("ok");
    expect(sigmaAgreement(1.4, 1)).toBe("ok");
    expect(sigmaAgreement(0.69, 1)).toBe("low");
    expect(sigmaAgreement(1.41, 1)).toBe("high");
    expect(sigmaAgreement(1, undefined)).toBeNull();
    expect(sigmaAgreement(1, 0)).toBeNull();
  });
});
