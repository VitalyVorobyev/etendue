import { describe, expect, it } from "vitest";

import type { FeatureResidual } from "../kernel/native";
import {
  groupViews,
  overallRms,
  residualLength,
  residualMarks,
  residualPrimitives,
  residualSize,
  viewFeatures,
} from "./residuals";

const f = (capture: string, camera: string, point: number, du: number, dv: number): FeatureResidual => ({
  capture,
  camera,
  point,
  observed_px: [100 + point, 200],
  projected_px: [100 + point + du, 200 + dv],
});

describe("per-view residuals", () => {
  const features = [
    f("c0", "left", 0, 3, 4),
    f("c0", "left", 1, 0, 0),
    f("c0", "right", 0, 1, 0),
    f("c1", "left", 0, 0, 2),
    f("c0", "left", 2, 0, 1),
  ];

  it("measures a residual from the observed point to its reprojection", () => {
    expect(residualLength(features[0]!)).toBe(5);
  });

  it("groups features by capture and camera, in the order they first appear", () => {
    const views = groupViews(features);
    expect(views.map((v) => [v.capture, v.camera, v.count])).toEqual([
      ["c0", "left", 3],
      ["c0", "right", 1],
      ["c1", "left", 1],
    ]);
    // √((25 + 0 + 1) / 3) and the largest, 5.
    expect(views[0]!.rms).toBeCloseTo(Math.sqrt(26 / 3), 12);
    expect(views[0]!.max).toBe(5);
    expect(views[2]!.rms).toBe(2);
  });

  it("selects one view's features", () => {
    expect(viewFeatures(features, "c0", "left").map((x) => x.point)).toEqual([0, 1, 2]);
    expect(viewFeatures(features, "c2", "left")).toEqual([]);
  });

  it("gives the RMS over every point, and none for no points", () => {
    expect(overallRms(features)).toBeCloseTo(Math.sqrt((25 + 0 + 1 + 4 + 1) / 5), 12);
    expect(overallRms([])).toBe(0);
  });

  it("sizes a residual against the RMS", () => {
    expect(residualSize(0.5, 1)).toBe("small");
    expect(residualSize(1, 1)).toBe("small");
    expect(residualSize(1.5, 1)).toBe("medium");
    expect(residualSize(2, 1)).toBe("medium");
    expect(residualSize(2.01, 1)).toBe("large");
    // Without a spread (exact data) every residual is small.
    expect(residualSize(1e-9, 0)).toBe("small");
  });

  it("magnifies the vector from the observed point towards the reprojection", () => {
    const [m] = residualMarks([f("c0", "left", 0, 3, 4)], 10, 5);
    expect(m!.observed).toEqual([100, 200]);
    expect(m!.projected).toEqual([103, 204]);
    expect(m!.tip).toEqual([130, 240]);
    expect(m!.size).toBe("small");
    expect(residualMarks([f("c0", "left", 0, 3, 4)], 1, 2)[0]!.size).toBe("large");
  });

  it("draws each size's vectors as one set of segments, in its colour", () => {
    const marks = residualMarks([f("c", "l", 0, 1, 0), f("c", "l", 1, 3, 0), f("c", "l", 2, 1.5, 0)], 2, 1);
    const prims = residualPrimitives(marks);
    const segments = prims.filter((p) => p.kind === "segments");
    expect(segments.map((s) => s.tone)).toEqual(["normal", "warn", "defect"]);
    expect(segments[2]).toMatchObject({ points: [101, 200, 107, 200] });
    // A cross per reprojection and a dot per observed point.
    expect(prims.filter((p) => p.kind === "point")).toHaveLength(6);
  });
});
