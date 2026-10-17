import type { BakedScenario, SceneSpec } from "@etendue/wasm";
import { describe, expect, it } from "vitest";

import { findNode, sceneTree, walk } from "./model";
import { targetOutline } from "./targets";

const I = { rotation: [0, 0, 0, 1], translation: [0, 0, 0] };

describe("sceneTree", () => {
  const scene = {
    version: 1,
    robots: [{ id: "arm", parent: "world", parent_se3_self: I, manifest: "m" }],
    rigs: [{ id: "rig", parent: "arm/tool0", parent_se3_self: I }],
    cameras: [{ id: "cam", parent: "rig", parent_se3_self: I }],
    targets: [{ id: "board", parent: "fixture", parent_se3_self: I }],
    frames: [{ id: "fixture", parent: "world", parent_se3_self: I }],
  } as unknown as SceneSpec;
  const baked = {
    frames: ["world", "arm/base", "arm/link1", "arm/tool0", "rig", "cam", "fixture", "board"],
  } as BakedScenario;

  it("nests entities under their parents and folds bare links", () => {
    const tree = sceneTree(scene, baked);
    const arm = findNode(tree, "arm")!;
    expect(arm.frame).toBe("arm/base");
    expect(arm.children.map((c) => c.id)).toEqual(["arm/tool0", "arm/"]);
    expect(findNode(tree, "arm/")!.children.map((c) => c.id)).toEqual(["arm/base", "arm/link1"]);
    expect(findNode(tree, "rig")!.children.map((c) => c.id)).toEqual(["cam"]);
    expect(findNode(tree, "fixture")!.children.map((c) => c.id)).toEqual(["board"]);
    expect([...walk(tree)].length).toBe(10);
    expect(findNode(tree, "nope")).toBeUndefined();
  });
});

describe("targets", () => {
  it("samples a planar target's outline as one closed polyline", () => {
    const line = targetOutline([0.4, 0.2], 2);
    // 4 edges × 2 samples, plus the closing corner.
    expect(line.length).toBe(9 * 3);
    expect(Array.from(line.subarray(0, 6))).toEqual([-0.2, -0.1, 0, 0, -0.1, 0]);
    expect(Array.from(line.subarray(24))).toEqual([-0.2, -0.1, 0]);
  });
});
