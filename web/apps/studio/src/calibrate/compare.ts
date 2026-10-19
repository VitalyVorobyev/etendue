/**
 * The calibration's estimate beside the truth (P13): the report's offsets place each estimated
 * camera, the target and each laser plane relative to its true frame. A calibration's errors are
 * far below what a 3-D view shows, so they are drawn exaggerated: every offset's rotation angle
 * and translation, and every plane's tilt and shift from the truth, times a factor.
 */

import type { BakedScenario, Iso3Schema, Observations, ObservedView, SceneSpec } from "@etendue/wasm";
import { composeIso3, invertIso3 } from "@vitavision/three";

import type { CalibrationReport, Plane } from "../kernel/native";
import type { EstimateDrawing } from "../scene/Estimates";
import { targetOutline } from "../scene/targets";

type Vec3 = [number, number, number];

const dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a: Vec3, b: Vec3): Vec3 => [
  a[1] * b[2] - a[2] * b[1],
  a[2] * b[0] - a[0] * b[2],
  a[0] * b[1] - a[1] * b[0],
];
const norm = (a: Vec3) => Math.hypot(a[0], a[1], a[2]);

/** `v` turned by `angle` about the unit `axis` (Rodrigues). */
function turn(v: Vec3, axis: Vec3, angle: number): Vec3 {
  const [c, s] = [Math.cos(angle), Math.sin(angle)];
  const k = cross(axis, v);
  const d = dot(axis, v) * (1 - c);
  return [v[0] * c + k[0] * s + axis[0] * d, v[1] * c + k[1] * s + axis[1] * d, v[2] * c + k[2] * s + axis[2] * d];
}

/** `offset` with its rotation angle and its translation times `scale` (1 gives it back). */
export function exaggerate(offset: Iso3Schema, scale: number): Iso3Schema {
  let [x, y, z, w] = offset.rotation;
  if (w < 0) [x, y, z, w] = [-x, -y, -z, -w];
  const s = Math.hypot(x, y, z);
  const half = Math.atan2(s, w) * scale;
  const k = s > 0 ? Math.sin(half) / s : 0;
  const t = offset.translation;
  return {
    rotation: [x * k, y * k, z * k, Math.cos(half)],
    translation: [t[0] * scale, t[1] * scale, t[2] * scale],
  };
}

/**
 * The estimated plane exaggerated from the true one by `scale`: its normal turned away from the
 * truth's by `scale` times their angle, its distance moved by `scale` times their difference.
 * Both have unit normals pointing the same way (the report's convention).
 */
export function exaggeratePlane(truth: Plane, estimate: Plane, scale: number): Plane {
  const axis = cross(truth.normal, estimate.normal);
  const sin = norm(axis);
  const angle = Math.atan2(sin, dot(truth.normal, estimate.normal));
  const normal =
    sin > 0 ? turn(truth.normal, [axis[0] / sin, axis[1] / sin, axis[2] / sin], scale * angle) : truth.normal;
  return { normal, distance: truth.distance + scale * (estimate.distance - truth.distance) };
}

/** Flat `[x, y, z, …]` points moved onto `plane` along its normal. */
export function ontoPlane(points: ArrayLike<number>, plane: Plane): Float32Array {
  const n = plane.normal;
  const out = new Float32Array(points.length);
  for (let i = 0; i + 2 < points.length; i += 3) {
    const p: Vec3 = [points[i]!, points[i + 1]!, points[i + 2]!];
    const d = dot(n, p) + plane.distance;
    out.set([p[0] - d * n[0], p[1] - d * n[1], p[2] - d * n[2]], i);
  }
  return out;
}

/**
 * A laser's fan in camera coordinates: its apex, then its arc, flat `[x, y, z, …]`. The fan lies
 * in the laser's `x = 0` plane, opening about +Z (as `LaserFan` draws it); `cameraSe3Laser`
 * places it.
 */
export function fanInCamera(
  cameraSe3Laser: Iso3Schema,
  halfAngle: number,
  length: number,
  segments = 24,
): Float32Array {
  const out = new Float32Array(3 * (segments + 2));
  const place = (p: Vec3, at: number) => {
    const moved = composeIso3(cameraSe3Laser, { rotation: [0, 0, 0, 1], translation: p }).translation;
    out.set(moved, 3 * at);
  };
  place([0, 0, 0], 0);
  for (let i = 0; i <= segments; i++) {
    const phi = -halfAngle + (2 * halfAngle * i) / segments;
    place([0, length * Math.sin(phi), length * Math.cos(phi)], i + 1);
  }
  return out;
}

/** `a_se3_b` from two frames' world poses at sample `k` of a bake, or null if either is absent. */
export function relativePose(baked: BakedScenario, a: string, b: string, k = 0): Iso3Schema | null {
  const [i, j] = [baked.frames.indexOf(a), baked.frames.indexOf(b)];
  const sample = baked.samples[k];
  if (i < 0 || j < 0 || !sample) return null;
  return composeIso3(invertIso3(sample.world_se3_frame[i]!), sample.world_se3_frame[j]!);
}

/** What `camera` observed at sample `k`, if `k` is a capture with observations. */
export function observedAt(
  observations: Observations | null,
  baked: BakedScenario,
  camera: string,
  k: number,
): ObservedView | null {
  const id = baked.samples[k]?.capture?.id;
  if (!observations || id === undefined) return null;
  return observations.captures.find((c) => c.id === id)?.views.find((v) => v.camera === camera) ?? null;
}

/**
 * What to draw of `report`'s estimate at `scale` (1: as estimated): each calibrated camera's
 * offset, its laser plane over the true fan (the camera's own laser, or the only one), and the
 * target's outline. `null` when the report has no offsets (no hand-eye or no target pose).
 */
export function estimateDrawing(
  report: CalibrationReport,
  scale: number,
  scene: SceneSpec,
  baked: BakedScenario,
  extent: (target: string) => readonly [number, number] | null,
): EstimateDrawing | null {
  const offsets = report.estimate.offsets;
  if (!offsets) return null;
  const lasers = scene.lasers ?? [];
  const cameras = report.cameras.flatMap((id, j) => {
    const offset = offsets.cameras[j];
    if (!offset) return [];
    const planes = offsets.planes[j];
    const laser = lasers.find((l) => l.camera === id) ?? (lasers.length === 1 ? lasers[0] : undefined);
    const cameraSe3Laser = laser ? relativePose(baked, id, laser.id) : null;
    const fan =
      planes && laser && cameraSe3Laser
        ? ontoPlane(
            fanInCamera(cameraSe3Laser, laser.fan_half_angle, laser.fan_length),
            exaggeratePlane(planes[0], planes[1], scale),
          )
        : null;
    return [{ id, offset: exaggerate(offset, scale), fan }];
  });
  const target = scene.targets?.length === 1 ? scene.targets[0]! : null;
  const size = target ? extent(target.id) : null;
  return {
    cameras,
    target:
      target && size
        ? { id: target.id, offset: exaggerate(offsets.target, scale), outline: targetOutline(size) }
        : null,
  };
}
