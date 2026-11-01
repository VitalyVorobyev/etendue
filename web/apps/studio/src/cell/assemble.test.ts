import { readFileSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";

import type { CameraSpec, LaserSpec, LibraryItem, RigLayout } from "@etendue/wasm";
import { describe, expect, it, vi } from "vitest";

import { differences } from "../../e2e/close";
import type { CellDraft } from "./assemble";

// Only the unit of a library item is used from rigs/design; it needs no kernel.
vi.mock("@etendue/wasm", () => ({ deriveOptics: (_: unknown, params: unknown) => ({ params }) }));

const { IDENTITY, assembleCell, defaultPositions, nominalDistance, renamePositions, targetCoverage } =
  await import("./assemble");

const ROOT = resolve(import.meta.dirname, "../../../../..");
const read = (path: string): unknown => JSON.parse(readFileSync(join(ROOT, path), "utf8"));

/** The built-in library, by reference, as the studio reads it. */
const library = new Map<string, LibraryItem>(
  ["cameras", "rigs", "targets", "mounts"].flatMap((folder) =>
    readdirSync(join(ROOT, `examples/library/${folder}`), { encoding: "utf8" }).map((n): [string, LibraryItem] => [
      `${folder}/${n.replace(/\.json$/, "")}`,
      read(`examples/library/${folder}/${n}`) as LibraryItem,
    ]),
  ),
);

/** The kernel's expansion for parallel rigs (tilt 0) with `image_x: common`: cameras on a circle, unrotated. */
function expand(layout: RigLayout, rig: string): { cameras: CameraSpec[]; lasers: LaserSpec[] } {
  const cameras = Array.from({ length: layout.count }, (_, i): CameraSpec => {
    const phi = (layout.phase_rad ?? 0) + (2 * Math.PI * i) / layout.count;
    return {
      id: `${rig}_cam${i}`,
      parent: rig,
      parent_se3_self: {
        rotation: [0, 0, 0, 1],
        translation: [layout.radius_m * Math.cos(phi), layout.radius_m * Math.sin(phi), 0],
      },
      params: layout.camera.params,
      resolution: layout.camera.resolution,
      ...(layout.camera.optics ? { optics: layout.camera.optics } : {}),
    };
  });
  return { cameras, lasers: [] };
}

const UR5E_Q = [0, -Math.PI / 2, Math.PI / 2, -Math.PI / 2, -Math.PI / 2, 0];

function draft(patch: Partial<CellDraft> = {}): CellDraft {
  return {
    description: "",
    optics: "cameras/python1300_pinhole",
    rig: { id: "rig", design: null, cameraId: "cam", positions: [] },
    mounting: "eye_in_hand",
    mount: null,
    flange: IDENTITY,
    stand: { id: "stand", pose: IDENTITY },
    rigOnStand: IDENTITY,
    target: { id: "board", item: "targets/chess_9x6_25mm", pose: IDENTITY },
    robot: { id: "ur5e", manifest: "assets/robots/ur5e/robot.json", base: IDENTITY, q: UR5E_Q },
    workingDistance: null,
    ...patch,
  };
}

const ctx = { library, tcpLink: "tool0", expand };

/** The examples' stereo pair: the parallel design, its cameras renamed, the right one its own unit. */
const stereo: CellDraft["rig"] = {
  id: "rig",
  design: "rigs/stereo_80mm_parallel",
  cameraId: "cam",
  positions: [
    { id: "cam_left", camera: null },
    { id: "cam_right", camera: "cameras/python1300_pinhole_right" },
  ],
};

describe("assembleCell", () => {
  it("recreates the eye-in-hand example from library items", () => {
    const example = read("examples/eye_in_hand_ur5e/scene.json") as { description: string };
    const { cell, problems } = assembleCell(
      draft({
        description: example.description,
        rig: stereo,
        mount: "mounts/flange_plate_30mm",
        target: {
          id: "board",
          item: "targets/chess_9x6_25mm",
          pose: { rotation: [0, 0, 0, 1], translation: [0.45, 0.15, 0] },
        },
      }),
      ctx,
    );
    expect(problems).toEqual([]);
    expect(differences(cell?.scene, example)).toEqual([]);
    expect(cell?.scenario.steps).toEqual([]);
    expect(cell?.used).toEqual([
      "cameras/python1300_pinhole",
      "rigs/stereo_80mm_parallel",
      "cameras/python1300_pinhole_right",
      "mounts/flange_plate_30mm",
      "targets/chess_9x6_25mm",
    ]);
  });

  it("recreates the eye-to-hand example from library items", () => {
    const example = read("examples/eye_to_hand_ur5e/scene.json") as {
      description: string;
      rigs: { parent_se3_self: CellDraft["rigOnStand"] }[];
    };
    const { cell, problems } = assembleCell(
      draft({
        description: example.description,
        rig: stereo,
        mounting: "eye_to_hand",
        mount: "mounts/board_holder_10mm",
        stand: { id: "stand", pose: { rotation: [0, 0, 0, 1], translation: [0.45, -0.75, 0] } },
        rigOnStand: example.rigs[0]!.parent_se3_self,
      }),
      ctx,
    );
    expect(problems).toEqual([]);
    expect(differences(cell?.scene, example)).toEqual([]);
  });

  it("keeps the layout only while the cameras are the layout's own", () => {
    const plain = { ...stereo, positions: defaultPositions("rig", 2) };
    const kept = assembleCell(draft({ rig: plain }), ctx).cell?.scene;
    expect(kept?.rigs?.[0]?.layout?.count).toBe(2);
    expect(kept?.cameras?.map((c) => c.id)).toEqual(["rig_cam0", "rig_cam1"]);
    // The optics of the first step replace the design's camera everywhere.
    const imx = assembleCell(draft({ rig: plain, optics: "cameras/imx174_lm25hc" }), ctx).cell?.scene;
    expect(imx?.rigs?.[0]?.layout?.camera.resolution).toEqual([1936, 1216]);
    expect(imx?.cameras?.every((c) => c.resolution[0] === 1936 && c.optics != null)).toBe(true);
    // Renamed, the rig is a plain rig.
    const renamed = assembleCell(draft({ rig: stereo }), ctx).cell?.scene;
    expect(renamed?.rigs?.[0]?.layout).toBeUndefined();
  });

  it("follows the rig's id in the positions that keep the layout's names", () => {
    const positions = renamePositions([...defaultPositions("rig", 2), { id: "mine", camera: null }], "rig", "head");
    expect(positions.map((p) => p.id)).toEqual(["head_cam0", "head_cam1", "mine"]);
  });

  it("puts one camera on the rig, and a head's laser on its camera", () => {
    const camera = assembleCell(draft(), ctx).cell?.scene;
    expect(camera?.cameras?.map((c) => [c.id, c.parent])).toEqual([["cam", "rig"]]);
    expect(camera?.lasers).toBeUndefined();
    const head: LibraryItem = {
      version: 2,
      id: "h",
      name: "Head",
      head: {
        camera: library.get("cameras/imx174_lm25hc")!.camera!,
        laser: { wavelength_nm: 660, fan_half_angle: 0.3, beam_waist_m: 4e-4 },
        fan_length_m: 0.8,
        camera_se3_laser: { rotation: [0, 0, 0, 1], translation: [0.05, 0, 0] },
      },
    };
    const withHead = new Map(library).set("heads/h", head);
    const scene = assembleCell(draft({ optics: "heads/h" }), { ...ctx, library: withHead }).cell?.scene;
    expect(scene?.lasers).toEqual([
      {
        id: "cam_laser",
        parent: "cam",
        parent_se3_self: { rotation: [0, 0, 0, 1], translation: [0.05, 0, 0] },
        fan_half_angle: 0.3,
        fan_length: 0.8,
        wavelength_nm: 660,
        beam_waist_m: 4e-4,
        camera: "cam",
      },
    ]);
  });

  it("says, by step, why the draft makes no cell", () => {
    const { cell, problems } = assembleCell(
      draft({ target: { id: "rig", item: null, pose: IDENTITY }, robot: { ...draft().robot, manifest: null } }),
      ctx,
    );
    expect(cell).toBeNull();
    expect(problems).toEqual([
      { step: "target", message: "Choose a target." },
      { step: "robot", message: "Choose a robot." },
      { step: "target", message: "`rig` is used twice." },
    ]);
    expect(assembleCell(draft(), { ...ctx, tcpLink: null }).problems).toEqual([
      { step: "robot", message: "Reading the robot…" },
    ]);
    expect(assembleCell(draft({ optics: null }), ctx).problems[0]?.step).toBe("optics");
  });
});

describe("the target's size against the field of view", () => {
  const camera = library.get("cameras/python1300_pinhole")!.camera!;

  it("compares the longer side with the image width at the distance", () => {
    // 1280 px at 2318.8 px focal length: 0.276 m wide at 0.5 m.
    const c = targetCoverage([0.225, 0.15], camera, 0.5);
    expect(c.fov_m).toBeCloseTo((1280 / 2318.8) * 0.5, 12);
    expect(c.fraction).toBeCloseTo(0.225 / c.fov_m, 12);
    expect(c.verdict).toBe("ok");
  });

  it("warns under a fifth and over nine tenths of the width", () => {
    expect(targetCoverage([0.225, 0.15], camera, 2.5).verdict).toBe("small");
    expect(targetCoverage([0.225, 0.15], camera, 0.25).verdict).toBe("large");
  });

  it("judges at the design's working distance, a camera's focus, or 0.5 m", () => {
    expect(nominalDistance(draft({ rig: stereo }), library)).toBe(0.5);
    expect(nominalDistance(draft({ optics: "cameras/imx174_lm25hc" }), library)).toBe(0.5);
    expect(nominalDistance(draft({ workingDistance: 0.8 }), library)).toBe(0.8);
    const ring = draft({ rig: { ...stereo, design: "rigs/ring3_laser" } });
    expect(nominalDistance(ring, library)).toBe(0.5);
  });
});
