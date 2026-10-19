/**
 * Poses of an edit in progress, without a bake.
 *
 * Moving entity F from `parent_se3_self = P_old` to `P_new` changes the world pose of F and of
 * every frame below it by one rigid correction, at every sample:
 *
 * ```text
 * W_F  = W_parent · P_old            W_F' = W_parent · P_new
 * G    = W_parent · P_new · P_old⁻¹ · W_parent⁻¹,      W_D' = G · W_D  for D = F and below
 * ```
 *
 * This is exact for everything that hangs on F, robot links included, as long as the scenario
 * does not depend on F's pose (a robot told to reach a moved frame re-plans only when the edit
 * is committed and baked).
 *
 * The studio's interim home for what `FrameTreeRuntime` should do itself (a lab-ui change).
 */

import { type FrameTreeRuntime, type Iso3, composeIso3, invertIso3, matrixFromIso3 } from "@vitavision/three";

import type { Moved } from "../scene/effect";

/** A placement change shown without a bake. */
export interface PoseOverride {
  /** The moved entity's parent frame. */
  parent: string;
  /** `P_new · P_old⁻¹`, in the parent frame. */
  local: Iso3;
  /** The frames that move: the entity's own and every one below. */
  moves: ReadonlySet<string>;
}

/** Where world poses are read: a frame tree runtime, or one with overrides on top. */
export interface PoseSource {
  /** `world_se3_frame` of `name` at sample `k` (default: the applied one). */
  pose(name: string, k?: number): Iso3 | undefined;
}

/** The overrides of moving `moved` (in scene order: a parent before what hangs on it). */
export function overridesOf(moved: readonly Moved[], followers: (id: string) => ReadonlySet<string>): PoseOverride[] {
  return moved.map((m) => ({
    parent: m.parent,
    local: composeIso3(m.to, invertIso3(m.from)),
    moves: followers(m.id),
  }));
}

/**
 * Per camera with physical optics: `P_se3_M`, its body in its projection frame. The kernel bakes
 * such a camera's frame at its entrance pupil P (ADR 0008); the document places it, and whatever
 * hangs on it, by its body M.
 */
export type Bodies = ReadonlyMap<string, Iso3>;

const NO_BODIES: Bodies = new Map();

/**
 * `runtime`'s poses with `overrides` applied. With none, the runtime itself. A moved entity's
 * parent pose is its body's when the parent is a camera with optics (`bodies`): that is the
 * frame its placement is in.
 */
export function posed(
  runtime: FrameTreeRuntime,
  overrides: readonly PoseOverride[],
  bodies: Bodies = NO_BODIES,
): PoseSource {
  if (overrides.length === 0) return runtime;
  return {
    pose: (name: string, k: number = runtime.current) => {
      // Each correction is computed from the poses the earlier ones gave.
      const at = (frame: string, upTo: number): Iso3 | undefined => {
        let p = runtime.pose(frame, k);
        for (let i = 0; i < upTo && p; i++) {
          const o = overrides[i]!;
          if (o.moves.has(frame)) p = composeIso3(correction(o, i), p);
        }
        return p;
      };
      const cache = new Map<number, Iso3>();
      const correction = (o: PoseOverride, i: number): Iso3 => {
        let g = cache.get(i);
        if (g) return g;
        const baked = at(o.parent, i) ?? { rotation: [0, 0, 0, 1], translation: [0, 0, 0] };
        const body = bodies.get(o.parent);
        const parent = body ? composeIso3(baked, body) : baked;
        g = composeIso3(composeIso3(parent, o.local), invertIso3(parent));
        cache.set(i, g);
        return g;
      };
      return at(name, overrides.length);
    },
  };
}

/**
 * `source`'s poses with every camera that has optics at its body instead of its projection
 * frame: what an editor shows and moves, since the document places cameras by their bodies.
 */
export function bodyPoses(source: PoseSource, bodies: Bodies): PoseSource {
  if (bodies.size === 0) return source;
  return {
    pose: (name: string, k?: number) => {
      const p = source.pose(name, k);
      const body = bodies.get(name);
      return p && body ? composeIso3(p, body) : p;
    },
  };
}

/** Per runtime: the overrides its frames show, kept through every `apply`. */
const layers = new WeakMap<FrameTreeRuntime, (overrides: readonly PoseOverride[], bodies: Bodies) => void>();

/**
 * Show `overrides` on `runtime`'s frame objects, and keep them through every later
 * `runtime.apply(k)` — the viewport's and every sensor image's, which call it on their own
 * schedule. Frames no longer overridden go back to their baked pose.
 */
export function showOverrides(
  runtime: FrameTreeRuntime,
  overrides: readonly PoseOverride[],
  bodies: Bodies = NO_BODIES,
): void {
  let set = layers.get(runtime);
  if (!set) {
    let current: readonly PoseOverride[] = [];
    let currentBodies: Bodies = NO_BODIES;
    let written = new Set<string>();
    const write = (index: number, restore: ReadonlySet<string>) => {
      const source = posed(runtime, current, currentBodies);
      const now = new Set<string>();
      for (const o of current) for (const f of o.moves) now.add(f);
      for (const f of new Set([...restore, ...now])) {
        const obj = runtime.frame(f);
        const p = source.pose(f, index);
        if (!obj || !p) continue;
        matrixFromIso3(p, obj.matrix);
        obj.matrixWorldNeedsUpdate = true;
      }
      written = now;
    };
    const apply = runtime.apply.bind(runtime);
    runtime.apply = (k: number): number => {
      const before = runtime.current;
      const index = apply(k);
      // A new sample rewrote every frame from the bake: write the overrides again.
      if (index !== before && written.size > 0) write(index, new Set());
      return index;
    };
    set = (overrides, b) => {
      const stale = written;
      current = overrides;
      currentBodies = b;
      write(runtime.current, stale);
    };
    layers.set(runtime, set);
  }
  set(overrides, bodies);
}
