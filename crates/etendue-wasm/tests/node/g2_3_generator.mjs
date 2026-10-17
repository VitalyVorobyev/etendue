// Gate G2.3 (docs/pivot/PLAN.md P7-6): the sphere pose generator in wasm,
// 400 candidates seen by three cameras, must take ≤ 100 ms (median of 7), so
// the studio can recompute it as its form changes.
//
//   node crates/etendue-wasm/scripts/build-npm.mjs
//   node crates/etendue-wasm/tests/node/g2_3_generator.mjs
//
// Runs the npm package (`pkg/`, the typed layer) on examples/rig3_laser_ur5e,
// loaded with its UR5e model as the studio does.

import { readFileSync } from "node:fs";

import { EtendueScene, initSync } from "../../pkg/index.js";

initSync({ module: readFileSync(new URL("../../pkg/etendue_wasm_bg.wasm", import.meta.url)) });
const root = new URL("../../../../", import.meta.url);
const read = (path) => readFileSync(new URL(path, root), "utf8");

const session = new EtendueScene(JSON.parse(read("examples/rig3_laser_ur5e/scene.json")), [
  {
    id: "ur5e",
    manifest: JSON.parse(read("assets/robots/ur5e/robot.json")),
    urdf: read("assets/robots/ur5e/robot.urdf"),
  },
]);
const spec = {
  type: "sphere",
  robot: "ur5e",
  look: "rig",
  target: "board",
  radii_m: [0.4, 0.45, 0.5, 0.55, 0.6],
  theta: { min_rad: 0.1, max_rad: 0.6, steps: 5 },
  phi: { min_rad: 0, max_rad: 2 * Math.PI, steps: 16 },
};
const BUDGET_MS = 100;
const times = [];
let generated;
for (let i = 0; i < 7; i++) {
  const t0 = performance.now();
  generated = session.generatePoses(spec);
  times.push(performance.now() - t0);
}
times.sort((a, b) => a - b);
const kept = generated.candidates.filter((c) => c.status.status === "ok").length;
const median = times[3];
console.log(
  `${generated.candidates.length} candidates × 3 cameras, ${kept} kept: median ${median.toFixed(1)} ms ` +
    `(min ${times[0].toFixed(1)}, max ${times[6].toFixed(1)}) ≤ ${BUDGET_MS} ms: ${median <= BUDGET_MS ? "PASS" : "FAIL"}`,
);
process.exitCode = median <= BUDGET_MS ? 0 : 1;
