/**
 * Frame and pose conventions (etendue PLAN §3), in exactly one place.
 *
 * - World: right-handed, **+Z up**, metres.
 * - Camera frames are calibration-rs/OpenCV: +Z forward, +X right, +Y down. A three.js
 *   (OpenGL) camera looks down −Z with +Y up; the two differ by a fixed rotation of π about
 *   X ({@link CV_TO_GL}).
 * - Transforms are named `a_se3_b` (maps b coordinates into a). On the wire an SE(3) is
 *   `{ rotation: [qx, qy, qz, qw], translation: [tx, ty, tz] }` — nalgebra `Isometry3` serde,
 *   scalar last, the same order as three's `Quaternion(x, y, z, w)`.
 *
 * @packageDocumentation
 */

import { Euler, Matrix4, Quaternion, Vector3 } from "three";

/** An SE(3) as it is read from JSON: `rotation` is `[qx, qy, qz, qw]`, `translation` `[tx, ty, tz]`. */
export interface Iso3Wire {
  /** The unit quaternion `[qx, qy, qz, qw]`, scalar last. */
  readonly rotation: ArrayLike<number>;
  /** The translation `[tx, ty, tz]`, in metres. */
  readonly translation: ArrayLike<number>;
}

/** An SE(3) in wire order, as this package returns it. */
export interface Iso3 {
  /** The unit quaternion `[qx, qy, qz, qw]`, scalar last. */
  rotation: [number, number, number, number];
  /** The translation `[tx, ty, tz]`, in metres. */
  translation: [number, number, number];
}

/** The identity transform. */
export const IDENTITY_ISO3: Readonly<Iso3> = Object.freeze<Iso3>({
  rotation: [0, 0, 0, 1],
  translation: [0, 0, 0],
});

/** The world's up axis. Set it as `camera.up` for orbit controls in a Z-up scene. */
export const Z_UP: Readonly<Vector3> = Object.freeze(new Vector3(0, 0, 1));

/**
 * `cv_se3_gl`: the rotation of π about X that maps OpenGL camera axes (−Z forward, +Y up)
 * onto CV camera axes (+Z forward, +Y down). A three.js camera placed at
 * `world_se3_cv · CV_TO_GL` sees what the CV camera sees.
 */
export const CV_TO_GL: Readonly<Matrix4> = Object.freeze(new Matrix4().makeRotationX(Math.PI));

const q = new Quaternion();
const t = new Vector3();
const ONE = new Vector3(1, 1, 1);

/** The 4×4 matrix of `iso` (written into `target` if given). */
export function matrixFromIso3(iso: Iso3Wire, target = new Matrix4()): Matrix4 {
  const r = iso.rotation;
  const p = iso.translation;
  q.set(r[0]!, r[1]!, r[2]!, r[3]!);
  t.set(p[0]!, p[1]!, p[2]);
  return target.compose(t, q, ONE);
}

/** The SE(3) of a rigid 4×4 matrix (any scale is dropped). */
export function iso3FromMatrix(m: Matrix4): Iso3 {
  const pos = new Vector3();
  const rot = new Quaternion();
  m.decompose(pos, rot, new Vector3());
  return { rotation: [rot.x, rot.y, rot.z, rot.w], translation: [pos.x, pos.y, pos.z] };
}

/** `a · b`: with `a = x_se3_y` and `b = y_se3_z`, returns `x_se3_z`. */
export function composeIso3(a: Iso3Wire, b: Iso3Wire): Iso3 {
  const qa = new Quaternion(a.rotation[0], a.rotation[1], a.rotation[2], a.rotation[3]);
  const qb = new Quaternion(b.rotation[0], b.rotation[1], b.rotation[2], b.rotation[3]);
  const tb = new Vector3(b.translation[0], b.translation[1], b.translation[2]).applyQuaternion(qa);
  const r = qa.multiply(qb);
  return {
    rotation: [r.x, r.y, r.z, r.w],
    translation: [a.translation[0]! + tb.x, a.translation[1]! + tb.y, a.translation[2]! + tb.z],
  };
}

/** The inverse transform: with `iso = a_se3_b`, returns `b_se3_a`. */
export function invertIso3(iso: Iso3Wire): Iso3 {
  const r = new Quaternion(-iso.rotation[0]!, -iso.rotation[1]!, -iso.rotation[2]!, iso.rotation[3]);
  const p = new Vector3(-iso.translation[0]!, -iso.translation[1]!, -iso.translation[2]!).applyQuaternion(r);
  return { rotation: [r.x, r.y, r.z, r.w], translation: [p.x, p.y, p.z] };
}

/**
 * The matrix of a three.js camera that sees what a CV camera at `worldSe3Cv` sees:
 * `world_se3_cv · CV_TO_GL`.
 */
export function glCameraMatrix(worldSe3Cv: Matrix4, target = new Matrix4()): Matrix4 {
  return target.multiplyMatrices(worldSe3Cv, CV_TO_GL);
}

/**
 * Roll, pitch, yaw in radians of a unit quaternion `[qx, qy, qz, qw]`, in the URDF / ROS
 * convention: fixed axes X, Y, Z, i.e. `R = Rz(yaw) · Ry(pitch) · Rx(roll)`.
 */
export function rpyFromQuaternion(rotation: ArrayLike<number>): [number, number, number] {
  const e = new Euler().setFromQuaternion(
    new Quaternion(rotation[0], rotation[1], rotation[2], rotation[3]),
    "ZYX",
  );
  return [e.x, e.y, e.z];
}

/** The unit quaternion `[qx, qy, qz, qw]` of roll, pitch, yaw (see {@link rpyFromQuaternion}). */
export function quaternionFromRpy(rpy: ArrayLike<number>): [number, number, number, number] {
  const r = new Quaternion().setFromEuler(new Euler(rpy[0], rpy[1], rpy[2], "ZYX"));
  return [r.x, r.y, r.z, r.w];
}
