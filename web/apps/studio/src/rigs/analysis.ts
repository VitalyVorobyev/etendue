/**
 * Everything the Rigs workspace draws for a rig, from the kernel: its layout expanded into
 * cameras and lasers, where each camera's projection frame and field are (a throwaway scene
 * resolves and backprojects them, so the studio does no camera math), and the coverage of the
 * rig's box with its volumes. A rig the kernel refuses gives the reason instead.
 */

import {
  type CameraSpec,
  type CoverageGrid,
  EtendueScene,
  type LaserSpec,
  type RigCoverage,
  type RigLayout,
  type SceneSpec,
  type Sharpness,
  expandRig,
  rigCoverage,
  rigSharpness,
} from "@etendue/wasm";
import { imageBorderPixels } from "@vitavision/three";
import { Matrix4, Quaternion, Vector3 } from "three";

import { IDENTITY } from "../scene/edit";
import { type RigParams, tiltOf, toLayout } from "./design";

/** A camera of the rig, drawn at its projection frame (the entrance pupil, for physical optics). */
export interface RigEye {
  id: string;
  /** The projection frame in the rig's frame. */
  rigFromProjection: Matrix4;
  /** The field's border rays in the projection frame. */
  borderRays: Float64Array;
}

/** A laser of the rig. */
export interface RigFan {
  id: string;
  rigFromLaser: Matrix4;
  halfAngle: number;
  length: number;
}

/** Volumes over the box (m³). */
export interface RigVolumes {
  /** Seen by at least 1, 2, and every camera. */
  seen: [number, number, number];
  /** Measured by at least one pair, and by two. */
  measured: [number, number];
}

export interface RigAnalysis {
  layout: RigLayout;
  cameras: CameraSpec[];
  lasers: LaserSpec[];
  eyes: RigEye[];
  fans: RigFan[];
  grid: CoverageGrid;
  coverage: RigCoverage;
  /** Where each camera is sharp along its axis, as the coverage judges it. */
  sharp: Sharpness[];
  volumes: RigVolumes;
  /** The angle between camera 0's axis and camera 1's (rad); 0 for one camera. */
  axisAngle: number;
  /** The distance between neighbouring cameras (m). */
  neighbour_m: number;
}

export function gridOf(p: RigParams, n = p.resolution): CoverageGrid {
  const { half_m, near_m, far_m } = p.box;
  return { min: [-half_m, -half_m, near_m], max: [half_m, half_m, far_m], n: [n, n, n] };
}

const matrix = (iso: { rotation: number[]; translation: number[] }) =>
  new Matrix4().compose(
    new Vector3(...(iso.translation as [number, number, number])),
    new Quaternion(...(iso.rotation as [number, number, number, number])),
    new Vector3(1, 1, 1),
  );

/** The volumes of `coverage` for a rig of `cameras`. */
export function volumesOf(coverage: RigCoverage, cameras: number): RigVolumes {
  const { min, max, n } = coverage.grid;
  const voxel = ((max[0] - min[0]) * (max[1] - min[1]) * (max[2] - min[2])) / (n[0] * n[1] * n[2]);
  const seen = [0, 0, 0];
  const measured = [0, 0];
  const all = Math.max(1, cameras);
  for (let i = 0; i < coverage.views.length; i++) {
    const v = coverage.views[i]!;
    const l = coverage.lit[i]!;
    if (v >= 1) seen[0]!++;
    if (v >= 2) seen[1]!++;
    if (v >= all) seen[2]!++;
    if (l >= 1) measured[0]!++;
    if (l >= 2) measured[1]!++;
  }
  return {
    seen: [seen[0]! * voxel, seen[1]! * voxel, seen[2]! * voxel],
    measured: [measured[0]! * voxel, measured[1]! * voxel],
  };
}

/** Analyse `p`; a string says why the kernel refused it. */
export function analyse(p: RigParams): RigAnalysis | string {
  try {
    const layout = toLayout(p);
    const { cameras, lasers } = expandRig(layout, "rig");
    const grid = gridOf(p);
    const coverage = rigCoverage(layout, grid, { max_blur_px: p.max_blur_px });
    const sharp = rigSharpness(layout, { max_blur_px: p.max_blur_px });
    const scene: SceneSpec = {
      version: 1,
      frames: [{ id: "rig", parent: "world", parent_se3_self: IDENTITY }],
      cameras,
      lasers,
    };
    const session = new EtendueScene(scene, []);
    const placed = new Map<string, Matrix4>();
    let eyes: RigEye[];
    try {
      eyes = cameras.map((c) => {
        const body = matrix(c.parent_se3_self);
        placed.set(c.id, body);
        const mech = session.mechSe3Projection(c.id);
        return {
          id: c.id,
          rigFromProjection: mech ? body.clone().multiply(matrix(mech)) : body,
          borderRays: session.backprojectPixels(c.id, imageBorderPixels(c.resolution[0], c.resolution[1], 8)),
        };
      });
    } finally {
      session.free();
    }
    const fans = lasers.map((l) => {
      const own = matrix(l.parent_se3_self);
      const parent = placed.get(l.parent);
      return {
        id: l.id,
        rigFromLaser: parent ? parent.clone().multiply(own) : own,
        halfAngle: l.fan_half_angle,
        length: l.fan_length,
      };
    });
    const axis = (c: CameraSpec) => new Vector3(0, 0, 1).applyQuaternion(new Quaternion(...c.parent_se3_self.rotation));
    const at = (c: CameraSpec) => new Vector3(...c.parent_se3_self.translation);
    const [c0, c1] = cameras;
    return {
      layout,
      cameras,
      lasers,
      eyes,
      fans,
      grid,
      coverage,
      sharp,
      volumes: volumesOf(coverage, cameras.length),
      axisAngle: c0 && c1 ? axis(c0).angleTo(axis(c1)) : 0,
      neighbour_m: c0 && c1 ? at(c0).distanceTo(at(c1)) : 0,
    };
  } catch (e) {
    return e instanceof Error ? e.message.split("\n")[0]! : String(e);
  }
}

/** How the rig's volumes change with its tilt φ, at a coarse grid: `steps` tilts over `range`. */
export function tiltSweep(
  p: RigParams,
  range: [number, number],
  steps = 19,
): { tilt: number; seen2: number; measured: number }[] {
  const out = [];
  const grid = gridOf(p, 24);
  for (let i = 0; i < steps; i++) {
    const tilt = range[0] + ((range[1] - range[0]) * i) / (steps - 1);
    try {
      const layout = toLayout({ ...p, converge: false, tilt_rad: tilt });
      const coverage = rigCoverage(layout, grid, { max_blur_px: p.max_blur_px });
      const v = volumesOf(coverage, layout.count);
      out.push({ tilt, seen2: v.seen[1], measured: v.measured[0] });
    } catch {
      out.push({ tilt, seen2: 0, measured: 0 });
    }
  }
  return out;
}

export { tiltOf };
