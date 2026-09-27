/**
 * The studio's one door to the kernel: load `@etendue/wasm` once, then turn a loaded scene
 * into a session (validated scene + robot models) and a baked scenario.
 */

import init, { type BakedScenario, EtendueScene, type EtendueError, isEtendueError } from "@etendue/wasm";

import type { LoadedScene } from "../io/load";

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
    const baked = session.bake(loaded.scenario ?? REST);
    return { loaded, session, baked };
  } catch (e) {
    session.free();
    throw e;
  }
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
