/**
 * The cell builder's choices and the scene they make. A cell is assembled from library items —
 * optics (a camera or a head), a rig (one camera, or a rig design's layout), a mount, a target —
 * and a robot from the robot library, mounted eye-in-hand (the rig on the robot's flange) or
 * eye-to-hand (the rig on a fixed stand, the target on the flange). The result is an ordinary
 * scene document and an empty scenario: nothing in them refers back to the builder.
 *
 * Pure: the rig layout's expansion (the kernel's `RigLayout::expand`) is passed in.
 */

import type {
  CameraSpec,
  CameraTemplate,
  Iso3Schema,
  LaserSpec,
  LibraryItem,
  RigLayout,
  ScenarioSpec,
  SceneSpec,
} from "@etendue/wasm";

import { type RigUnit, unitOfItem } from "../rigs/design";

export type Mounting = "eye_in_hand" | "eye_to_hand";

/** The builder's steps, in order. */
export const STEPS = ["optics", "rig", "mounting", "target", "robot"] as const;
export type StepId = (typeof STEPS)[number];

/** One position of a rig design: its camera's id, and a library camera that replaces the optics there. */
export interface Position {
  id: string;
  /** A library camera's reference; `null` keeps the optics of the optics step. */
  camera: string | null;
}

/** Everything the builder asks. Library items are named by reference (`cameras/…`). */
export interface CellDraft {
  description: string;
  /** A camera or a head; `null` uses a rig design's own cameras. */
  optics: string | null;
  rig: {
    id: string;
    /** A rig design; `null` for one camera (or head) on the rig. */
    design: string | null;
    /** The one camera's id, without a design. */
    cameraId: string;
    /** Per position of the design (as many as it has cameras). */
    positions: Position[];
  };
  mounting: Mounting;
  /** What sits on the flange (the rig eye-in-hand, the target eye-to-hand) goes through this mount. */
  mount: string | null;
  /** The flange-to-device pose without a mount. */
  flange: Iso3Schema;
  /** Eye-to-hand: the stand on the world, and the rig on it. */
  stand: { id: string; pose: Iso3Schema };
  rigOnStand: Iso3Schema;
  /** The target; eye-in-hand it stands on the world at `pose`. */
  target: { id: string; item: string | null; pose: Iso3Schema };
  /** A robot from the robot library (its repository-relative manifest), placed on the world. */
  robot: { id: string; manifest: string | null; base: Iso3Schema; q: number[] };
  /** The distance the target is judged at; `null` takes the optics' (see `nominalDistance`). */
  workingDistance: number | null;
}

/** What the assembly needs besides the draft. */
export interface AssemblyContext {
  /** Every library item the project sees, by reference. */
  library: ReadonlyMap<string, LibraryItem>;
  /** The robot's tool flange link (`tcp_link` of its manifest), once the robot has loaded. */
  tcpLink: string | null;
  /** The kernel's expansion of a rig layout. */
  expand: (layout: RigLayout, rigId: string) => { cameras: CameraSpec[]; lasers: LaserSpec[] };
}

/** Why the draft does not make a cell yet, by step. */
export interface Problem {
  step: StepId;
  message: string;
}

/** A built cell: the scene, its (empty) scenario, and the library items it used. */
export interface Cell {
  scene: SceneSpec;
  scenario: ScenarioSpec;
  /** The library references the cell was made from (to copy into the project). */
  used: string[];
}

export const IDENTITY: Iso3Schema = { rotation: [0, 0, 0, 1], translation: [0, 0, 0] };

/** An empty program: the cell at the robot's start pose. */
export const EMPTY_SCENARIO: ScenarioSpec = { version: 1, dt: 0.01, steps: [] };

/** A camera's default id at position `i` of rig `rig` (the kernel's `<rig>_cam<i>`). */
export const layoutCameraId = (rig: string, i: number) => `${rig}_cam${i}`;

const VALID_ID = /^[A-Za-z0-9_-]+$/;

/** The unit (camera or head) the optics step chose, if it chose one that is a camera or head. */
export function opticsUnit(draft: CellDraft, library: ReadonlyMap<string, LibraryItem>): RigUnit | null {
  const item = draft.optics ? library.get(draft.optics) : undefined;
  return item ? unitOfItem(item) : null;
}

/** The rig design's layout with the chosen optics at every position. */
export function designLayout(design: RigLayout, unit: RigUnit | null): RigLayout {
  if (!unit) return design;
  const layout: RigLayout = { ...design, camera: unit.camera };
  // A head brings its laser, fixed to each camera's body; a camera keeps the design's lasers.
  if (unit.laser) layout.lasers = unit.laser;
  return layout;
}

/** `spec` (placed by the layout) carrying `template` instead. */
function cameraOf(spec: CameraSpec, template: CameraTemplate): CameraSpec {
  return {
    id: spec.id,
    parent: spec.parent,
    parent_se3_self: spec.parent_se3_self,
    params: template.params,
    resolution: template.resolution,
    ...(template.optics ? { optics: template.optics } : {}),
  };
}

/**
 * The scene the draft makes, or why it makes none yet. Every id is the draft's; the frame tree is:
 *
 * - eye-in-hand: `robot` on the world, the rig on `<robot>/<flange>` (at the mount's pose), its
 *   cameras (and lasers) on the rig, the target on the world;
 * - eye-to-hand: the stand on the world, the rig on the stand, the target on the flange.
 */
export function assembleCell(draft: CellDraft, ctx: AssemblyContext): { cell: Cell | null; problems: Problem[] } {
  const problems: Problem[] = [];
  const problem = (step: StepId, message: string) => problems.push({ step, message });
  const used: string[] = [];
  const item = (ref: string | null): LibraryItem | undefined => {
    const found = ref ? ctx.library.get(ref) : undefined;
    if (found && ref && !used.includes(ref)) used.push(ref);
    return found;
  };

  // Optics.
  const opticsItem = item(draft.optics);
  const unit = opticsItem ? unitOfItem(opticsItem) : null;
  if (draft.optics && !unit) problem("optics", "Choose a camera or a head.");
  const designItem = item(draft.rig.design);
  const design = designItem?.rig?.layout;
  if (draft.rig.design && !design) problem("rig", "Choose a rig design.");
  if (!unit && !draft.rig.design) problem("optics", "Choose a camera or a head, or a rig design in the next step.");

  // Rig: its cameras and lasers.
  const rigId = draft.rig.id;
  let cameras: CameraSpec[] = [];
  let lasers: LaserSpec[] = [];
  let layout: RigLayout | null = null;
  if (design) {
    const laidOut = designLayout(design, unit);
    const expanded = ctx.expand(laidOut, rigId);
    const positions = expanded.cameras.map(
      (_, i) => draft.rig.positions[i] ?? { id: layoutCameraId(rigId, i), camera: null },
    );
    const renamed = new Map<string, string>();
    cameras = expanded.cameras.map((c, i) => {
      const position = positions[i]!;
      const override = item(position.camera);
      if (position.camera && !override?.camera) problem("rig", `Position ${i + 1}: choose a camera.`);
      if (position.id !== c.id) renamed.set(c.id, position.id);
      const spec = override?.camera ? cameraOf(c, override.camera) : c;
      return { ...spec, id: position.id };
    });
    const follow = (id: string) => renamed.get(id) ?? id;
    lasers = expanded.lasers.map((l) => ({
      ...l,
      parent: follow(l.parent),
      ...(l.camera != null ? { camera: follow(l.camera) } : {}),
    }));
    // The rig keeps its layout only while its cameras are the layout's own: renamed or
    // individually chosen cameras would be lost when the layout is edited again.
    const own = positions.every((p, i) => p.id === layoutCameraId(rigId, i) && p.camera === null);
    layout = own ? laidOut : null;
  } else if (unit) {
    const id = draft.rig.cameraId;
    cameras = [
      {
        id,
        parent: rigId,
        parent_se3_self: IDENTITY,
        params: unit.camera.params,
        resolution: unit.camera.resolution,
        ...(unit.camera.optics ? { optics: unit.camera.optics } : {}),
      },
    ];
    const laser = unit.laser;
    if (laser) {
      lasers = [
        {
          id: `${id}_laser`,
          parent: id,
          parent_se3_self: laser.camera_se3_laser ?? IDENTITY,
          fan_half_angle: laser.fan_half_angle,
          fan_length: laser.fan_length,
          wavelength_nm: laser.wavelength_nm,
          beam_waist_m: laser.beam_waist_m,
          ...(laser.waist_distance_m != null ? { waist_distance_m: laser.waist_distance_m } : {}),
          ...(laser.power_w != null ? { power_w: laser.power_w } : {}),
          camera: id,
        },
      ];
    }
  }

  // Mounting.
  const mountItem = item(draft.mount);
  if (draft.mount && !mountItem?.mount) problem("mounting", "Choose a mount.");
  const onFlange = mountItem?.mount?.flange_se3_device ?? draft.flange;
  const eyeInHand = draft.mounting === "eye_in_hand";

  // Target.
  const targetItem = item(draft.target.item);
  const geometry = targetItem?.target?.geometry;
  if (!geometry) problem("target", "Choose a target.");

  // Robot.
  const robot = draft.robot;
  if (!robot.manifest) problem("robot", "Choose a robot.");
  else if (ctx.tcpLink === null) problem("robot", "Reading the robot…");
  const flange = `${robot.id}/${ctx.tcpLink ?? "tool0"}`;

  // Ids: one namespace.
  const ids: [StepId, string][] = [
    ["robot", robot.id],
    ["rig", rigId],
    ...cameras.map((c): [StepId, string] => ["rig", c.id]),
    ...lasers.map((l): [StepId, string] => ["rig", l.id]),
    ["target", draft.target.id],
    ...(eyeInHand ? [] : [["mounting", draft.stand.id] as [StepId, string]]),
  ];
  const seen = new Set<string>();
  for (const [step, id] of ids) {
    if (!VALID_ID.test(id) || id === "world") problem(step, `\`${id}\` is not an id: use letters, digits, _ and -.`);
    else if (seen.has(id)) problem(step, `\`${id}\` is used twice.`);
    seen.add(id);
  }

  if (problems.length > 0 || !geometry || !robot.manifest) return { cell: null, problems };

  const scene: SceneSpec = {
    version: 1,
    ...(draft.description.trim() !== "" ? { description: draft.description } : {}),
    ...(eyeInHand ? {} : { frames: [{ id: draft.stand.id, parent: "world", parent_se3_self: draft.stand.pose }] }),
    robots: [
      {
        id: robot.id,
        parent: "world",
        parent_se3_self: robot.base,
        // Repository-relative, as the robot library names it: the loader finds it there, and Save
        // makes it relative to where the scene is written.
        manifest: robot.manifest,
        initial_q: robot.q,
      },
    ],
    rigs: [
      {
        id: rigId,
        parent: eyeInHand ? flange : draft.stand.id,
        parent_se3_self: eyeInHand ? onFlange : draft.rigOnStand,
        ...(layout ? { layout } : {}),
      },
    ],
    cameras,
    ...(lasers.length > 0 ? { lasers } : {}),
    targets: [
      {
        id: draft.target.id,
        parent: eyeInHand ? "world" : flange,
        parent_se3_self: eyeInHand ? draft.target.pose : onFlange,
        geometry,
      },
    ],
  };
  return { cell: { scene, scenario: EMPTY_SCENARIO, used }, problems };
}

/**
 * The distance the target is judged at when the draft names none: the rig design's working
 * distance, a head's window (its middle), a physical camera's focus distance, else 0.5 m.
 */
export function nominalDistance(draft: CellDraft, library: ReadonlyMap<string, LibraryItem>): number {
  if (draft.workingDistance !== null) return draft.workingDistance;
  const design = draft.rig.design ? library.get(draft.rig.design)?.rig?.layout : undefined;
  if (design) return design.working_distance_m;
  const optics = draft.optics ? library.get(draft.optics) : undefined;
  const window = optics?.head?.window;
  if (window) return (window.near_m + window.far_m) / 2;
  const focus = (optics?.camera ?? optics?.head?.camera)?.optics?.focus_distance_m;
  return focus ?? 0.5;
}

/** Below this fraction of the image width a board is small for calibration; above the other, it leaves no margin. */
export const COVERAGE = { min: 0.2, max: 0.9 } as const;

/** How much of a camera's image width a target spans at a distance (pinhole, along the axis). */
export interface Coverage {
  /** The target's longer side over the width the image sees there. */
  fraction: number;
  /** The width the image sees at the distance, metres. */
  fov_m: number;
  verdict: "small" | "ok" | "large";
}

/**
 * How much of `camera`'s image width a target of `extent` (metres, `[x, y]`) covers, facing the
 * camera at `distance` metres: its longer side against `width / fx · distance`.
 */
export function targetCoverage(extent: readonly [number, number], camera: CameraTemplate, distance: number): Coverage {
  const fov_m = (camera.resolution[0] / camera.params.intrinsics.fx) * distance;
  const fraction = Math.max(extent[0], extent[1]) / fov_m;
  return {
    fraction,
    fov_m,
    verdict: fraction < COVERAGE.min ? "small" : fraction > COVERAGE.max ? "large" : "ok",
  };
}

/** The positions of a rig design with `count` cameras on rig `rig`: the layout's ids, the optics everywhere. */
export function defaultPositions(rig: string, count: number): Position[] {
  return Array.from({ length: count }, (_, i) => ({ id: layoutCameraId(rig, i), camera: null }));
}

/** `positions` for a rig renamed from `from` to `to`: positions still named as the layout names them follow. */
export function renamePositions(positions: Position[], from: string, to: string): Position[] {
  return positions.map((p, i) => (p.id === layoutCameraId(from, i) ? { ...p, id: layoutCameraId(to, i) } : p));
}
