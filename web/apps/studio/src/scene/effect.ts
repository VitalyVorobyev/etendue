/**
 * What a scene edit does to the bake. The bake (frame poses at every sample) depends only on the
 * frame tree, the robots and the scenario. An edit of anything else (intrinsics, a laser's fan,
 * a target's print, a light) leaves every baked pose as it is. An edit that only moves placed
 * things changes poses but not the frame set, and is previewed without a bake (`PoseOverride`).
 */

import type { Iso3Schema, SceneSpec } from "@etendue/wasm";

/** One entity whose placement changed: in `parent`, from `from` to `to` (both `parent_se3_self`). */
export interface Moved {
  id: string;
  parent: string;
  from: Iso3Schema;
  to: Iso3Schema;
}

/**
 * - `none`: the scenes are the same document.
 * - `params`: no baked pose changes; the kernel's scene is replaced, the bake kept.
 * - `poses`: only placements changed (`moved`, in scene order); a bake gives the exact poses.
 * - `bake`: the frame set, a parent, a robot or a rig layout changed.
 */
export type SceneEffect = { kind: "none" } | { kind: "params" } | { kind: "poses"; moved: Moved[] } | { kind: "bake" };

interface Placed {
  id: string;
  parent: string;
  parent_se3_self: Iso3Schema;
}

/** Every placed entity of the scene, in a fixed order, with whether its other fields shape the frame tree. */
function placed(scene: SceneSpec): { entity: Placed; kinematic: boolean }[] {
  const of = (list: readonly Placed[] | undefined, kinematic: boolean) =>
    (list ?? []).map((entity) => ({ entity, kinematic }));
  return [
    // A frame is nothing but its placement; a robot's joints, its rest pose and its manifest, and
    // a rig's layout, all shape the frames below them.
    ...of(scene.frames, true),
    ...of(scene.robots, true),
    ...of(scene.rigs, true),
    ...of(scene.cameras, false),
    ...of(scene.lasers, false),
    ...of(scene.lights, false),
    ...of(scene.targets, false),
    ...of(scene.parts, false),
  ];
}

function samePose(a: Iso3Schema, b: Iso3Schema): boolean {
  return a.rotation.every((x, i) => x === b.rotation[i]) && a.translation.every((x, i) => x === b.translation[i]);
}

/** Every field of `entity` but its placement, as text. */
function unplaced(entity: Placed): string {
  const { parent_se3_self: _, ...rest } = entity;
  return JSON.stringify(rest);
}

/** The fields of `entity` that shape the bake, as text (its placement excluded). */
function shape(entity: Placed, kinematic: boolean): string {
  return kinematic ? unplaced(entity) : `${entity.id}\u0000${entity.parent}`;
}

/** What changing `before` into `after` does to the bake. */
export function sceneEffect(before: SceneSpec, after: SceneSpec): SceneEffect {
  if (before === after) return { kind: "none" };
  const a = placed(before);
  const b = placed(after);
  if (a.length !== b.length) return { kind: "bake" };
  const moved: Moved[] = [];
  let params = false;
  for (let i = 0; i < a.length; i++) {
    const x = a[i]!;
    const y = b[i]!;
    if (x.entity === y.entity) continue;
    if (x.kinematic !== y.kinematic || shape(x.entity, x.kinematic) !== shape(y.entity, y.kinematic)) {
      return { kind: "bake" };
    }
    if (!samePose(x.entity.parent_se3_self, y.entity.parent_se3_self)) {
      moved.push({
        id: y.entity.id,
        parent: y.entity.parent,
        from: x.entity.parent_se3_self,
        to: y.entity.parent_se3_self,
      });
    }
    if (!x.kinematic && unplaced(x.entity) !== unplaced(y.entity)) params = true;
  }
  // Moves are shown by overrides of the baked poses, parameters by the kernel's scene: both at
  // once would leave the kernel's scene out of step with the bake.
  if (moved.length > 0) return params ? { kind: "bake" } : { kind: "poses", moved };
  return { kind: "params" };
}

/** The entity id a frame belongs to: `ur5e/tool0` belongs to robot `ur5e`. */
export function ownerOf(frame: string): string {
  const slash = frame.indexOf("/");
  return slash < 0 ? frame : frame.slice(0, slash);
}

/**
 * The baked frames that move with entity `id`: its own frames (a robot's links) and everything
 * attached below them, at any depth.
 */
export function followers(scene: SceneSpec, frames: readonly string[], id: string): Set<string> {
  const owners = new Set([id]);
  const all = placed(scene).map((p) => p.entity);
  for (let grew = true; grew;) {
    grew = false;
    for (const e of all) {
      if (!owners.has(e.id) && owners.has(ownerOf(e.parent))) {
        owners.add(e.id);
        grew = true;
      }
    }
  }
  return new Set(frames.filter((f) => owners.has(ownerOf(f))));
}
