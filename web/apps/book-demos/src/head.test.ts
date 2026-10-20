import { describe, expect, it } from "vitest";

import { DEFAULT_HEAD, aimDistance, laserPose, quatFromBasis, windowRange } from "./head";

/** Rotate `v` by the unit quaternion `q = [x, y, z, w]`. */
function rotate(q: [number, number, number, number], v: [number, number, number]): number[] {
  const [x, y, z, w] = q;
  const [vx, vy, vz] = v;
  const tx = 2 * (y * vz - z * vy);
  const ty = 2 * (z * vx - x * vz);
  const tz = 2 * (x * vy - y * vx);
  return [vx + w * tx + (y * tz - z * ty), vy + w * ty + (z * tx - x * tz), vz + w * tz + (x * ty - y * tx)];
}

describe("quatFromBasis", () => {
  it("is the identity for the standard basis", () => {
    const q = quatFromBasis([1, 0, 0], [0, 1, 0], [0, 0, 1]);
    expect(q[3]).toBeCloseTo(1, 12);
  });

  it("maps the axes onto the columns", () => {
    // 90° about z: x → y, y → −x.
    const q = quatFromBasis([0, 1, 0], [-1, 0, 0], [0, 0, 1]);
    rotate(q, [1, 0, 0]).forEach((v, i) => expect(v).toBeCloseTo([0, 1, 0][i]!, 12));
    rotate(q, [0, 1, 0]).forEach((v, i) => expect(v).toBeCloseTo([-1, 0, 0][i]!, 12));
  });
});

describe("laserPose", () => {
  it("aims the laser's central ray (+z) at the aim point on the camera axis", () => {
    const h = DEFAULT_HEAD;
    const pose = laserPose(h);
    const dir = rotate(pose.rotation, [0, 0, 1]);
    const d = aimDistance(h);
    const hit = pose.translation.map((o, i) => o + dir[i]! * d);
    expect(hit[0]).toBeCloseTo(0, 12);
    expect(hit[1]).toBeCloseTo(0, 12);
    expect(hit[2]).toBeCloseTo(h.aim_m, 12);
  });

  it("puts the laser line along camera x", () => {
    const line = rotate(laserPose(DEFAULT_HEAD).rotation, [0, 1, 0]);
    expect(line[0]).toBeCloseTo(1, 12);
  });
});

describe("windowRange", () => {
  it("centres the window on the aim point", () => {
    const [near, far] = windowRange(DEFAULT_HEAD);
    expect((near + far) / 2).toBeCloseTo(aimDistance(DEFAULT_HEAD), 12);
    expect(far - near).toBeCloseTo(DEFAULT_HEAD.depth_m, 12);
  });
});
