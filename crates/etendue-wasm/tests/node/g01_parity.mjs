// Gate G0.1, wasm side (docs/pivot/PLAN.md P0-3): run the wasm build of
// etendue-wasm under Node and dump the inputs and outputs as IEEE-754 bit
// patterns, for `examples/g01_verify.rs` to compare against the native build.
//
//   wasm-pack build crates/etendue-wasm --target nodejs --release
//   node crates/etendue-wasm/tests/node/g01_parity.mjs > target/g01.json
//   cargo run -p etendue-wasm --example g01_verify -- target/g01.json

import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const wasm = require("../../pkg/etendue_wasm.js");

const view = new DataView(new ArrayBuffer(8));
const hex = (x) => {
  view.setFloat64(0, x);
  return view.getBigUint64(0).toString(16).padStart(16, "0");
};

// A deterministic 9×9×9 lattice filling a 0.2 m cube around the default
// target centre (0, 0, 0.30) — mostly in front of the camera, so it exercises
// the full projection chain across and beyond the image.
const xyz = [];
const n = 9;
for (let i = 0; i < n; i++) {
  for (let j = 0; j < n; j++) {
    for (let k = 0; k < n; k++) {
      xyz.push(-0.1 + (0.2 * i) / (n - 1), -0.1 + (0.2 * j) / (n - 1), 0.2 + (0.2 * k) / (n - 1));
    }
  }
}
// The default camera sits at (0.28, -0.55, 0.30) looking toward the target
// centre: its own centre and a point behind it must come back as NaN.
xyz.push(0.28, -0.55, 0.3, 0.56, -1.1, 0.3);

const cameraIndex = 0;
const uv = wasm.project_points(wasm.default_mvp_scene_json(), cameraIndex, new Float64Array(xyz));

process.stdout.write(
  JSON.stringify({
    gate: "G0.1",
    camera_index: cameraIndex,
    xyz: xyz.map(hex),
    uv: Array.from(uv, hex),
  }) + "\n",
);
