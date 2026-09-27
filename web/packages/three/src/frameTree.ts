/**
 * Apply a baked scenario (etendue ADR 0003) to a three.js object graph.
 *
 * Kinematics ran once, upstream: every sample of a baked scenario already holds
 * `world_se3_frame` for every frame of the scene. This runtime only copies those poses into
 * matrices — it never computes a pose from joint values.
 */

import { Group, type Object3D } from "three";

import { type Iso3, type Iso3Wire, matrixFromIso3 } from "./conventions";

/** One sample of a baked scenario — the fields this runtime reads. */
export interface BakedSampleLike {
  /** Time in seconds. */
  readonly t: number;
  /** `world_se3_frame` of every frame, index-aligned with {@link BakedScenarioLike.frames}. */
  readonly world_se3_frame: readonly Iso3Wire[];
  /** Set on capture samples. */
  readonly capture?: { readonly id: string } | null | undefined;
}

/** A baked scenario (etendue `BakedScenario`) — the fields this runtime reads. */
export interface BakedScenarioLike {
  /** Sample period in seconds. */
  readonly dt: number;
  /** Frame names, topological order, `"world"` first. */
  readonly frames: readonly string[];
  /** Samples at `t = k · dt`. */
  readonly samples: readonly BakedSampleLike[];
}

/** A capture event of a baked scenario. */
export interface CaptureMarker {
  /** Sample index. */
  index: number;
  /** Capture id. */
  id: string;
}

/** The name of the world frame. */
export const WORLD = "world";

/**
 * One `Object3D` per frame of a baked scenario, posed at a chosen sample.
 *
 * {@link FrameTreeRuntime.root} is the world frame; every other frame is a direct child of it
 * with `matrixAutoUpdate = false` and `matrix = world_se3_frame`. Attach geometry to a frame
 * with `runtime.frame(name).add(object)` — the object is then expressed in that frame
 * (for a camera frame: CV axes, +Z forward).
 *
 * ```ts
 * const runtime = new FrameTreeRuntime(baked);
 * scene.add(runtime.root);
 * runtime.apply(k); // each animation frame
 * ```
 */
export class FrameTreeRuntime {
  /** The world frame; add it to a scene. */
  readonly root: Group;
  /** Sample period in seconds. */
  readonly dt: number;
  /** Number of samples. */
  readonly sampleCount: number;
  /** Frame names, `"world"` first. */
  readonly frameNames: readonly string[];
  /** Capture samples, in time order. */
  readonly captures: readonly CaptureMarker[];

  readonly #frames = new Map<string, Object3D>();
  readonly #objects: Object3D[] = [];
  /** 7 numbers per frame per sample: qx, qy, qz, qw, tx, ty, tz. */
  readonly #poses: Float64Array;
  readonly #wire = { rotation: new Float64Array(4), translation: new Float64Array(3) };
  #current = -1;

  /** @throws `Error` if a sample's pose list is not aligned with `frames`, or `frames[0]` is not `"world"`. */
  constructor(baked: BakedScenarioLike) {
    if (baked.frames[0] !== WORLD) throw new Error(`the first frame must be "${WORLD}"`);
    if (baked.samples.length === 0) throw new Error("a baked scenario needs at least one sample");
    const n = baked.frames.length;
    this.dt = baked.dt;
    this.sampleCount = baked.samples.length;
    this.frameNames = [...baked.frames];

    this.root = new Group();
    this.root.name = WORLD;
    this.#frames.set(WORLD, this.root);
    for (const name of baked.frames.slice(1)) {
      const g = new Group();
      g.name = name;
      g.matrixAutoUpdate = false;
      this.root.add(g);
      this.#frames.set(name, g);
      this.#objects.push(g);
    }

    this.#poses = new Float64Array(this.sampleCount * n * 7);
    const captures: CaptureMarker[] = [];
    baked.samples.forEach((s, k) => {
      if (s.world_se3_frame.length !== n) {
        throw new Error(`sample ${k}: ${s.world_se3_frame.length} poses for ${n} frames`);
      }
      s.world_se3_frame.forEach((p, f) => {
        const o = (k * n + f) * 7;
        for (let i = 0; i < 4; i++) this.#poses[o + i] = p.rotation[i]!;
        for (let i = 0; i < 3; i++) this.#poses[o + 4 + i] = p.translation[i]!;
      });
      if (s.capture) captures.push({ index: k, id: s.capture.id });
    });
    this.captures = captures;
    this.apply(0);
  }

  /** The object of frame `name` (`"world"` is {@link FrameTreeRuntime.root}). */
  frame(name: string): Object3D | undefined {
    return this.#frames.get(name);
  }

  /** The sample currently applied. */
  get current(): number {
    return this.#current;
  }

  /**
   * Pose every frame at sample `k` (rounded and clamped to the valid range). Cheap when `k` is
   * already applied. Returns the sample index applied.
   */
  apply(k: number): number {
    const index = Math.min(this.sampleCount - 1, Math.max(0, Math.round(k)));
    if (index === this.#current) return index;
    const n = this.frameNames.length;
    this.#objects.forEach((obj, i) => {
      this.#read(index, i + 1, n);
      matrixFromIso3(this.#wire, obj.matrix);
      obj.matrixWorldNeedsUpdate = true;
    });
    this.#current = index;
    return index;
  }

  /** `world_se3_frame` of frame `name` at sample `k` (default: the current one). */
  pose(name: string, k = this.#current): Iso3 | undefined {
    const f = this.frameNames.indexOf(name);
    if (f < 0 || k < 0 || k >= this.sampleCount) return undefined;
    this.#read(k, f, this.frameNames.length);
    const { rotation: r, translation: t } = this.#wire;
    return { rotation: [r[0]!, r[1]!, r[2]!, r[3]!], translation: [t[0]!, t[1]!, t[2]!] };
  }

  #read(k: number, f: number, n: number): void {
    const o = (k * n + f) * 7;
    this.#wire.rotation.set(this.#poses.subarray(o, o + 4));
    this.#wire.translation.set(this.#poses.subarray(o + 4, o + 7));
  }
}
