import type { BakedScenario, SceneSpec, TargetSpec } from "@etendue/wasm";
import { describe, expect, it } from "vitest";

import { findNode, sceneTree, walk } from "./model";
import { checkerOf, targetPolylines } from "./targets";

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
  const baked = { frames: ["world", "arm/base", "arm/link1", "arm/tool0", "rig", "cam", "fixture", "board"] } as BakedScenario;

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
  const board = (b: object) => ({ geometry: { type: "board", board: b } }) as unknown as TargetSpec;

  it("derives checker squares per board kind", () => {
    expect(checkerOf(board({ kind: "chessboard", rows: 6, cols: 9 }))).toEqual({ cols: 10, rows: 7 });
    expect(checkerOf(board({ kind: "charuco", rows: 5, cols: 7 }))).toEqual({ cols: 7, rows: 5 });
    expect(checkerOf(board({ kind: "ringgrid" }))).toBeNull();
    expect(checkerOf({ geometry: { type: "rectangle", width: 1, height: 1 } } as TargetSpec)).toBeNull();
  });

  it("samples the grid lines of the board in its plane", () => {
    const lines = targetPolylines([0.4, 0.2], { cols: 4, rows: 2 }, 2);
    expect(lines).toHaveLength(5 + 3);
    expect(Array.from(lines[0]!)).toEqual([-0.2, -0.1, 0, -0.2, 0, 0, -0.2, 0.1, 0]);
    expect(targetPolylines([1, 1], null)).toHaveLength(4);
  });
});
