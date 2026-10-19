import type { Iso3Schema, SceneSpec } from "@etendue/wasm";
import { FrameTreeRuntime, type Iso3, composeIso3 } from "@vitavision/three";
import { describe, expect, it } from "vitest";

import { followers, sceneEffect } from "../scene/effect";
import { overridesOf, posed, showOverrides } from "./posed";

/** A rotation of `angle` about the unit `axis`, and a translation. */
function iso(axis: [number, number, number], angle: number, t: [number, number, number]): Iso3Schema {
  const n = Math.hypot(...axis);
  const s = Math.sin(angle / 2) / n;
  return { rotation: [axis[0] * s, axis[1] * s, axis[2] * s, Math.cos(angle / 2)], translation: t };
}

const ROBOT_BASE = iso([0, 0, 1], 0.3, [0.1, -0.2, 0]);
/** The robot's links in its base: two samples of a joint moving. */
const LINK_A = [iso([0, 1, 0], 0.2, [0, 0, 0.16]), iso([0, 1, 0], 0.5, [0, 0, 0.16])];
const LINK_B = [iso([1, 0, 0], -0.4, [0.4, 0, 0.1]), iso([1, 0, 0], 0.1, [0.4, 0, 0.1])];

function scene(rig: Iso3Schema, cam: Iso3Schema, robot = ROBOT_BASE): SceneSpec {
  return {
    version: 1,
    robots: [{ id: "r", parent: "world", parent_se3_self: robot, manifest: "robot.json" }],
    rigs: [{ id: "rig", parent: "r/b", parent_se3_self: rig }],
    cameras: [
      {
        id: "cam",
        parent: "rig",
        parent_se3_self: cam,
        resolution: [640, 480],
        params: {} as never,
      },
    ],
    targets: [{ id: "board", parent: "world", parent_se3_self: iso([0, 0, 1], 0, [0, 0, 0.5]) } as never],
  };
}

/** What a bake of `s` gives: every frame's world pose at both samples. */
function bake(s: SceneSpec) {
  const robot = s.robots![0]!.parent_se3_self;
  const rig = s.rigs![0]!.parent_se3_self;
  const cam = s.cameras![0]!.parent_se3_self;
  const board = s.targets![0]!.parent_se3_self;
  const frames = ["world", "r/a", "r/b", "rig", "cam", "board"];
  const samples = [0, 1].map((k) => {
    const a = composeIso3(robot, LINK_A[k]!);
    const b = composeIso3(robot, LINK_B[k]!);
    const r = composeIso3(b, rig);
    return {
      t: k * 0.01,
      world_se3_frame: [iso([0, 0, 1], 0, [0, 0, 0]), a, b, r, composeIso3(r, cam), board],
    };
  });
  return { dt: 0.01, frames, samples };
}

/** The largest difference of two poses' components (a quaternion and its negation are one rotation). */
function gap(actual: Iso3 | undefined, expected: Iso3Schema): number {
  if (!actual) return Infinity;
  const sign = Math.sign(actual.rotation[3] * expected.rotation[3]) || 1;
  return Math.max(
    ...actual.rotation.map((x, i) => Math.abs(x * sign - expected.rotation[i]!)),
    ...actual.translation.map((x, i) => Math.abs(x - expected.translation[i]!)),
  );
}

/** How far the override poses of editing `before` into `after` are from a bake of `after`. */
function deviation(before: SceneSpec, after: SceneSpec): number {
  const runtime = new FrameTreeRuntime(bake(before));
  const effect = sceneEffect(before, after);
  if (effect.kind !== "poses") return Infinity;
  const source = posed(
    runtime,
    overridesOf(effect.moved, (id) => followers(after, runtime.frameNames, id)),
  );
  const truth = bake(after);
  let worst = 0;
  for (const k of [0, 1]) {
    truth.frames.forEach((f, i) => {
      worst = Math.max(worst, gap(source.pose(f, k), truth.samples[k]!.world_se3_frame[i]!));
    });
  }
  return worst;
}

describe("pose overrides", () => {
  const rig = iso([0, 0, 1], 0.1, [0, 0, 0.05]);
  const cam = iso([1, 1, 0], 0.7, [0.06, 0, 0]);

  it("move an entity and what hangs on it, at every sample", () => {
    expect(deviation(scene(rig, cam), scene(iso([1, 0, 0], 0.4, [0.02, 0.01, 0.07]), cam))).toBeLessThan(1e-12);
  });

  it("move a robot with its links and its rig", () => {
    expect(deviation(scene(rig, cam), scene(rig, cam, iso([0, 1, 1], -0.6, [1, 0.5, 0.2])))).toBeLessThan(1e-12);
  });

  it("move a parent and its child in one edit", () => {
    expect(
      deviation(scene(rig, cam), scene(iso([0, 1, 0], 0.3, [0, 0.1, 0]), iso([0, 0, 1], -1, [0, 0, 0.2]))),
    ).toBeLessThan(1e-12);
  });

  it("keep the overrides on the frame objects through apply, and restore them", () => {
    const before = scene(rig, cam);
    const after = scene(iso([1, 0, 0], 0.4, [0.02, 0.01, 0.07]), cam);
    const runtime = new FrameTreeRuntime(bake(before));
    const effect = sceneEffect(before, after);
    if (effect.kind !== "poses") throw new Error(effect.kind);
    const overrides = overridesOf(effect.moved, (id) => followers(after, runtime.frameNames, id));
    // The camera frame object's x translation, as drawn.
    const camX = () => runtime.frame("cam")!.matrix.elements[12];
    const truth = bake(after);
    showOverrides(runtime, overrides);
    expect(camX()).toBeCloseTo(truth.samples[0]!.world_se3_frame[4]!.translation[0], 12);
    runtime.apply(1);
    expect(camX()).toBeCloseTo(truth.samples[1]!.world_se3_frame[4]!.translation[0], 12);
    showOverrides(runtime, []);
    expect(camX()).toBeCloseTo(bake(before).samples[1]!.world_se3_frame[4]!.translation[0], 12);
  });
});

describe("scene effects", () => {
  const base = scene(iso([0, 0, 1], 0.1, [0, 0, 0.05]), iso([1, 1, 0], 0.7, [0.06, 0, 0]));

  it("an intrinsics edit needs no bake", () => {
    const after = { ...base, cameras: [{ ...base.cameras![0]!, resolution: [800, 600] as [number, number] }] };
    expect(sceneEffect(base, after).kind).toBe("params");
  });

  it("a new parent, a rest pose or a new entity needs a bake", () => {
    const reparented = { ...base, cameras: [{ ...base.cameras![0]!, parent: "world" }] };
    expect(sceneEffect(base, reparented).kind).toBe("bake");
    const rest = { ...base, robots: [{ ...base.robots![0]!, initial_q: [0, 1] }] };
    expect(sceneEffect(base, rest).kind).toBe("bake");
    expect(
      sceneEffect(base, { ...base, frames: [{ id: "f", parent: "world", parent_se3_self: ROBOT_BASE }] }).kind,
    ).toBe("bake");
  });

  it("the followers of a robot are its links and what hangs on them", () => {
    expect([...followers(base, ["world", "r/a", "r/b", "rig", "cam", "board"], "r")]).toEqual([
      "r/a",
      "r/b",
      "rig",
      "cam",
    ]);
  });
});
