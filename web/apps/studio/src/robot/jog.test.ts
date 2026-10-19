import { describe, expect, it } from "vitest";

import { jogged, teach } from "./jog";

const s = Math.SQRT1_2;
// A TCP 0.5 m out along x, its tool z pointing down (180° about x).
const tcp = {
  rotation: [1, 0, 0, 0] as [number, number, number, number],
  translation: [0.5, 0, 0.4] as [number, number, number],
};

describe("jogging", () => {
  it("steps along the tool's axes or the base's", () => {
    const up = {
      rotation: [0, 0, 0, 1] as [number, number, number, number],
      translation: [0, 0, 0.01] as [number, number, number],
    };
    // Tool z points down: +z in tool axes lowers the TCP; in base axes it raises it.
    expect(jogged(tcp, up, "tool").translation[2]).toBeCloseTo(0.39, 12);
    expect(jogged(tcp, up, "base").translation[2]).toBeCloseTo(0.41, 12);
    // A turn about base z keeps the TCP where it is.
    const turn = {
      rotation: [0, 0, s, s] as [number, number, number, number],
      translation: [0, 0, 0] as [number, number, number],
    };
    expect(jogged(tcp, turn, "base").translation).toEqual(tcp.translation);
  });

  it("teaches a PTP move with its joints, and a capture", () => {
    const q = [0, -1, 1, -1, -1.5, 0];
    const taught = teach({ version: 1, dt: 0.01, steps: [{ type: "wait", duration_s: 1 }] }, "ur5e", q, tcp, true);
    expect(taught.steps).toEqual([
      { type: "wait", duration_s: 1 },
      { type: "ptp_pose", robot: "ur5e", base_se3_tool: tcp, q },
      { type: "capture" },
    ]);
    expect(teach(null, "ur5e", q, tcp, false)).toEqual({
      version: 1,
      dt: 0.01,
      steps: [{ type: "ptp_pose", robot: "ur5e", base_se3_tool: tcp, q }],
    });
  });
});
