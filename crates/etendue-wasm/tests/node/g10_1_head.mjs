// Gate G10.1, kernel side (docs/pivot/PLAN.md P10): a head's working-volume map on a 64 × 64
// grid takes ≤ 30 ms in wasm (median of 7), so the Optics workspace can redraw it as a slider
// moves; the section and the optimiser are timed too.
//
//   node crates/etendue-wasm/scripts/build-npm.mjs
//   node crates/etendue-wasm/tests/node/g10_1_head.mjs

import { readFileSync } from "node:fs";

import { headMap, headOptimise, headScheimpflug, headSection, initSync } from "../../pkg/index.js";
import { headFixture } from "./head_fixture.mjs";

initSync({ module: readFileSync(new URL("../../pkg/etendue_wasm_bg.wasm", import.meta.url)) });
const head = headFixture(new URL("../../../../", import.meta.url));
const design = JSON.stringify(head);

const median = (f, runs = 7) => {
  const times = [];
  let out;
  for (let i = 0; i < runs; i++) {
    const t0 = performance.now();
    out = f();
    times.push(performance.now() - t0);
  }
  times.sort((a, b) => a - b);
  return { ms: times[Math.floor(runs / 2)], out };
};

const BUDGET_MS = 30;
const map = median(() => headMap(design, null, 64, 64));
const section = median(() => headSection(design, { probes: [0.25, 0.32, 0.4], blur_limit_px: 2 }));
const hinge = headScheimpflug(design);
const optimise = median(() => headOptimise(design, { grid: [9, 9], f_number_range: [2, 16], tilt_free: false }), 3);
const visible = map.out.visible.reduce((a, b) => a + b, 0);
const result = {
  gate: "G10.1",
  map_64x64_ms: map.ms,
  map_visible: `${visible}/${64 * 64}`,
  section_ms: section.ms,
  optimise_square_sensor_ms: optimise.ms,
  optimise_iterations: optimise.out.iterations,
  scheimpflug_tilt_deg: hinge.tilt.map((t) => (t * 180) / Math.PI),
  budget_ms: BUDGET_MS,
  pass: map.ms <= BUDGET_MS,
};
console.log(JSON.stringify(result, null, 2));
process.exit(result.pass ? 0 : 1);
