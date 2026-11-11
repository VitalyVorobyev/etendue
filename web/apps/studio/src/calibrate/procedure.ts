/**
 * Calibration procedures (ADR 0011) as the studio edits them: what each option and workflow is
 * called, and edits that keep a procedure well formed (a renamed stage renames the seeds that
 * name it, a changed workflow drops what no longer applies). Which workflow takes which option
 * and seed is the kernel's (`procedureRules`), the same table its check (`validateLibraryItem`)
 * reads, so the editor offers exactly what checks.
 */

import {
  type Freedoms,
  type ProblemRules,
  type ProcedurePart,
  type Seed,
  type Seeds,
  type Stage,
  procedureRules,
} from "@etendue/wasm";

export type Problem = Stage["problem"];

let cached: ReturnType<typeof procedureRules> | undefined;
/** The kernel's rules (read once, after the kernel is initialised). */
const rules = () => (cached ??= procedureRules());
const ruleOf = (p: Problem): ProblemRules => {
  const r = rules().problems.find((x) => x.problem === p);
  if (!r) throw new Error(`the kernel has no rules for workflow \`${p}\``);
  return r;
};

/** The workflows, in the order a procedure usually runs them. */
export const PROBLEMS: readonly { value: Problem; label: string; description: string }[] = [
  { value: "planar_intrinsics", label: "Intrinsics", description: "One camera from planar views, no robot" },
  { value: "rig_extrinsics", label: "Rig", description: "A rig's cameras and their poses in it, no robot" },
  { value: "single_cam_handeye", label: "Hand-eye", description: "One camera and its hand-eye" },
  { value: "rig_handeye", label: "Rig hand-eye", description: "A rig and its hand-eye" },
  { value: "laserline_device", label: "Laser head", description: "A camera with its laser: intrinsics, tilt, plane" },
  { value: "rig_handeye_laserline", label: "Rig, lasers, hand-eye", description: "A rig with lasers, jointly" },
];

export const problemLabel = (p: Problem) => PROBLEMS.find((x) => x.value === p)?.label ?? p;

/** Whether the workflow calibrates several cameras at once. */
export const isRig = (p: Problem) => ruleOf(p).rig;
/** Whether it solves a hand-eye. */
export const isHandeye = (p: Problem) => ruleOf(p).handeye;
/** Whether it needs lasers. */
export const isLaser = (p: Problem) => ruleOf(p).laser;

/** The workflow that calibrates a dataset of this shape in one stage (`auto`'s choice). */
export function problemForDataset(cameras: number, lasers: boolean): Problem {
  const rig = cameras > 1;
  const r = rules().by_shape.find((x) => x.rig === rig && x.lasers === lasers);
  if (!r) throw new Error("the kernel has no workflow for this dataset's shape");
  return r.problem;
}

/** The parameters calibration-rs can hold fixed, per slot, by its own names. */
export const FIX_PARAMS = {
  intrinsics: ["fx", "fy", "cx", "cy"],
  distortion: ["k1", "k2", "k3", "p1", "p2"],
  sensor: ["tilt_x", "tilt_y"],
} as const;
export type FixSlot = keyof typeof FIX_PARAMS;

/** A freedom: what it is called and how it is entered. Where it applies is the kernel's. */
export interface FreedomSpec {
  key: keyof Freedoms;
  label: string;
  kind: "bool" | "number" | "indices";
  /** For numbers: shown in this unit, stored as `scale ×` the shown value. */
  unit?: string;
  scale?: number;
  /** Whether a stage of `p` takes it (the kernel's rules). */
  applies: (p: Problem) => boolean;
}

const spec = (s: Omit<FreedomSpec, "applies">): FreedomSpec => ({
  ...s,
  applies: (p) => ruleOf(p).freedoms.includes(s.key),
});

/** Every freedom, in the order the editor shows them (`Freedoms` in etendue-scene). */
export const FREEDOMS: readonly FreedomSpec[] = [
  spec({ key: "refine_intrinsics", label: "Refine intrinsics with the rig", kind: "bool" }),
  spec({ key: "refine_sensor", label: "Refine sensor tilt with the rig", kind: "bool" }),
  spec({ key: "refine_rig", label: "Refine the rig in the hand-eye", kind: "bool" }),
  spec({ key: "refine_robot_poses", label: "Refine robot poses", kind: "bool" }),
  spec({ key: "robot_rot_sigma_rad", label: "Robot rotation σ", kind: "number", unit: "°", scale: Math.PI / 180 }),
  spec({ key: "robot_trans_sigma_m", label: "Robot translation σ", kind: "number", unit: "mm", scale: 1e-3 }),
  spec({ key: "min_motion_angle_deg", label: "Least motion between views", kind: "number", unit: "°" }),
  spec({ key: "fix_handeye", label: "Hold the hand-eye", kind: "bool" }),
  spec({ key: "fix_target", label: "Hold the target", kind: "bool" }),
  spec({ key: "fix_plane", label: "Hold the laser plane", kind: "bool" }),
  spec({ key: "fix_poses", label: "Target poses held", kind: "indices" }),
];

/** The seed slots a stage of `problem` has (`Seeds`); `sensor` only where a tilt is calibrated. */
export function seedSlots(problem: Problem, tilted: boolean): (keyof Seeds)[] {
  const r = ruleOf(problem);
  return (tilted ? r.seed_slots_tilted : r.seed_slots) as (keyof Seeds)[];
}

/** What a seed slot may start from (a rig only from an earlier stage). */
export const seedKinds = (slot: keyof Seeds): ("nominal" | "truth" | "stage")[] =>
  (rules().seed_kinds.find((x) => x.slot === slot)?.kinds ?? []) as ("nominal" | "truth" | "stage")[];

/** A seed as a select value: `""` (the workflow's own initialisation), `nominal`, `truth`, `stage:<id>`. */
export const seedValue = (s: Seed | null | undefined): string =>
  s == null ? "" : typeof s === "string" ? s : `stage:${s.stage}`;
export const seedOf = (v: string): Seed | undefined =>
  v === "" ? undefined : v === "nominal" || v === "truth" ? v : { stage: v.slice("stage:".length) };

/** `stage` without empty parts, as etendue writes it (absent, not `null` or `{}`). */
export function tidy(stage: Stage): Stage {
  const out: Stage = { id: stage.id, problem: stage.problem };
  if (stage.camera) out.camera = stage.camera;
  const fix = Object.fromEntries(Object.entries(stage.model?.fix ?? {}).filter(([, v]) => v != null));
  const sensor = stage.model?.sensor ?? "design";
  if (sensor !== "design" || Object.keys(fix).length > 0) {
    out.model = { sensor, ...(Object.keys(fix).length > 0 ? { fix } : {}) };
  }
  const free = Object.fromEntries(Object.entries(stage.free ?? {}).filter(([, v]) => v != null));
  if (Object.keys(free).length > 0) out.free = free;
  const solver = Object.fromEntries(Object.entries(stage.solver ?? {}).filter(([, v]) => v != null));
  if (Object.keys(solver).length > 0) out.solver = solver;
  const seed = Object.fromEntries(Object.entries(stage.seed ?? {}).filter(([, v]) => v != null));
  if (Object.keys(seed).length > 0) out.seed = seed;
  return out;
}

/** `stage` running `problem`: what does not apply to it is dropped. */
export function withProblem(stage: Stage, problem: Problem): Stage {
  const free: Freedoms = {};
  for (const f of FREEDOMS) {
    const v = stage.free?.[f.key];
    if (v != null && f.applies(problem)) (free as Record<string, unknown>)[f.key] = v;
  }
  const slots = new Set<string>(ruleOf(problem).seed_slots_tilted);
  const seed = Object.fromEntries(Object.entries(stage.seed ?? {}).filter(([k]) => slots.has(k))) as Seeds;
  return tidy({
    ...stage,
    problem,
    ...(isRig(problem) ? { camera: null } : {}),
    free,
    seed,
  });
}

/** A stage id not yet in `stages`, from `base`. */
export function freeStageId(stages: readonly Stage[], base: string): string {
  const ids = new Set(stages.map((s) => s.id));
  if (!ids.has(base)) return base;
  let n = 2;
  while (ids.has(`${base}_${n}`)) n += 1;
  return `${base}_${n}`;
}

const STAGE_IDS: Record<Problem, string> = {
  planar_intrinsics: "intrinsics",
  rig_extrinsics: "rig",
  single_cam_handeye: "handeye",
  rig_handeye: "handeye",
  laserline_device: "device",
  rig_handeye_laserline: "laser",
};

/** A new stage after the last: the next workflow a procedure usually runs, seeded from the last stage. */
export function addStage(procedure: ProcedurePart, problem: Problem): ProcedurePart {
  const last = procedure.stages.at(-1);
  const stage: Stage = { id: freeStageId(procedure.stages, STAGE_IDS[problem]), problem };
  if (last) {
    const seed: Seeds = { intrinsics: { stage: last.id } };
    if (isRig(problem) && isRig(last.problem)) seed.rig = { stage: last.id };
    stage.seed = seed;
  }
  return { stages: [...procedure.stages, tidy(stage)] };
}

/** Whether seed `v` is stage `id`'s estimate. */
const fromStage = (v: Seed | null | undefined, id: string | undefined) =>
  v != null && typeof v === "object" && v.stage === id;

const seedEntries = (seeds: Seeds) => Object.entries(seeds) as [keyof Seeds, Seed | null | undefined][];

/** Stage `index` renamed `id`, and every seed that named it with it. */
export function renameStage(procedure: ProcedurePart, index: number, id: string): ProcedurePart {
  const old = procedure.stages[index]?.id;
  return {
    stages: procedure.stages.map((s, i) => {
      if (i === index) return { ...s, id };
      if (!s.seed || old === undefined) return s;
      const seed = Object.fromEntries(
        seedEntries(s.seed).map(([k, v]) => [k, fromStage(v, old) ? { stage: id } : v]),
      ) as Seeds;
      return { ...s, seed };
    }),
  };
}

/** Stage `index` removed, and the seeds that named it with it (they fall back to the workflow's own). */
export function removeStage(procedure: ProcedurePart, index: number): ProcedurePart {
  const gone = procedure.stages[index]?.id;
  return {
    stages: procedure.stages
      .filter((_, i) => i !== index)
      .map((s) => {
        if (!s.seed) return s;
        const seed = Object.fromEntries(seedEntries(s.seed).filter(([, v]) => !fromStage(v, gone))) as Seeds;
        return tidy({ ...s, seed });
      }),
  };
}

/** Stage `index` moved by `by` places (the kernel's check names seeds that now point forward). */
export function moveStage(procedure: ProcedurePart, index: number, by: -1 | 1): ProcedurePart {
  const to = index + by;
  if (to < 0 || to >= procedure.stages.length) return procedure;
  const stages = [...procedure.stages];
  const [s] = stages.splice(index, 1);
  stages.splice(to, 0, s!);
  return { stages };
}

/** The kernel's issues with `procedure` (as a library item), one per line; empty when it checks. */
export function procedureIssues(procedure: ProcedurePart, validate: (text: string) => unknown): string[] {
  try {
    validate(JSON.stringify({ version: 2, id: "procedure", name: "procedure", procedure }));
    return [];
  } catch (e) {
    const text = e instanceof Error ? e.message : String(e);
    // "library item: 2 problem(s)\n  procedure.stages[0].seed.intrinsics: …": one issue per
    // indented line, under a heading; a parse error is one line.
    const lines = text.split("\n");
    const issues = lines
      .slice(1)
      .map((l) => l.trim().replace(/^procedure\./, ""))
      .filter((l) => l !== "");
    return issues.length > 0 ? issues : [text];
  }
}
