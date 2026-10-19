/**
 * A rig as the Rigs workspace edits it: one unit (a camera, or a head with its laser on the
 * camera's body) repeated as a stereo pair or a ring, and the box its coverage is judged over.
 * `toLayout` gives the scene's `RigLayout` (P7-3, P11); stereo is the same layout with two
 * cameras on the rig's x axis and their image +X along it (`image_x: "common"`).
 */

import { type CameraTemplate, type LaserLayout, type LibraryItem, type RigLayout, deriveOptics } from "@etendue/wasm";

import { type Project, defaultOf, itemsOf } from "../io/project";
import { type HeadParams, toDesign } from "../optics/design";

export type Arrangement = "stereo" | "ring";

/** What every position of the rig carries. */
export interface RigUnit {
  kind: "camera" | "head";
  name: string;
  camera: CameraTemplate;
  /** A head's laser, fixed to the camera's body. */
  laser: LaserLayout | null;
}

export interface RigParams {
  name: string;
  id: string;
  unit: RigUnit;
  arrangement: Arrangement;
  /** Cameras on a ring (a stereo pair has two). */
  count: number;
  /** ρ: each camera's distance from the rig axis (m); a stereo baseline is 2ρ. */
  radius_m: number;
  /** The camera axes meet the rig axis at the working distance; otherwise they are tilted by `tilt_rad`. */
  converge: boolean;
  working_distance_m: number;
  /** φ: each camera axis's angle to the rig axis, toward it (rad), when not converging. */
  tilt_rad: number;
  /** Azimuth of camera 0 (rad). */
  phase_rad: number;
  /**
   * Focus every camera on the point of its axis at the working distance (a camera unit with
   * optics; a head keeps the focus its laser plane was designed for).
   */
  refocus: boolean;
  /** A ring camera's image +X: along the ring, or away from the axis. */
  image_x: "tangential" | "radial";
  /** The box coverage is judged over: ± `half_m` across, `near_m` to `far_m` along the rig axis. */
  box: { half_m: number; near_m: number; far_m: number };
  max_blur_px: number;
  /** Voxels along each axis. */
  resolution: number;
}

/** The rig's tilt: the angle every camera axis makes with the rig axis. */
export function tiltOf(p: RigParams): number {
  return p.converge ? Math.atan2(p.radius_m, p.working_distance_m) : p.tilt_rad;
}

/** Whether `p`'s cameras are refocused on the working distance. */
export function refocuses(p: RigParams): boolean {
  return p.refocus && p.unit.kind === "camera" && p.unit.camera.optics != null;
}

/** The unit's camera, refocused on the working distance when `p` says so. */
function cameraOf(p: RigParams): CameraTemplate {
  const camera = p.unit.camera;
  if (!refocuses(p) || !camera.optics) return camera;
  const optics = { ...camera.optics, focus_distance_m: p.working_distance_m / Math.cos(tiltOf(p)) };
  return { ...camera, optics, params: deriveOptics(optics, camera.params).params };
}

/** Throws when a refocused camera cannot focus that close. */
export function toLayout(p: RigParams): RigLayout {
  const stereo = p.arrangement === "stereo";
  return {
    count: stereo ? 2 : p.count,
    radius_m: p.radius_m,
    phase_rad: stereo ? 0 : p.phase_rad,
    working_distance_m: p.working_distance_m,
    ...(p.converge ? {} : { tilt_rad: p.tilt_rad }),
    image_x: stereo ? "common" : p.image_x,
    camera: cameraOf(p),
    ...(p.unit.laser ? { lasers: p.unit.laser } : {}),
  };
}

/** The library item for `p`. */
export function toItem(p: RigParams): LibraryItem {
  return { version: 1, id: p.id, name: p.name, rig: { layout: toLayout(p) } };
}

/** A head designed in the Optics workspace, as a unit. */
export function unitOfHead(head: HeadParams): RigUnit {
  const design = toDesign(head);
  return {
    kind: "head",
    name: head.name,
    camera: design.camera,
    laser: {
      camera_se3_laser: design.camera_se3_laser,
      fan_half_angle: design.laser.fan_half_angle,
      fan_length: design.fan_length_m,
      wavelength_nm: design.laser.wavelength_nm,
      beam_waist_m: design.laser.beam_waist_m ?? 5e-4,
      ...(design.laser.waist_distance_m != null ? { waist_distance_m: design.laser.waist_distance_m } : {}),
    },
  };
}

/** A library camera or head as a unit; `null` for other items. */
export function unitOfItem(item: LibraryItem): RigUnit | null {
  if (item.camera) return { kind: "camera", name: item.name, camera: item.camera, laser: null };
  const head = item.head;
  if (!head) return null;
  return {
    kind: "head",
    name: item.name,
    camera: head.camera,
    laser: {
      camera_se3_laser: head.camera_se3_laser,
      fan_half_angle: head.laser.fan_half_angle,
      fan_length: head.fan_length_m,
      wavelength_nm: head.laser.wavelength_nm,
      beam_waist_m: head.laser.beam_waist_m ?? 5e-4,
      ...(head.laser.waist_distance_m != null ? { waist_distance_m: head.laser.waist_distance_m } : {}),
    },
  };
}

/** The box a rig is judged over by default: around its working point, as wide as it is deep. */
function boxAround(working_distance_m: number): RigParams["box"] {
  const depth = Math.max(0.05, 0.5 * working_distance_m);
  return { half_m: depth / 2, near_m: working_distance_m - depth / 2, far_m: working_distance_m + depth / 2 };
}

/** `p` from a library rig. */
export function fromItem(item: LibraryItem): RigParams | null {
  const layout = item.rig?.layout;
  if (!layout) return null;
  const laser = layout.lasers ?? null;
  const stereo = layout.count === 2 && layout.image_x === "common";
  return {
    name: item.name,
    id: item.id,
    unit: {
      kind: laser?.camera_se3_laser ? "head" : "camera",
      name: item.name,
      camera: layout.camera,
      laser,
    },
    arrangement: stereo ? "stereo" : "ring",
    count: layout.count,
    radius_m: layout.radius_m,
    converge: layout.tilt_rad == null,
    working_distance_m: layout.working_distance_m,
    tilt_rad: layout.tilt_rad ?? Math.atan2(layout.radius_m, layout.working_distance_m),
    phase_rad: layout.phase_rad ?? 0,
    // A saved rig's cameras are as saved.
    refocus: false,
    image_x: layout.image_x === "radial" ? "radial" : "tangential",
    box: boxAround(layout.working_distance_m),
    max_blur_px: 2,
    resolution: 48,
  };
}

/**
 * A rig to start from: a stereo pair of the Optics workspace's head when there is one, else of
 * the project's default camera (or its first), 120 mm apart, converging at 0.4 m.
 */
export function initialRig(project: Project | null, head: HeadParams | null): RigParams | null {
  let unit: RigUnit | null = null;
  if (head) {
    try {
      unit = unitOfHead(head);
    } catch {
      unit = null;
    }
  }
  if (!unit) {
    const camera = defaultOf(project, "camera") ?? (project ? itemsOf(project, "camera")[0]?.[1] : undefined);
    unit = camera ? unitOfItem(camera) : null;
  }
  if (!unit) return null;
  const working = 0.4;
  return {
    name: "Stereo pair",
    id: "stereo_pair",
    unit,
    arrangement: "stereo",
    count: 3,
    radius_m: 0.06,
    converge: true,
    working_distance_m: working,
    tilt_rad: Math.atan2(0.06, working),
    phase_rad: 0,
    refocus: true,
    image_x: "tangential",
    box: boxAround(working),
    max_blur_px: 2,
    resolution: 48,
  };
}

/** `p` with its working distance moved, the box following. */
export function withWorkingDistance(p: RigParams, working_distance_m: number): RigParams {
  const shift = working_distance_m - p.working_distance_m;
  return {
    ...p,
    working_distance_m,
    box: { ...p.box, near_m: Math.max(0.01, p.box.near_m + shift), far_m: p.box.far_m + shift },
  };
}
