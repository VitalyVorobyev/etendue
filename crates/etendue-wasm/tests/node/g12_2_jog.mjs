// Gate G12.2 (docs/pivot/PLAN.md P12): dragging the UR5e's TCP solves IK from the last answer
// (`restarts: 0`) in p95 ≤ 2 ms in wasm, and ≥ 99 % of the 1000 steps of a drag path converge
// with a residual ≤ 1e-6 m and 1e-6 rad, without a joint jumping.
//
// The path: 1000 steps of a circle of 150 mm radius in the base's x–y plane, from a working pose
// 0.58 m out toward the base (it stays inside the arm's reach), the tool also turning ±20° about
// its own z and ±10° about x, as a hand drags a gizmo.
//
//   node crates/etendue-wasm/scripts/build-npm.mjs
//   node crates/etendue-wasm/tests/node/g12_2_jog.mjs

import { readFileSync } from "node:fs";

import { EtendueScene, initSync } from "../../pkg/index.js";

initSync({ module: readFileSync(new URL("../../pkg/etendue_wasm_bg.wasm", import.meta.url)) });
const root = new URL("../../../../", import.meta.url);
const read = (path) => readFileSync(new URL(path, root), "utf8");
const session = new EtendueScene(read("examples/eye_in_hand_ur5e/scene.json"), [
  {
    id: "ur5e",
    manifest: JSON.parse(read("assets/robots/ur5e/robot.json")),
    urdf: read("assets/robots/ur5e/robot.urdf"),
  },
]);

// Quaternion helpers, [x, y, z, w].
const mul = ([ax, ay, az, aw], [bx, by, bz, bw]) => [
  aw * bx + ax * bw + ay * bz - az * by,
  aw * by - ax * bz + ay * bw + az * bx,
  aw * bz + ax * by - ay * bx + az * bw,
  aw * bw - ax * bx - ay * by - az * bz,
];
const about = (axis, angle) => {
  const s = Math.sin(angle / 2);
  return [axis[0] * s, axis[1] * s, axis[2] * s, Math.cos(angle / 2)];
};

const q0 = [0.2, -1.4, 1.6, -1.8, -1.57, 0.3];
const start = session.robotTcp("ur5e", q0).base_se3_tcp;
const STEPS = 1000;
const RADIUS = 0.15;
let q = q0;
const times = [];
let converged = 0;
let worstT = 0;
let worstR = 0;
let largestJump = 0;
let minManipulability = Infinity;
for (let i = 1; i <= STEPS; i++) {
  const a = (2 * Math.PI * i) / STEPS;
  const target = {
    translation: [
      start.translation[0] + RADIUS * (1 - Math.cos(a)),
      start.translation[1] + RADIUS * Math.sin(a),
      start.translation[2],
    ],
    rotation: mul(mul(start.rotation, about([0, 0, 1], 0.35 * Math.sin(a))), about([1, 0, 0], 0.17 * Math.sin(2 * a))),
  };
  const t0 = performance.now();
  const answer = session.robotIk("ur5e", target, q, { restarts: 0 });
  times.push(performance.now() - t0);
  if (answer.converged && answer.translation_error <= 1e-6 && answer.rotation_error <= 1e-6) {
    converged++;
    worstT = Math.max(worstT, answer.translation_error);
    worstR = Math.max(worstR, answer.rotation_error);
    largestJump = Math.max(largestJump, ...answer.q.map((v, j) => Math.abs(v - q[j])));
    minManipulability = Math.min(minManipulability, answer.manipulability);
    q = answer.q;
  }
}
times.sort((x, y) => x - y);
const p = (f) => times[Math.min(times.length - 1, Math.floor(f * times.length))];
const result = {
  gate: "G12.2",
  robot: "ur5e",
  steps: STEPS,
  ik_ms: { p50: p(0.5), p95: p(0.95), max: times[times.length - 1] },
  success: converged / STEPS,
  worst_residual: { m: worstT, rad: worstR },
  largest_joint_step_rad: largestJump,
  min_manipulability: minManipulability,
  budget: { p95_ms: 2, success: 0.99, residual: 1e-6 },
};
result.pass = result.ik_ms.p95 <= 2 && result.success >= 0.99 && largestJump < 0.1;
process.stdout.write(JSON.stringify(result, null, 2) + "\n");
session.free();
if (!result.pass) process.exit(1);
