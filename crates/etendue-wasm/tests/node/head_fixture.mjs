// A triangulation head for the wasm probes (G0.1, G10.1): the library's IMX174 + LM25HC camera
// and StingRay laser, the laser 100 mm off the camera axis aimed at it 0.35 m out, the laser line
// along camera x, with a 120 mm deep, 40 mm wide window around the aim point.

import { readFileSync } from "node:fs";

export function headFixture(root) {
  const read = (path) => JSON.parse(readFileSync(new URL(path, root), "utf8"));
  const camera = read("examples/library/cameras/imx174_lm25hc.json").camera;
  const laser = read("examples/library/lasers/stingray_660.json").laser;
  const origin = [0, -0.1, 0.05];
  const aim = [0, 0, 0.35];
  const d = aim.map((a, i) => a - origin[i]);
  const n = Math.hypot(...d);
  const z = d.map((x) => x / n); // central ray
  const y = [1, 0, 0]; // the line
  const x = [y[1] * z[2] - y[2] * z[1], y[2] * z[0] - y[0] * z[2], y[0] * z[1] - y[1] * z[0]]; // normal
  // Rotation matrix [x y z] (columns) to a quaternion.
  const m = [
    [x[0], y[0], z[0]],
    [x[1], y[1], z[1]],
    [x[2], y[2], z[2]],
  ];
  const qw = Math.sqrt(1 + m[0][0] + m[1][1] + m[2][2]) / 2;
  const rotation = [(m[2][1] - m[1][2]) / (4 * qw), (m[0][2] - m[2][0]) / (4 * qw), (m[1][0] - m[0][1]) / (4 * qw), qw];
  return {
    camera,
    laser,
    fan_length_m: 1,
    camera_se3_laser: { rotation, translation: origin },
    window: { near_m: n - 0.06, far_m: n + 0.06, width_m: 0.04 },
  };
}
