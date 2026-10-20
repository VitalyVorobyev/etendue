/**
 * Small rotation helpers for the robot widgets: quaternions in the wire order `[x, y, z, w]`.
 */

export type Quat = readonly [number, number, number, number];
export type V3 = [number, number, number];

/** Rotate `v` by the unit quaternion `q`. */
export function rotate(q: Quat, v: readonly [number, number, number]): V3 {
  const [x, y, z, w] = q;
  const [vx, vy, vz] = v;
  const tx = 2 * (y * vz - z * vy);
  const ty = 2 * (z * vx - x * vz);
  const tz = 2 * (x * vy - y * vx);
  return [vx + w * tx + (y * tz - z * ty), vy + w * ty + (z * tx - x * tz), vz + w * tz + (x * ty - y * tx)];
}

/**
 * Roll, pitch and yaw (rad) of `q`: the rotation is `Rz(yaw) · Ry(pitch) · Rx(roll)`, as robot
 * controllers usually report a tool's orientation.
 */
export function rpy(q: Quat): V3 {
  const [x, y, z, w] = q;
  const roll = Math.atan2(2 * (w * x + y * z), 1 - 2 * (x * x + y * y));
  const s = 2 * (w * y - z * x);
  const pitch = Math.abs(s) >= 1 ? (Math.sign(s) * Math.PI) / 2 : Math.asin(s);
  const yaw = Math.atan2(2 * (w * z + x * y), 1 - 2 * (y * y + z * z));
  return [roll, pitch, yaw];
}

/** `a − b` wrapped into `(−π, π]`: how far apart two joint angles are as positions of the arm. */
export function angleDiff(a: number, b: number): number {
  let d = (a - b) % (2 * Math.PI);
  if (d > Math.PI) d -= 2 * Math.PI;
  if (d <= -Math.PI) d += 2 * Math.PI;
  return d;
}

/** The largest wrapped joint difference between two configurations. */
export function maxJointDiff(a: readonly number[], b: readonly number[]): number {
  let m = 0;
  for (let i = 0; i < a.length; i++) m = Math.max(m, Math.abs(angleDiff(a[i]!, b[i]!)));
  return m;
}
