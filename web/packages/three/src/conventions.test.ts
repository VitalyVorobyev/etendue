import { Matrix4, Vector3 } from "three";
import { describe, expect, it } from "vitest";

import {
  CV_TO_GL,
  IDENTITY_ISO3,
  composeIso3,
  glCameraMatrix,
  invertIso3,
  iso3FromMatrix,
  matrixFromIso3,
  quaternionFromRpy,
  rpyFromQuaternion,
} from "./conventions";

const s = Math.SQRT1_2;
/** 90° about Z, then 1 m along X. */
const A = { rotation: [0, 0, s, s], translation: [1, 0, 0] } as const;
const B = { rotation: [0.1, -0.2, 0.3, Math.sqrt(1 - 0.14)], translation: [0.3, -0.4, 2] } as const;

function close(a: ArrayLike<number>, b: ArrayLike<number>, tol = 1e-12): void {
  expect(a.length).toBe(b.length);
  for (let i = 0; i < a.length; i++) expect(Math.abs(a[i]! - b[i]!)).toBeLessThan(tol);
}

describe("conventions", () => {
  it("maps the wire order onto three's quaternion order", () => {
    // 90° about Z maps +X to +Y, then translates.
    const p = new Vector3(1, 0, 0).applyMatrix4(matrixFromIso3(A));
    close(p.toArray(), [1, 1, 0]);
  });

  it("round-trips through a matrix", () => {
    const back = iso3FromMatrix(matrixFromIso3(B));
    close(back.rotation, B.rotation);
    close(back.translation, B.translation);
  });

  it("composes like matrices and inverts to identity", () => {
    const ab = matrixFromIso3(composeIso3(A, B));
    close(ab.elements, new Matrix4().multiplyMatrices(matrixFromIso3(A), matrixFromIso3(B)).elements);
    const id = composeIso3(B, invertIso3(B));
    close(id.rotation, IDENTITY_ISO3.rotation);
    close(id.translation, IDENTITY_ISO3.translation);
  });

  it("turns a CV camera into a GL camera by Rx(π)", () => {
    // CV looks down +Z with +Y down; the GL camera must look down its −Z with +Y up.
    const worldSe3Cv = matrixFromIso3(B);
    const gl = glCameraMatrix(worldSe3Cv);
    const cvForward = new Vector3(0, 0, 1).transformDirection(worldSe3Cv);
    const glForward = new Vector3(0, 0, -1).transformDirection(gl);
    close(glForward.toArray(), cvForward.toArray());
    const cvDown = new Vector3(0, 1, 0).transformDirection(worldSe3Cv);
    const glUp = new Vector3(0, 1, 0).transformDirection(gl);
    close(glUp.toArray(), cvDown.clone().negate().toArray());
    // CV_TO_GL is its own inverse.
    close(CV_TO_GL.clone().multiply(CV_TO_GL).elements, new Matrix4().elements);
  });

  it("round-trips roll-pitch-yaw in the URDF convention", () => {
    const rpy = [0.1, -0.4, 2.5];
    close(rpyFromQuaternion(quaternionFromRpy(rpy)), rpy);
    // Pure yaw of 90° is the rotation of A.
    close(quaternionFromRpy([0, 0, Math.PI / 2]), A.rotation);
    // Fixed-axis XYZ: R = Rz(yaw)·Ry(pitch)·Rx(roll).
    const r = matrixFromIso3({ rotation: quaternionFromRpy(rpy), translation: [0, 0, 0] });
    const expected = new Matrix4()
      .makeRotationZ(rpy[2]!)
      .multiply(new Matrix4().makeRotationY(rpy[1]!))
      .multiply(new Matrix4().makeRotationX(rpy[0]!));
    close(r.elements, expected.elements);
  });
});
