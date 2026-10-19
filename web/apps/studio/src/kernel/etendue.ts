/**
 * The studio's one door to the kernel: load `@etendue/wasm` once, then turn a loaded scene
 * into a session (validated scene + robot models) and a baked scenario.
 */

import init, { type BakedScenario, EtendueScene, type EtendueError, isEtendueError } from "@etendue/wasm";

/** A rig layout's cameras and lasers (the kernel's `RigLayout::expand`). Needs `initKernel()`. */
export { expandRig } from "@etendue/wasm";

import type { LoadedScene } from "../io/load";
import { sceneEffect } from "../scene/effect";

let ready: Promise<unknown> | undefined;

/** Load the wasm module (once). */
export function initKernel(): Promise<unknown> {
  ready ??= init();
  return ready;
}

/** A scene the kernel accepted, baked. */
export interface Baked {
  loaded: LoadedScene;
  session: EtendueScene;
  baked: BakedScenario;
}

/** What went wrong, for display. */
export interface KernelFailure {
  title: string;
  message: string;
  issues: { path: string; message: string }[];
}

/** Every bake records joint velocities and accelerations: the Motion workspace charts them. */
const BAKE = { derivatives: true };

/**
 * The name of the performance mark set at every bake: a slider drag must set none (gate G8.1),
 * which tests count with `performance.getEntriesByName(BAKE_MARK)`.
 */
export const BAKE_MARK = "etendue:bake";

function bake(session: EtendueScene, loaded: LoadedScene): BakedScenario {
  performance.mark(BAKE_MARK);
  return session.bake(loaded.scenario ?? REST, BAKE);
}

/** The rest pose: a scenario with no steps bakes to one sample at `initial_q`. */
export const REST: { version: number; dt: number; steps: [] } = { version: 1, dt: 0.01, steps: [] };

/**
 * Validate and bake. The caller owns the returned session and must `free()` it when it is
 * replaced.
 */
export async function bakeScene(loaded: LoadedScene): Promise<Baked> {
  await initKernel();
  const robots = loaded.robots.map(({ id, manifest, urdf }) => ({ id, manifest, urdf }));
  const session = new EtendueScene(loaded.scene, robots);
  try {
    const baked = bake(session, loaded);
    return { loaded, session, baked };
  } catch (e) {
    session.free();
    throw e;
  }
}

/**
 * Bake an edit of `prev`'s scene or scenario. The session is reused when the scene names the
 * same robots (no URDF is parsed again): its scene is replaced in place, and `prev` must not be
 * used afterwards. Otherwise a new session is built and `prev`'s freed. On error nothing
 * changes and `prev` stays valid.
 *
 * An edit that changes no baked pose (intrinsics, a laser's fan, a target's print) keeps
 * `prev.baked`, the same object: the frame tree, and the robot meshes on it, stay as they are.
 */
export function rebake(prev: Baked, loaded: LoadedScene): Baked {
  const sameRobots = loaded.robots === prev.loaded.robots;
  if (!sameRobots) {
    const robots = loaded.robots.map(({ id, manifest, urdf }) => ({ id, manifest, urdf }));
    const session = new EtendueScene(loaded.scene, robots);
    try {
      const baked = bake(session, loaded);
      prev.session.free();
      return { loaded, session, baked };
    } catch (e) {
      session.free();
      throw e;
    }
  }
  if (loaded.scene !== prev.loaded.scene) {
    // An invalid scene throws here and leaves the session as it was.
    prev.session.updateScene(loaded.scene);
    if (loaded.scenario === prev.loaded.scenario && sceneEffect(prev.loaded.scene, loaded.scene).kind === "params") {
      return { loaded, session: prev.session, baked: prev.baked };
    }
    try {
      return { loaded, session: prev.session, baked: bake(prev.session, loaded) };
    } catch (e) {
      // The scene is valid but the scenario does not run in it: put the old scene back.
      prev.session.updateScene(prev.loaded.scene);
      throw e;
    }
  }
  return { loaded, session: prev.session, baked: bake(prev.session, loaded) };
}

/** Describe any error thrown while loading or baking. */
export function describeFailure(error: unknown): KernelFailure {
  if (isEtendueError(error)) {
    const e: EtendueError = error;
    const title =
      e.kind === "invalid"
        ? `The ${e.document ?? "document"} is invalid`
        : e.kind === "parse"
          ? "A document does not parse"
          : e.kind === "kinematics"
            ? "Kinematics failed"
            : "Bad input";
    return { title, message: e.message.split("\n")[0] ?? e.message, issues: e.issues ?? [] };
  }
  return { title: "Loading failed", message: error instanceof Error ? error.message : String(error), issues: [] };
}
