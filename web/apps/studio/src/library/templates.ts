/**
 * What a new library item starts as: a plain part for the simple kinds, and for the others a part
 * the project already has (or the Optics and Rigs workspaces' starting designs), so it checks
 * from the first keystroke.
 */

import type { CameraTemplate, LibraryItem, TargetPart } from "@etendue/wasm";

import { type LibraryKind, type Project, itemsOf } from "../io/project";
import { initialHead, toDesign } from "../optics/design";
import { initialRig, toLayout } from "../rigs/design";

const PINHOLE: CameraTemplate = {
  params: {
    projection: { type: "pinhole" },
    distortion: { type: "brown_conrady5", k1: 0, k2: 0, k3: 0, p1: 0, p2: 0, iters: 8 },
    sensor: { type: "identity" },
    intrinsics: { fx: 2000, fy: 2000, cx: 640, cy: 512 },
  },
  resolution: [1280, 1024],
};

const CHESSBOARD: TargetPart = {
  geometry: { type: "board", board: { kind: "chessboard", rows: 6, cols: 9, square_size_m: 0.02 } },
};

/** The part a new item of `kind` holds (`{ lens: … }`), given what `project` sees. */
export function newPart(kind: LibraryKind, project: Project | null): Partial<LibraryItem> {
  switch (kind) {
    case "sensor":
      return { sensor: { pixel_pitch_m: 3.45e-6, resolution: [2448, 2048] } };
    case "lens":
      return { lens: { focal_length_m: 0.025, min_f_number: 1.4, mount: "c_mount" } };
    case "laser":
      return { laser: { wavelength_nm: 660, fan_half_angle: 0.26 } };
    case "mount":
      return { mount: { flange_se3_device: { rotation: [0, 0, 0, 1], translation: [0, 0, 0] } } };
    case "camera":
      return { camera: structuredClone(firstOf(project, "camera")?.camera ?? PINHOLE) };
    case "target":
      return { target: structuredClone(firstOf(project, "target")?.target ?? CHESSBOARD) };
    case "head": {
      const head = firstOf(project, "head")?.head;
      return { head: head ? structuredClone(head) : toDesign(initialHead(project)) };
    }
    case "rig": {
      const rig = firstOf(project, "rig")?.rig;
      if (rig) return { rig: structuredClone(rig) };
      const start = initialRig(project, null);
      if (!start) throw new Error("A rig needs a camera or a head in the library to start from.");
      return { rig: { layout: toLayout(start) } };
    }
    case "procedure":
      return {
        procedure: structuredClone(
          firstOf(project, "procedure")?.procedure ?? { stages: [{ id: "intrinsics", problem: "planar_intrinsics" }] },
        ),
      };
  }
}

function firstOf(project: Project | null, kind: LibraryKind): LibraryItem | undefined {
  return project ? itemsOf(project, kind)[0]?.[1] : undefined;
}
