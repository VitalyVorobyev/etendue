// Gate G11.3 (docs/pivot/PLAN.md P11): a rig's coverage on a 64³ grid takes ≤ 100 ms in wasm
// (median of 5), so the Rigs workspace can recompute it as a slider is released.
//
//   node crates/etendue-wasm/scripts/build-npm.mjs
//   node crates/etendue-wasm/tests/node/g11_3_coverage.mjs

import { readFileSync } from "node:fs";

import { initSync, rigCoverage } from "../../pkg/index.js";
import { ringFixture } from "./head_fixture.mjs";

initSync({ module: readFileSync(new URL("../../pkg/etendue_wasm_bg.wasm", import.meta.url)) });
const { layout, box } = ringFixture(new URL("../../../../", import.meta.url));

const median = (f, runs = 5) => {
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

const BUDGET_MS = 100;
const run = (n, criteria) => median(() => rigCoverage(layout, { ...box, n: [n, n, n] }, criteria));
const grid64 = run(64, { max_blur_px: 3 });
const grid32 = run(32, { max_blur_px: 3 });
const count = (a, min) => a.reduce((s, v) => s + (v >= min ? 1 : 0), 0);
const result = {
  gate: "G11.3",
  rig: "3 heads (IMX174 + LM25HC f/4, laser on the body), 80 mm ring, 0.35 m",
  coverage_64_ms: grid64.ms,
  coverage_32_ms: grid32.ms,
  voxels_seen_by_3: count(grid64.out.views, 3),
  voxels_measured: count(grid64.out.lit, 1),
  budget_ms: BUDGET_MS,
  pass: grid64.ms <= BUDGET_MS,
};
process.stdout.write(JSON.stringify(result, null, 2) + "\n");
if (!result.pass) process.exit(1);
