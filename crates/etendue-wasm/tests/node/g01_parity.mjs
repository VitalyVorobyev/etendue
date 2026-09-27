// Gate G0.1, wasm side (docs/pivot/PLAN.md P0-3): run the wasm build of
// etendue-wasm under Node and dump the inputs and outputs as IEEE-754 bit
// patterns, for `examples/g01_verify.rs` to compare against the native build.
//
//   node crates/etendue-wasm/scripts/build-npm.mjs
//   node crates/etendue-wasm/tests/node/g01_parity.mjs > target/g01.json
//   cargo run -p etendue-wasm --example g01_verify -- target/g01.json
//
// It runs the published npm package (`pkg/`, the raw wasm-bindgen module) on
// the scene examples/eye_in_hand_ur5e, loaded with its UR5e model as the
// studio does; both of its cameras are probed.

import { readFileSync } from "node:fs";

import * as wasm from "../../pkg/etendue_wasm.js";

wasm.initSync({ module: readFileSync(new URL("../../pkg/etendue_wasm_bg.wasm", import.meta.url)) });
const root = new URL("../../../../", import.meta.url);
const read = (path) => readFileSync(new URL(path, root), "utf8");

const scene = read("examples/eye_in_hand_ur5e/scene.json");
const robots = JSON.stringify([
  {
    id: "ur5e",
    manifest: JSON.parse(read("assets/robots/ur5e/robot.json")),
    urdf: read("assets/robots/ur5e/robot.urdf"),
  },
]);
const session = new wasm.EtendueScene(scene, robots);

const view = new DataView(new ArrayBuffer(8));
const hex = (x) => {
  view.setFloat64(0, x);
  return view.getBigUint64(0).toString(16).padStart(16, "0");
};

// Camera pose: 0.5 m above the world origin, tilted 20° about world X so the
// optical axis is not axis-aligned. Wire order [qx, qy, qz, qw, tx, ty, tz].
const half = (20 * Math.PI) / 180 / 2;
const worldSe3Camera = [Math.sin(half), 0, 0, Math.cos(half), 0, 0, 0.5];

// A deterministic 9×9×9 lattice filling a 0.4 m cube in front of the camera,
// plus the camera centre (must be NaN).
const pose = { q: worldSe3Camera.slice(0, 4), t: worldSe3Camera.slice(4) };
const rotate = ([x, y, z]) => {
  const [qx, qy, qz, qw] = pose.q;
  // v' = v + 2w(q×v) + 2 q×(q×v)
  const cx = qy * z - qz * y, cy = qz * x - qx * z, cz = qx * y - qy * x;
  const dx = qy * cz - qz * cy, dy = qz * cx - qx * cz, dz = qx * cy - qy * cx;
  return [x + 2 * (qw * cx + dx), y + 2 * (qw * cy + dy), z + 2 * (qw * cz + dz)];
};
const xyz = [];
const n = 9;
for (let i = 0; i < n; i++) {
  for (let j = 0; j < n; j++) {
    for (let k = 0; k < n; k++) {
      const local = [-0.2 + (0.4 * i) / (n - 1), -0.2 + (0.4 * j) / (n - 1), 0.2 + (0.4 * k) / (n - 1)];
      const [x, y, z] = rotate(local);
      xyz.push(x + pose.t[0], y + pose.t[1], z + pose.t[2]);
    }
  }
}
xyz.push(...pose.t);

const cameras = ["cam_left", "cam_right"];
const uv = cameras.map((id) =>
  Array.from(session.projectPoints(id, new Float64Array(worldSe3Camera), new Float64Array(xyz)), hex),
);
session.free();

process.stdout.write(
  JSON.stringify({
    gate: "G0.1",
    cameras,
    world_se3_camera: worldSe3Camera.map(hex),
    xyz: xyz.map(hex),
    uv,
  }) + "\n",
);
