import { describe, expect, it } from "vitest";

import { describeRun, runName, samePath } from "./runs";

describe("kept runs", () => {
  const at = new Date(2026, 9, 19, 18, 5, 2);

  it("names a run by its dataset, time and observations", () => {
    expect(runName("/p/datasets/ring3/", {}, at)).toBe("ring3-20261019-180502-exact");
    expect(runName("C:\\p\\datasets\\ring3", { noise: { pixel_sigma_px: 0.5, seed: 4 }, trials: 8 }, at)).toBe(
      "ring3-20261019-180502-noise-s4x8",
    );
    expect(runName("/d/x", { detected: true, camera: "cam left" }, at)).toBe("x-20261019-180502-detected-cam_left");
  });

  it("describes what a run calibrated from", () => {
    expect(describeRun({})).toBe("exact");
    // A kept run's config spells every field out; zeros are exact.
    expect(describeRun({ noise: { pixel_sigma_px: 0, dropout_fraction: 0, seed: 3 }, trials: 1 })).toBe("exact");
    expect(describeRun({ noise: { pixel_sigma_px: 0.5, dropout_fraction: 0.1, seed: 2 }, trials: 4 })).toBe(
      "σ 0.5 px, 10 % dropped, seed 2, 4 trials",
    );
    expect(describeRun({ detected: true, camera: "cam_left" })).toBe("detected, cam_left alone");
  });

  it("compares folders whatever their separators", () => {
    expect(samePath("C:\\a\\b\\", "C:/a/b")).toBe(true);
    expect(samePath("/a/b", "/a/bc")).toBe(false);
  });
});
