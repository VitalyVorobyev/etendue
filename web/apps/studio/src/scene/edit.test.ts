import type { SceneSpec } from "@etendue/wasm";
import { describe, expect, it } from "vitest";

import {
  IDENTITY,
  addEntity,
  childrenOf,
  entityOf,
  newEntity,
  removeEntity,
  removeRobot,
  renameEntity,
  uniqueId,
  updateEntity,
} from "./edit";

const scene: SceneSpec = {
  version: 1,
  robots: [{ id: "ur5e", parent: "world", parent_se3_self: IDENTITY, manifest: "robot.json" }],
  rigs: [{ id: "rig", parent: "ur5e/tool0", parent_se3_self: IDENTITY }],
  frames: [{ id: "plate", parent: "world", parent_se3_self: IDENTITY }],
  lasers: [
    {
      id: "laser",
      parent: "rig",
      parent_se3_self: IDENTITY,
      fan_half_angle: 0.3,
      fan_length: 1,
      wavelength_nm: 660,
      beam_waist_m: 0.0005,
    },
  ],
};

describe("scene edits", () => {
  it("makes fresh ids", () => {
    expect(uniqueId(scene, "cam")).toBe("cam");
    expect(uniqueId(scene, "rig")).toBe("rig_2");
    const withCam = addEntity(scene, "cameras", newEntity(scene, "cameras", "rig"));
    expect(entityOf(withCam, "cam")?.parent).toBe("rig");
    expect(newEntity(withCam, "cameras", "rig").id).toBe("cam_2");
  });

  it("updates one entity and shares the rest", () => {
    const moved = updateEntity(scene, "plate", (e) => ({
      ...e,
      parent_se3_self: { ...e.parent_se3_self, translation: [1, 0, 0] },
    }));
    expect(entityOf(moved, "plate")?.parent_se3_self.translation).toEqual([1, 0, 0]);
    expect(moved.rigs).toBe(scene.rigs);
    expect(scene.frames?.[0]?.parent_se3_self.translation).toEqual([0, 0, 0]);
    expect(updateEntity(scene, "nope", (e) => e)).toBe(scene);
  });

  it("removes an entity with what hangs on it", () => {
    expect(childrenOf(scene, "rig")).toEqual(["laser"]);
    const gone = removeEntity(scene, "rig");
    expect(gone.rigs).toEqual([]);
    expect(gone.lasers).toEqual([]);
    expect(gone.frames).toBe(scene.frames);
  });

  it("renames an entity and the parents that name it", () => {
    const renamed = renameEntity(scene, "rig", "head");
    expect(renamed?.rigs?.[0]?.id).toBe("head");
    expect(renamed?.lasers?.[0]?.parent).toBe("head");
    expect(renameEntity(scene, "rig", "plate")).toBeNull();
    expect(renameEntity(scene, "rig", " ")).toBeNull();
    expect(renameEntity(scene, "rig", "rig")).toBe(scene);
  });

  it("removes a robot with what it carries and the steps that move it", () => {
    const { scene: gone, scenario } = removeRobot(
      scene,
      {
        version: 1,
        dt: 0.01,
        steps: [{ type: "ptp_joints", robot: "ur5e", q: [0, 0, 0, 0, 0, 0], speed_scale: 1 }, { type: "capture" }],
      },
      "ur5e",
    );
    expect(gone.robots).toEqual([]);
    expect(gone.rigs).toEqual([]);
    expect(gone.lasers).toEqual([]);
    expect(gone.frames).toBe(scene.frames);
    expect(scenario?.steps).toEqual([{ type: "capture" }]);
  });
});
