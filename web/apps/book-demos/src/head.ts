/**
 * A laser-triangulation head as the widgets set it up: the studio's Optics workspace model
 * (web/apps/studio/src/optics/design.ts) reduced to what the widgets change. The camera looks
 * along +z; the laser sits `baseline` off the axis (toward camera −y) and `height` along it,
 * aimed at the point `aim` on the axis, with its line along camera x.
 */

import { type HeadDesign, deriveOptics } from "@etendue/wasm";

export interface Head {
  /** Sensor: pixel pitch (m) and resolution. */
  pitch_m: number;
  resolution: [number, number];
  focal_length_m: number;
  f_number: number;
  /** From the image-plane datum (m). */
  focus_distance_m: number;
  /** Physical sensor tilt `[τx, τy]` (rad). */
  tilt: [number, number];
  wavelength_nm: number;
  /** The beam's 1/e² waist radius (m). */
  beam_waist_m: number;
  baseline_m: number;
  height_m: number;
  aim_m: number;
  /** The window, centred on the aim point: depth along the laser and width along its line (m). */
  depth_m: number;
  width_m: number;
  max_blur_px: number;
}

/** An IMX174 behind a 25 mm lens, a 660 nm laser 100 mm off axis aimed 350 mm out. */
export const DEFAULT_HEAD: Head = {
  pitch_m: 5.86e-6,
  resolution: [1936, 1216],
  focal_length_m: 0.025,
  f_number: 4,
  focus_distance_m: 0.35,
  tilt: [0, 0],
  wavelength_nm: 660,
  beam_waist_m: 5e-5,
  baseline_m: 0.1,
  height_m: 0.05,
  aim_m: 0.35,
  depth_m: 0.12,
  width_m: 0.04,
  max_blur_px: 2,
};

type V3 = [number, number, number];

const cross = (a: V3, b: V3): V3 => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const norm = (a: V3): V3 => {
  const n = Math.hypot(...a);
  return [a[0] / n, a[1] / n, a[2] / n];
};

/** The unit quaternion `[x, y, z, w]` of the rotation whose columns are `x`, `y`, `z`. */
export function quatFromBasis(x: V3, y: V3, z: V3): [number, number, number, number] {
  const [m00, m10, m20] = x;
  const [m01, m11, m21] = y;
  const [m02, m12, m22] = z;
  const trace = m00 + m11 + m22;
  if (trace > 0) {
    const s = 0.5 / Math.sqrt(trace + 1);
    return [(m21 - m12) * s, (m02 - m20) * s, (m10 - m01) * s, 0.25 / s];
  }
  if (m00 > m11 && m00 > m22) {
    const s = 2 * Math.sqrt(1 + m00 - m11 - m22);
    return [0.25 * s, (m01 + m10) / s, (m02 + m20) / s, (m21 - m12) / s];
  }
  if (m11 > m22) {
    const s = 2 * Math.sqrt(1 + m11 - m00 - m22);
    return [(m01 + m10) / s, 0.25 * s, (m12 + m21) / s, (m02 - m20) / s];
  }
  const s = 2 * Math.sqrt(1 + m22 - m00 - m11);
  return [(m02 + m20) / s, (m12 + m21) / s, 0.25 * s, (m10 - m01) / s];
}

/** The laser's pose in the camera body: origin off axis, fan in its y–z plane, aimed at the axis. */
export function laserPose(h: Head): { rotation: [number, number, number, number]; translation: V3 } {
  const origin: V3 = [0, -h.baseline_m, h.height_m];
  const central = norm([0, h.baseline_m, h.aim_m - h.height_m]);
  const line: V3 = [1, 0, 0];
  const normal = cross(line, central);
  return { rotation: quatFromBasis(normal, line, central), translation: origin };
}

/** From the laser's origin to its aim point (m): the window's centre. */
export const aimDistance = (h: Head): number => Math.hypot(h.baseline_m, h.aim_m - h.height_m);

/** The window's range along the laser's central ray, from its origin (m). */
export function windowRange(h: Head): [number, number] {
  const centre = aimDistance(h);
  return [Math.max(1e-3, centre - h.depth_m / 2), centre + h.depth_m / 2];
}

/** The library's head design for `h`, its camera params derived by the kernel. */
export function toDesign(h: Head): HeadDesign {
  const [w, hgt] = h.resolution;
  const optics = {
    sensor: { pixel_pitch_m: h.pitch_m },
    lens: { focal_length_m: h.focal_length_m, f_number: h.f_number },
    focus_distance_m: h.focus_distance_m,
    tilt_rad: h.tilt,
  };
  const asBuilt = {
    projection: { type: "pinhole" as const },
    distortion: { type: "none" as const },
    sensor: { type: "identity" as const },
    intrinsics: { type: "fx_fy_cx_cy_skew" as const, fx: 1, fy: 1, cx: w / 2, cy: hgt / 2, skew: 0 },
  };
  const { params } = deriveOptics(optics, asBuilt);
  const [near, far] = windowRange(h);
  return {
    camera: { params, resolution: [w, hgt], optics },
    laser: {
      wavelength_nm: h.wavelength_nm,
      fan_half_angle: 0.26,
      beam_waist_m: h.beam_waist_m,
      waist_distance_m: aimDistance(h),
    },
    fan_length_m: 1,
    camera_se3_laser: laserPose(h),
    window: { near_m: near, far_m: far, width_m: h.width_m, max_blur_px: h.max_blur_px },
  };
}
