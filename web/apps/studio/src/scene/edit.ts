/**
 * Edits of a scene document: add, change, rename and remove placed entities. Pure functions on
 * `SceneSpec` that return a new scene (new objects for what changed, the rest shared), so the
 * studio's undo history can keep every version. Validation is the kernel's: an edit that breaks
 * the scene (a dangling parent, say) is refused when it is baked.
 */

import type {
  CameraSpec,
  LibraryItem,
  ScenarioSpec,
  FrameSpec,
  Iso3Schema,
  LaserSpec,
  LightSpec,
  RigLayout,
  RigSpec,
  SceneSpec,
  TargetSpec,
} from "@etendue/wasm";

/** The scene's lists of placed entities that the studio edits (robots have their own edits). */
export type EntityList = "frames" | "rigs" | "cameras" | "lasers" | "lights" | "targets";

/** An entity of one of those lists. */
export type Entity = FrameSpec | RigSpec | CameraSpec | LaserSpec | LightSpec | TargetSpec;

/** Every list, in the scene's order. */
export const ENTITY_LISTS: readonly EntityList[] = ["frames", "rigs", "cameras", "lasers", "lights", "targets"];

export const IDENTITY: Iso3Schema = { rotation: [0, 0, 0, 1], translation: [0, 0, 0] };

/** Every id in the scene's one namespace (frames, robots, rigs, cameras, lasers, lights, targets, parts). */
export function sceneIds(scene: SceneSpec): Set<string> {
  return new Set(
    [
      ...(scene.frames ?? []),
      ...(scene.robots ?? []),
      ...(scene.rigs ?? []),
      ...(scene.cameras ?? []),
      ...(scene.lasers ?? []),
      ...(scene.lights ?? []),
      ...(scene.targets ?? []),
      ...(scene.parts ?? []),
    ].map((e) => e.id),
  );
}

/** `base` if unused, else `base_2`, `base_3`, … */
export function uniqueId(scene: SceneSpec, base: string): string {
  const ids = sceneIds(scene);
  if (!ids.has(base)) return base;
  for (let n = 2; ; n++) if (!ids.has(`${base}_${n}`)) return `${base}_${n}`;
}

/** Which list holds `id`, if any. */
export function listOf(scene: SceneSpec, id: string): EntityList | null {
  return ENTITY_LISTS.find((l) => scene[l]?.some((e) => e.id === id)) ?? null;
}

/** The entity `id`, if it is in one of the edited lists. */
export function entityOf(scene: SceneSpec, id: string): Entity | null {
  const list = listOf(scene, id);
  return list === null ? null : ((scene[list] as Entity[]).find((e) => e.id === id) ?? null);
}

function mapList(scene: SceneSpec, list: EntityList, f: (e: Entity) => Entity): SceneSpec {
  const items: Entity[] | undefined = scene[list];
  if (!items) return scene;
  let changed = false;
  const next = items.map((e) => {
    const n = f(e);
    if (n !== e) changed = true;
    return n;
  });
  return changed ? { ...scene, [list]: next } : scene;
}

/** Replace entity `id` with `change(entity)`. */
export function updateEntity<E extends Entity>(scene: SceneSpec, id: string, change: (e: E) => E): SceneSpec {
  const list = listOf(scene, id);
  if (list === null) return scene;
  return mapList(scene, list, (e) => (e.id === id ? change(e as E) : e));
}

/** Add `entity` to `list`. */
export function addEntity(scene: SceneSpec, list: EntityList, entity: Entity): SceneSpec {
  return { ...scene, [list]: [...(scene[list] ?? []), entity] };
}

/** What is attached to `id` (by `parent`), robots and parts included. */
export function childrenOf(scene: SceneSpec, id: string): string[] {
  return [
    ...(scene.frames ?? []),
    ...(scene.robots ?? []),
    ...(scene.rigs ?? []),
    ...(scene.cameras ?? []),
    ...(scene.lasers ?? []),
    ...(scene.lights ?? []),
    ...(scene.targets ?? []),
    ...(scene.parts ?? []),
  ]
    .filter((e) => e.parent === id)
    .map((e) => e.id);
}

/** Remove entity `id` and everything attached to it (recursively, within the edited lists). */
export function removeEntity(scene: SceneSpec, id: string): SceneSpec {
  const doomed = new Set<string>([id]);
  for (let grew = true; grew;) {
    grew = false;
    for (const list of ENTITY_LISTS) {
      for (const e of scene[list] ?? []) {
        if (!doomed.has(e.id) && doomed.has(e.parent)) {
          doomed.add(e.id);
          grew = true;
        }
      }
    }
  }
  let next = scene;
  for (const list of ENTITY_LISTS) {
    const items: Entity[] | undefined = next[list];
    if (items?.some((e) => doomed.has(e.id))) next = { ...next, [list]: items.filter((e) => !doomed.has(e.id)) };
  }
  // A laser linked to a removed camera keeps working, unlinked.
  return next;
}

/**
 * Rename entity `from` to `to`, and every `parent` that names it. Fails (returns `null`) if `to`
 * is taken or empty.
 */
export function renameEntity(scene: SceneSpec, from: string, to: string): SceneSpec | null {
  if (to === from) return scene;
  if (to.trim() === "" || sceneIds(scene).has(to)) return null;
  let next = updateEntity(scene, from, (e) => ({ ...e, id: to }));
  for (const list of ENTITY_LISTS) next = mapList(next, list, (e) => (e.parent === from ? { ...e, parent: to } : e));
  if (next.parts?.some((p) => p.parent === from)) {
    next = { ...next, parts: next.parts.map((p) => (p.parent === from ? { ...p, parent: to } : p)) };
  }
  if (next.robots?.some((r) => r.parent === from)) {
    next = { ...next, robots: next.robots.map((r) => (r.parent === from ? { ...r, parent: to } : r)) };
  }
  return next;
}

/**
 * What new cameras, lasers and targets are made from: the project's default library items
 * (ADR 0009). Without one, a minimal built-in value is used.
 */
export interface EntityDefaults {
  camera?: LibraryItem | undefined;
  laser?: LibraryItem | undefined;
  target?: LibraryItem | undefined;
}

/** A new entity of `list` with defaults, attached to `parent`, with a fresh id. */
export function newEntity(scene: SceneSpec, list: EntityList, parent: string, defaults: EntityDefaults = {}): Entity {
  const base = { parent, parent_se3_self: IDENTITY };
  const camera = defaults.camera?.camera;
  const laser = defaults.laser?.laser;
  const target = defaults.target?.target;
  switch (list) {
    case "frames":
      return { id: uniqueId(scene, "frame"), ...base };
    case "rigs":
      return { id: uniqueId(scene, "rig"), ...base };
    case "cameras":
      if (camera) {
        return {
          id: uniqueId(scene, "cam"),
          ...base,
          params: camera.params,
          resolution: camera.resolution,
          ...(camera.optics ? { optics: camera.optics } : {}),
        };
      }
      return {
        id: uniqueId(scene, "cam"),
        ...base,
        params: {
          projection: { type: "pinhole" },
          distortion: { type: "none" },
          sensor: { type: "identity" },
          intrinsics: { type: "fx_fy_cx_cy_skew", fx: 2400, fy: 2400, cx: 640, cy: 512, skew: 0 },
        },
        resolution: [1280, 1024],
      };
    case "lasers":
      return {
        id: uniqueId(scene, "laser"),
        ...base,
        fan_half_angle: laser?.fan_half_angle ?? (20 * Math.PI) / 180,
        fan_length: 1,
        wavelength_nm: laser?.wavelength_nm ?? 660,
        beam_waist_m: laser?.beam_waist_m ?? 0.0005,
        ...(laser?.waist_distance_m != null ? { waist_distance_m: laser.waist_distance_m } : {}),
      };
    case "lights":
      return {
        id: uniqueId(scene, "light"),
        ...base,
        power_w: 50,
        color: [1, 1, 1],
        shape: { type: "point", radius_m: 0.05 },
      };
    case "targets":
      return {
        id: uniqueId(scene, "board"),
        ...base,
        geometry: target?.geometry ?? {
          type: "printed",
          target: { kind: "chessboard", inner_rows: 6, inner_cols: 9, square_size_mm: 25 },
        },
      };
  }
}

/** A robot as placed in a scene. */
export type RobotSpec = NonNullable<SceneSpec["robots"]>[number];

/**
 * Remove robot `id`: its placement, everything attached to its links (recursively), and the
 * scenario steps that move it.
 */
export function removeRobot(
  scene: SceneSpec,
  scenario: ScenarioSpec | null,
  id: string,
): { scene: SceneSpec; scenario: ScenarioSpec | null } {
  let next: SceneSpec = { ...scene, robots: (scene.robots ?? []).filter((r) => r.id !== id) };
  for (const list of ENTITY_LISTS) {
    for (const e of next[list] ?? []) {
      if (e.parent.startsWith(`${id}/`)) next = removeEntity(next, e.id);
    }
  }
  const steps = scenario?.steps.filter((s) => !("robot" in s) || s.robot !== id);
  return {
    scene: next,
    scenario: scenario && steps && steps.length !== scenario.steps.length ? { ...scenario, steps } : scenario,
  };
}

/**
 * Lay out rig `rigId`: its cameras and lasers become `expanded` (from `expandRig`), and the rig
 * keeps `layout`. The cameras and lasers attached to the rig before are removed, with whatever
 * hangs on them.
 */
export function applyLayout(
  scene: SceneSpec,
  rigId: string,
  layout: RigLayout,
  expanded: { cameras: CameraSpec[]; lasers: LaserSpec[] },
): SceneSpec {
  let next = scene;
  for (const e of [...(scene.cameras ?? []), ...(scene.lasers ?? [])]) {
    if (e.parent === rigId) next = removeEntity(next, e.id);
  }
  next = updateEntity<RigSpec>(next, rigId, (r) => ({ ...r, layout }));
  return {
    ...next,
    cameras: [...(next.cameras ?? []), ...expanded.cameras],
    lasers: [...(next.lasers ?? []), ...expanded.lasers],
  };
}
