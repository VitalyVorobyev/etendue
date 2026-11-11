import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { type ProcedurePart, initSync, procedureRules, validateLibraryItem } from "@etendue/wasm";
import { beforeAll, describe, expect, it } from "vitest";

import {
  FREEDOMS,
  addStage,
  moveStage,
  problemForDataset,
  procedureIssues,
  removeStage,
  renameStage,
  seedOf,
  seedSlots,
  seedValue,
  tidy,
  withProblem,
} from "./procedure";

beforeAll(() => {
  // The real kernel: its checks are the ones the editor shows.
  initSync({ module: readFileSync(resolve(process.cwd(), "../../../crates/etendue-wasm/pkg/etendue_wasm_bg.wasm")) });
});

const twoStages: ProcedurePart = {
  stages: [
    { id: "intrinsics", problem: "planar_intrinsics" },
    { id: "handeye", problem: "single_cam_handeye", seed: { intrinsics: { stage: "intrinsics" } } },
  ],
};

describe("procedures", () => {
  it("adds the next stage, seeded from the last", () => {
    const p = addStage(addStage({ stages: [] }, "planar_intrinsics"), "rig_extrinsics");
    expect(p.stages).toEqual([
      { id: "intrinsics", problem: "planar_intrinsics" },
      { id: "rig", problem: "rig_extrinsics", seed: { intrinsics: { stage: "intrinsics" } } },
    ]);
    const q = addStage(p, "rig_handeye");
    expect(q.stages[2]).toEqual({
      id: "handeye",
      problem: "rig_handeye",
      seed: { intrinsics: { stage: "rig" }, rig: { stage: "rig" } },
    });
    expect(addStage(q, "rig_handeye").stages[3]!.id).toBe("handeye_2");
    expect(procedureIssues(q, validateLibraryItem)).toEqual([]);
  });

  it("renames and removes stages with the seeds that name them", () => {
    const renamed = renameStage(twoStages, 0, "k");
    expect(renamed.stages.map((s) => s.id)).toEqual(["k", "handeye"]);
    expect(renamed.stages[1]!.seed).toEqual({ intrinsics: { stage: "k" } });
    const removed = removeStage(twoStages, 0);
    expect(removed.stages).toEqual([{ id: "handeye", problem: "single_cam_handeye" }]);
  });

  it("drops what a new workflow does not take", () => {
    const stage = {
      id: "s",
      problem: "rig_handeye" as const,
      free: { refine_rig: true, refine_robot_poses: true },
      seed: { rig: { stage: "r" }, handeye: "nominal" as const, intrinsics: "truth" as const },
    };
    expect(withProblem(stage, "planar_intrinsics")).toEqual({
      id: "s",
      problem: "planar_intrinsics",
      seed: { intrinsics: "truth" },
    });
    expect(withProblem(stage, "single_cam_handeye").free).toEqual({ refine_robot_poses: true });
  });

  it("writes stages without empty parts", () => {
    expect(
      tidy({
        id: "s",
        problem: "planar_intrinsics",
        camera: null,
        model: { sensor: "design", fix: { intrinsics: null } },
        free: { fix_poses: null },
        solver: { max_iters: null, robust_loss: null },
        seed: { intrinsics: null },
      }),
    ).toEqual({ id: "s", problem: "planar_intrinsics" });
  });

  it("offers the freedoms and seeds that apply, as the kernel checks them", () => {
    // Every freedom set on a stage it applies to checks; on one it does not, the kernel says so.
    for (const f of FREEDOMS) {
      const value = f.kind === "bool" ? true : f.kind === "indices" ? [0] : 1;
      for (const problem of [
        "planar_intrinsics",
        "rig_extrinsics",
        "single_cam_handeye",
        "rig_handeye",
        "laserline_device",
        "rig_handeye_laserline",
      ] as const) {
        const issues = procedureIssues(
          { stages: [{ id: "s", problem, free: { [f.key]: value } }] },
          validateLibraryItem,
        );
        expect(issues.length === 0, `${f.key} on ${problem}: ${issues.join("; ")}`).toBe(f.applies(problem));
      }
    }
    expect(seedSlots("rig_handeye", true)).toEqual(["intrinsics", "sensor", "rig", "handeye"]);
    expect(seedSlots("planar_intrinsics", false)).toEqual(["intrinsics"]);
    expect(seedOf(seedValue({ stage: "a" }))).toEqual({ stage: "a" });
    expect(seedOf("")).toBeUndefined();
  });

  it("labels every option the kernel knows, and no other", () => {
    const known = new Set(procedureRules().problems.flatMap((p) => p.freedoms));
    expect(new Set(FREEDOMS.map((f) => f.key))).toEqual(known);
    expect(problemForDataset(1, false)).toBe("single_cam_handeye");
    expect(problemForDataset(3, true)).toBe("rig_handeye_laserline");
  });

  it("reports a seed that points forward after a move", () => {
    const moved = moveStage(twoStages, 1, -1);
    expect(moved.stages.map((s) => s.id)).toEqual(["handeye", "intrinsics"]);
    expect(procedureIssues(moved, validateLibraryItem)).toEqual([
      "stages[0].seed.intrinsics: `intrinsics` is not an earlier stage",
    ]);
  });
});
