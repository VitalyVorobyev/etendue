// P8 baseline (docs/pivot/PLAN.md P8): what one inspector edit costs the studio today.
// Every edit runs `updateScene` + `bake` (studio `rebake`), then the studio parses nothing
// more: the baked scenario crosses as JSON and becomes a new FrameTreeRuntime.
//
//   node crates/etendue-wasm/scripts/build-npm.mjs
//   node crates/etendue-wasm/tests/node/p8_rebake_baseline.mjs
//
// Runs the npm package on examples/rig3_laser_ur5e with its UR5e model, as the studio does.

import { readFileSync } from "node:fs";

import { EtendueScene, initSync } from "../../pkg/index.js";

initSync({ module: readFileSync(new URL("../../pkg/etendue_wasm_bg.wasm", import.meta.url)) });
const root = new URL("../../../../", import.meta.url);
const read = (path) => readFileSync(new URL(path, root), "utf8");

const scene = JSON.parse(read("examples/rig3_laser_ur5e/scene.json"));
const scenario = JSON.parse(read("examples/rig3_laser_ur5e/scenario.json"));
const session = new EtendueScene(scene, [
  {
    id: "ur5e",
    manifest: JSON.parse(read("assets/robots/ur5e/robot.json")),
    urdf: read("assets/robots/ur5e/robot.urdf"),
  },
]);

const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];
const RUNS = 9;
const update = [];
const bake = [];
let baked;
for (let i = 0; i < RUNS; i++) {
  const moved = structuredClone(scene);
  moved.targets[0].parent_se3_self.translation[0] += 1e-3 * (i + 1);
  let t0 = performance.now();
  session.updateScene(moved);
  update.push(performance.now() - t0);
  t0 = performance.now();
  baked = session.bake(scenario, { derivatives: true });
  bake.push(performance.now() - t0);
}
const result = {
  example: "rig3_laser_ur5e",
  samples: baked.samples.length,
  frames: baked.frames.length,
  update_scene_ms: median(update),
  bake_ms: median(bake),
  edit_ms: median(update) + median(bake),
};
console.log(JSON.stringify(result, null, 2));
