import { describe, expect, it } from "vitest";

import { jogged, motionSteps, reteach, teach } from "./jog";

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

  it("re-teaches a step in place", () => {
    const q = [0, -1, 1, -1, -1.5, 0];
    const scenario = {
      version: 1,
      dt: 0.01,
      steps: [
        { type: "ptp_joints" as const, robot: "ur5e", q: [0, 0, 0, 0, 0, 0] },
        { type: "capture" as const },
        { type: "lin" as const, robot: "ur5e", base_se3_tool: tcp, speed_scale: 0.5 },
        { type: "ptp_pose" as const, robot: "other", base_se3_tool: tcp },
      ],
    };
    expect(motionSteps(scenario, "ur5e")).toEqual([
      { index: 0, label: "1: PTP" },
      { index: 2, label: "3: LIN" },
    ]);
    const moved = { ...tcp, translation: [0.4, 0.1, 0.3] as [number, number, number] };
    expect(reteach(scenario.steps[0]!, q, moved)).toEqual({ type: "ptp_joints", robot: "ur5e", q });
    // A LIN keeps its speed and takes the pose.
    expect(reteach(scenario.steps[2]!, q, moved)).toEqual({
      type: "lin",
      robot: "ur5e",
      base_se3_tool: moved,
      speed_scale: 0.5,
    });
    expect(reteach(scenario.steps[3]!, q, moved)).toEqual({
      type: "ptp_pose",
      robot: "other",
      base_se3_tool: moved,
      q,
    });
    expect(reteach(scenario.steps[1]!, q, moved)).toBeNull();
  });
});
