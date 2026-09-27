import { Vector3 } from "three";
import { describe, expect, it } from "vitest";

import { FrameTreeRuntime, type BakedScenarioLike } from "./frameTree";

const I = { rotation: [0, 0, 0, 1], translation: [0, 0, 0] };
const at = (x: number) => ({ rotation: [0, 0, 0, 1], translation: [x, 0, 0] });

const baked: BakedScenarioLike = {
  dt: 0.5,
  frames: ["world", "robot/base", "robot/tool0"],
  samples: [
    { t: 0, world_se3_frame: [I, I, at(1)] },
    { t: 0.5, world_se3_frame: [I, I, at(2)], capture: { id: "c0" } },
    { t: 1, world_se3_frame: [I, I, at(3)], capture: null },
  ],
};

const worldPos = (runtime: FrameTreeRuntime, frame: string) => {
  runtime.root.updateMatrixWorld(true);
  return new Vector3().setFromMatrixPosition(runtime.frame(frame)!.matrixWorld).x;
};

describe("FrameTreeRuntime", () => {
  it("creates one object per frame, world as the root", () => {
    const r = new FrameTreeRuntime(baked);
    expect(r.frame("world")).toBe(r.root);
    expect(r.root.children.map((c) => c.name)).toEqual(["robot/base", "robot/tool0"]);
    expect(r.frame("robot/tool0")!.matrixAutoUpdate).toBe(false);
    expect(r.frame("nope")).toBeUndefined();
    expect(r.sampleCount).toBe(3);
    expect(r.dt).toBe(0.5);
    expect(r.captures).toEqual([{ index: 1, id: "c0" }]);
  });

  it("applies samples, rounding and clamping the index", () => {
    const r = new FrameTreeRuntime(baked);
    expect(r.current).toBe(0);
    expect(worldPos(r, "robot/tool0")).toBe(1);
    expect(r.apply(1.4)).toBe(1);
    expect(worldPos(r, "robot/tool0")).toBe(2);
    expect(r.apply(99)).toBe(2);
    expect(worldPos(r, "robot/tool0")).toBe(3);
    expect(r.apply(-5)).toBe(0);
    expect(r.apply(0)).toBe(0);
  });

  it("reads a frame's pose at any sample", () => {
    const r = new FrameTreeRuntime(baked);
    expect(r.pose("robot/tool0", 2)).toEqual({ rotation: [0, 0, 0, 1], translation: [3, 0, 0] });
    expect(r.pose("robot/tool0")!.translation[0]).toBe(1);
    expect(r.pose("nope")).toBeUndefined();
    expect(r.pose("world", 7)).toBeUndefined();
  });

  it("rejects malformed scenarios", () => {
    expect(() => new FrameTreeRuntime({ ...baked, frames: ["base", "world"] })).toThrow(/world/);
    expect(() => new FrameTreeRuntime({ ...baked, samples: [] })).toThrow(/sample/);
    expect(
      () => new FrameTreeRuntime({ ...baked, samples: [{ t: 0, world_se3_frame: [I] }] }),
    ).toThrow(/1 poses for 3 frames/);
  });
});
