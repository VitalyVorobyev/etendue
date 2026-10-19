/**
 * A laser-triangulation head as the Optics workspace edits it: the parts (sensor, lens, laser),
 * the camera's setting (f-number, focus, tilt), where the laser sits (a baseline off the camera
 * axis, aimed at a point on it) and the window it must work in. `toDesign` turns it into the
 * library's `HeadDesign`, with the camera's params derived by the kernel (ADR 0008).
 */

import { type HeadDesign, type LibraryItem, deriveOptics } from "@etendue/wasm";
import { Matrix4, Quaternion, Vector3 } from "three";

import { type Project, itemsOf } from "../io/project";

export interface HeadParams {
  /** The design's name and library id. */
  name: string;
  id: string;
  sensor: { name: string; pixel_pitch_m: number; resolution: [number, number] };
  lens: {
    name: string;
    focal_length_m: number;
    min_f_number: number;
    principal_gap_m: number;
    entrance_pupil_m: number;
  };
  f_number: number;
  /** From the camera's image-plane datum (m). */
  focus_distance_m: number;
  /** Physical sensor tilt `[τx, τy]` (rad). */
  tilt: [number, number];
  laser: { name: string; wavelength_nm: number; fan_half_angle: number; beam_waist_m: number | null };
  fan_length_m: number;
  /** The laser's origin: `baseline` off the camera axis (toward camera −y), `height` along it (m). */
  baseline_m: number;
  height_m: number;
  /** Where the laser's central ray crosses the camera axis (m from the datum). */
  aim_m: number;
  /** The window, centred on the aim point: depth along the laser, width along its line (m). */
  depth_m: number;
  width_m: number;
  /** The blur the window must stay under (px). */
  max_blur_px: number;
}

/** The laser's pose in the camera body for `p`: its line along camera x, aimed at the axis. */
export function laserPose(p: HeadParams): {
  rotation: [number, number, number, number];
  translation: [number, number, number];
} {
  const origin = new Vector3(0, -p.baseline_m, p.height_m);
  const central = new Vector3(0, 0, p.aim_m).sub(origin).normalize();
  const line = new Vector3(1, 0, 0);
  const normal = new Vector3().crossVectors(line, central);
  const q = new Quaternion().setFromRotationMatrix(new Matrix4().makeBasis(normal, line, central));
  return { rotation: [q.x, q.y, q.z, q.w], translation: [origin.x, origin.y, origin.z] };
}

/** The window's range along the laser's central ray, from its origin (m). */
export function windowRange(p: HeadParams): [number, number] {
  const centre = Math.hypot(p.baseline_m, p.aim_m - p.height_m);
  return [Math.max(1e-3, centre - p.depth_m / 2), centre + p.depth_m / 2];
}

/** The library's design for `p`. Throws when the optics are not physical (a focus too close). */
export function toDesign(p: HeadParams): HeadDesign {
  const [w, h] = p.sensor.resolution;
  const optics = {
    sensor: { name: p.sensor.name, pixel_pitch_m: p.sensor.pixel_pitch_m },
    lens: {
      name: p.lens.name,
      focal_length_m: p.lens.focal_length_m,
      f_number: p.f_number,
      principal_gap_m: p.lens.principal_gap_m,
      entrance_pupil_m: p.lens.entrance_pupil_m,
    },
    focus_distance_m: p.focus_distance_m,
    tilt_rad: p.tilt,
  };
  const asBuilt = {
    projection: { type: "pinhole" as const },
    distortion: { type: "none" as const },
    sensor: { type: "identity" as const },
    intrinsics: { type: "fx_fy_cx_cy_skew" as const, fx: 1, fy: 1, cx: w / 2, cy: h / 2, skew: 0 },
  };
  const { params } = deriveOptics(optics, asBuilt);
  const [near, far] = windowRange(p);
  return {
    camera: { params, resolution: [w, h], optics },
    laser: {
      wavelength_nm: p.laser.wavelength_nm,
      fan_half_angle: p.laser.fan_half_angle,
      ...(p.laser.beam_waist_m !== null ? { beam_waist_m: p.laser.beam_waist_m } : {}),
    },
    fan_length_m: p.fan_length_m,
    camera_se3_laser: laserPose(p),
    window: { near_m: near, far_m: far, width_m: p.width_m, max_blur_px: p.max_blur_px },
  };
}

/** `p` with the camera setting `s` (an optimiser's, say). */
export function withSetting(
  p: HeadParams,
  s: { tilt: [number, number]; focus_distance_m: number; f_number: number },
): HeadParams {
  return { ...p, tilt: s.tilt, focus_distance_m: s.focus_distance_m, f_number: s.f_number };
}

/** A head to start from: the project's first sensor, lens and laser, or a 25 mm IMX174 rig. */
export function initialHead(project: Project | null): HeadParams {
  const first = (kind: "sensor" | "lens" | "laser") => (project ? itemsOf(project, kind)[0]?.[1] : undefined);
  const sensor = first("sensor");
  const lens =
    itemsOf(project ?? EMPTY, "lens").find(([, l]) => l.lens!.focal_length_m === 0.025)?.[1] ?? first("lens");
  const laser = first("laser");
  return {
    name: "Laser head",
    id: "laser_head",
    sensor: sensor?.sensor
      ? { name: sensor.name, pixel_pitch_m: sensor.sensor.pixel_pitch_m, resolution: sensor.sensor.resolution }
      : { name: "Sony IMX174", pixel_pitch_m: 5.86e-6, resolution: [1936, 1216] },
    lens: lens?.lens
      ? {
          name: lens.name,
          focal_length_m: lens.lens.focal_length_m,
          min_f_number: lens.lens.min_f_number,
          principal_gap_m: lens.lens.principal_gap_m ?? 0,
          entrance_pupil_m: lens.lens.entrance_pupil_m ?? 0,
        }
      : { name: "25 mm", focal_length_m: 0.025, min_f_number: 1.4, principal_gap_m: 0, entrance_pupil_m: 0 },
    f_number: 4,
    focus_distance_m: 0.35,
    tilt: [0, 0],
    laser: laser?.laser
      ? {
          name: laser.name,
          wavelength_nm: laser.laser.wavelength_nm,
          fan_half_angle: laser.laser.fan_half_angle,
          beam_waist_m: laser.laser.beam_waist_m ?? null,
        }
      : { name: "660 nm line laser", wavelength_nm: 660, fan_half_angle: 0.26, beam_waist_m: 5e-5 },
    fan_length_m: 1,
    baseline_m: 0.1,
    height_m: 0.05,
    aim_m: 0.35,
    depth_m: 0.12,
    width_m: 0.04,
    max_blur_px: 2,
  };
}

const EMPTY: Project = {
  source: { name: "", has: () => false, readText: () => Promise.reject(new Error("empty")), url: () => "" },
  root: "",
  location: "",
  write: null,
  manifest: { version: 1, name: "" },
  cells: [],
  library: new Map(),
  problems: [],
};

/** The library item for `p`, to save. */
export function toItem(p: HeadParams): LibraryItem {
  return { version: 1, id: p.id, name: p.name, head: toDesign(p) };
}

/**
 * `p` from a library head, when its laser sits as this editor places lasers (its line along
 * camera x, aimed at the axis); `null` otherwise.
 */
export function fromItem(item: LibraryItem): HeadParams | null {
  const head = item.head;
  const optics = head?.camera.optics;
  if (!head || !optics) return null;
  const [ox, oy, oz] = head.camera_se3_laser.translation;
  const q = new Quaternion(...head.camera_se3_laser.rotation);
  const line = new Vector3(0, 1, 0).applyQuaternion(q);
  const central = new Vector3(0, 0, 1).applyQuaternion(q);
  if (Math.abs(ox) > 1e-9 || Math.abs(Math.abs(line.x) - 1) > 1e-9 || Math.abs(central.y) < 1e-9) return null;
  const t = -oy / central.y;
  const aim = oz + t * central.z;
  const window = head.window;
  const centre = Math.hypot(oy, aim - oz);
  return {
    name: item.name,
    id: item.id,
    sensor: {
      name: optics.sensor.name ?? "sensor",
      pixel_pitch_m: optics.sensor.pixel_pitch_m,
      resolution: head.camera.resolution,
    },
    lens: {
      name: optics.lens.name ?? "lens",
      focal_length_m: optics.lens.focal_length_m,
      min_f_number: Math.min(1.4, optics.lens.f_number),
      principal_gap_m: optics.lens.principal_gap_m ?? 0,
      entrance_pupil_m: optics.lens.entrance_pupil_m ?? 0,
    },
    f_number: optics.lens.f_number,
    focus_distance_m: optics.focus_distance_m,
    tilt: optics.tilt_rad ?? [0, 0],
    laser: {
      name: "laser",
      wavelength_nm: head.laser.wavelength_nm,
      fan_half_angle: head.laser.fan_half_angle,
      beam_waist_m: head.laser.beam_waist_m ?? null,
    },
    fan_length_m: head.fan_length_m,
    baseline_m: -oy,
    height_m: oz,
    aim_m: aim,
    depth_m: window ? window.far_m - window.near_m : 0.1,
    width_m: window?.width_m ?? 0.04,
    max_blur_px: window?.max_blur_px ?? 2,
    // A window off its centre is re-centred on the aim point.
    ...(window ? { depth_m: 2 * Math.max(centre - window.near_m, window.far_m - centre) } : {}),
  };
}
