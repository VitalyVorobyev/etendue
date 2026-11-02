import { describe, expect, it } from "vitest";

import { differences, largestDeviation, robotOf } from "../../e2e/close";

describe("the tolerance-aware comparison of gate G15.1", () => {
  const scene = {
    version: 1,
    robots: [{ id: "ur5e", parent: "world", manifest: "../../assets/robots/ur5e/robot.json", initial_q: [0, 1.5] }],
  };

  it("takes numbers within the tolerance, and a manifest by the robot it names", () => {
    const close = structuredClone(scene);
    close.robots[0]!.initial_q[1] = 1.5 + 5e-13;
    close.robots[0]!.manifest = "assets/robots/ur5e/robot.json";
    expect(differences(close, scene)).toEqual([]);
    expect(largestDeviation(close, scene)).toBeCloseTo(5e-13, 15);
    expect(robotOf("/abs/assets/robots-local/fr5/robot.json")).toBe("assets/robots-local/fr5/robot.json");
  });

  it("finds a number out of tolerance, another id or robot, a missing or extra key and another length", () => {
    const off = structuredClone(scene);
    off.robots[0]!.initial_q[1] = 1.5 + 2e-12;
    expect(differences(off, scene)).toHaveLength(1);
    const renamed = structuredClone(scene);
    renamed.robots[0]!.id = "arm";
    expect(differences(renamed, scene)).toEqual(['$.robots[0].id: "arm" ≠ "ur5e"']);
    const other = structuredClone(scene);
    other.robots[0]!.manifest = "assets/robots/abb/robot.json";
    expect(differences(other, scene)).toHaveLength(1);
    expect(differences({ ...scene, lasers: [] }, scene)).toEqual(["$.lasers: not expected"]);
    expect(differences({ version: 1 }, scene)).toEqual(["$.robots: missing"]);
    expect(differences({ ...scene, robots: [] }, scene)).toEqual(["$.robots: 0 entries, not 1"]);
  });
});
