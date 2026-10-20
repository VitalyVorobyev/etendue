import { describe, expect, it } from "vitest";

import { angleDiff, maxJointDiff, rotate, rpy } from "./geometry";

const h = Math.SQRT1_2;

describe("rotate", () => {
  it("turns x into y for 90° about z", () => {
    rotate([0, 0, h, h], [1, 0, 0]).forEach((v, i) => expect(v).toBeCloseTo([0, 1, 0][i]!, 12));
  });
});

describe("rpy", () => {
  it("reads single-axis rotations", () => {
    expect(rpy([h, 0, 0, h])[0]).toBeCloseTo(Math.PI / 2, 12);
    expect(rpy([0, h, 0, h])[1]).toBeCloseTo(Math.PI / 2, 12);
    expect(rpy([0, 0, h, h])[2]).toBeCloseTo(Math.PI / 2, 12);
  });

  it("composes as Rz · Ry · Rx", () => {
    // Rz(30°) · Rx(20°): rotate by roll first, then yaw.
    const [r, y] = [(20 * Math.PI) / 180, (30 * Math.PI) / 180];
    const qx = [Math.sin(r / 2), 0, 0, Math.cos(r / 2)] as const;
    const qz = [0, 0, Math.sin(y / 2), Math.cos(y / 2)] as const;
    // q = qz ⊗ qx
    const q = [
      qz[3] * qx[0] + qz[0] * qx[3] + qz[1] * qx[2] - qz[2] * qx[1],
      qz[3] * qx[1] - qz[0] * qx[2] + qz[1] * qx[3] + qz[2] * qx[0],
      qz[3] * qx[2] + qz[0] * qx[1] - qz[1] * qx[0] + qz[2] * qx[3],
      qz[3] * qx[3] - qz[0] * qx[0] - qz[1] * qx[1] - qz[2] * qx[2],
    ] as const;
    const [roll, pitch, yaw] = rpy(q);
    expect(roll).toBeCloseTo(r, 12);
    expect(pitch).toBeCloseTo(0, 12);
    expect(yaw).toBeCloseTo(y, 12);
  });
});

describe("angleDiff", () => {
  it("wraps a full turn to zero", () => {
    expect(angleDiff(2 * Math.PI + 0.1, 0)).toBeCloseTo(0.1, 12);
    expect(angleDiff(-0.1, 2 * Math.PI)).toBeCloseTo(-0.1, 12);
  });

  it("measures configurations", () => {
    expect(maxJointDiff([0, 1, 2], [2 * Math.PI, 1, 2.5])).toBeCloseTo(0.5, 12);
  });
});
