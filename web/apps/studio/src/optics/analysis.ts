/**
 * Everything the Optics workspace draws for a head, from the kernel in one pass: the design, the
 * camera's derived optics, the Scheimpflug section, the map over the window, and the window's
 * worst figures. A head the kernel refuses (a focus closer than the lens reaches, say) gives the
 * reason instead.
 */

import {
  type DerivedOptics,
  type HeadDesign,
  type HeadMap,
  type HeadSection,
  type HeadSetting,
  deriveOptics,
  headMap,
  headScheimpflug,
  headSection,
} from "@etendue/wasm";

import { type HeadParams, toDesign, windowRange } from "./design";

/** The window's figures over its visible samples. */
export interface WindowStats {
  /** The share of the window the camera sees. */
  coverage: number;
  worstBlurPx: number;
  /** Depth resolution: best and worst (m per px). */
  depthPerPx: [number, number];
  /** Lateral resolution along the line: best and worst (m per px). */
  lateralPerPx: [number, number];
  triangulationRad: [number, number];
}

export interface HeadAnalysis {
  design: HeadDesign;
  derived: DerivedOptics;
  section: HeadSection;
  map: HeadMap;
  /** The Scheimpflug setting at this f-number, if the laser plane has one. */
  hinge: HeadSetting | null;
  /** The window along the laser's central ray (m). */
  range: [number, number];
  stats: WindowStats;
}

/** The map's grid: across the line by along the range. */
export const GRID: [number, number] = [64, 48];

function stats(map: HeadMap): WindowStats {
  let seen = 0;
  let worst = 0;
  const depth: [number, number] = [Infinity, 0];
  const lateral: [number, number] = [Infinity, 0];
  const tri: [number, number] = [Infinity, 0];
  for (let i = 0; i < map.visible.length; i++) {
    if (!map.visible[i]) continue;
    seen++;
    worst = Math.max(worst, map.blur_px[i]!);
    depth[0] = Math.min(depth[0], map.depth_m_per_px[i]!);
    depth[1] = Math.max(depth[1], map.depth_m_per_px[i]!);
    lateral[0] = Math.min(lateral[0], map.lateral_m_per_px[i]!);
    lateral[1] = Math.max(lateral[1], map.lateral_m_per_px[i]!);
    tri[0] = Math.min(tri[0], map.triangulation_rad[i]!);
    tri[1] = Math.max(tri[1], map.triangulation_rad[i]!);
  }
  return {
    coverage: seen / map.visible.length,
    worstBlurPx: worst,
    depthPerPx: depth,
    lateralPerPx: lateral,
    triangulationRad: tri,
  };
}

/** Analyse `p`; a string says why the kernel refused it. */
export function analyse(p: HeadParams): HeadAnalysis | string {
  try {
    const design = toDesign(p);
    const derived = deriveOptics(design.camera.optics!, design.camera.params);
    const range = windowRange(p);
    const probes = [range[0], (range[0] + range[1]) / 2, range[1]];
    const section = headSection(design, { probes, blur_limit_px: p.max_blur_px });
    const map = headMap(design, null, GRID[0], GRID[1]);
    let hinge: HeadSetting | null = null;
    try {
      hinge = headScheimpflug(design);
    } catch {
      hinge = null;
    }
    return { design, derived, section, map, hinge, range, stats: stats(map) };
  } catch (e) {
    return e instanceof Error ? e.message.split("\n")[0]! : String(e);
  }
}
