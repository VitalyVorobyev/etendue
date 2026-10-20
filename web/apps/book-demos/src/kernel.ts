/**
 * The widgets' door to the kernel: load `@etendue/wasm` once, and the documents shipped beside
 * `demos.js` (the UR5e's description and an example cell).
 */

import init, { EtendueScene, type RobotSource, type ScenarioSpec, type SceneSpec } from "@etendue/wasm";

let ready: Promise<unknown> | undefined;

/** Load the wasm module (once). */
export function kernel(): Promise<unknown> {
  ready ??= init();
  return ready;
}

/** Where this module was loaded from: `demos/`, beside the data. (A variable, so that Vite does not
 * turn the `new URL` below into a build-time glob.) */
const HERE = import.meta.url;

/** A file shipped beside `demos.js` (see vite.config.ts). */
async function data(path: string): Promise<string> {
  const response = await fetch(new URL(`data/${path}`, HERE));
  if (!response.ok) throw new Error(`could not load ${path} (${response.status})`);
  return response.text();
}

let ur5e: Promise<RobotSource> | undefined;

/** The UR5e's description, as scene robot `ur5e`. */
export function ur5eSource(): Promise<RobotSource> {
  ur5e ??= Promise.all([data("ur5e/robot.json"), data("ur5e/robot.urdf")]).then(([manifest, urdf]) => ({
    id: "ur5e",
    manifest: JSON.parse(manifest) as RobotSource["manifest"],
    urdf,
  }));
  return ur5e;
}

/** A UR5e on its own at the world origin. */
export async function robotSession(): Promise<EtendueScene> {
  await kernel();
  const scene: SceneSpec = {
    version: 1,
    robots: [
      {
        id: "ur5e",
        parent: "world",
        parent_se3_self: { rotation: [0, 0, 0, 1], translation: [0, 0, 0] },
        manifest: "ur5e/robot.json",
        initial_q: [0, -1.5708, 1.5708, -1.5708, -1.5708, 0],
      },
    ],
  };
  return new EtendueScene(scene, [await ur5eSource()]);
}

/** The closed-loop example cell (a UR5e carrying a stereo rig over a chessboard). */
export interface Cell {
  session: EtendueScene;
  scene: SceneSpec;
  scenario: ScenarioSpec;
}

let cell: Promise<Cell> | undefined;

export function closedLoopCell(): Promise<Cell> {
  cell ??= (async () => {
    await kernel();
    const [scene, scenario, robot] = await Promise.all([
      data("closed_loop_ur5e/scene.json"),
      data("closed_loop_ur5e/scenario.json"),
      ur5eSource(),
    ]);
    const spec = JSON.parse(scene) as SceneSpec;
    return { session: new EtendueScene(spec, [robot]), scene: spec, scenario: JSON.parse(scenario) as ScenarioSpec };
  })();
  return cell;
}
