/**
 * The Optics workspace (P10): a triangulation head designed against its window. Its head is kept
 * across workspaces and starts from the project's parts; Place puts it into the open cell.
 */

import { Callout, Empty } from "@vitavision/ui";
import { type ReactNode, useMemo, useState } from "react";

import type { Project } from "../io/project";
import { IDENTITY, addEntity, uniqueId } from "../scene/edit";
import type { Baked } from "../kernel/etendue";
import type { Studio } from "../state/studio";
import { analyse } from "./analysis";
import { type HeadParams, initialHead, placement, toItem } from "./design";
import { HeadView } from "./HeadView";
import { OpticsForm, OpticsReadouts } from "./OpticsPanels";
import { PlaneMap } from "./PlaneMap";
import { RangeViews } from "./RangeViews";

/** Above the world origin, looking down: where a design goes without a selected frame. */
export const LOOKING_DOWN = {
  rotation: [1, 0, 0, 0] as [number, number, number, number],
  translation: [0, 0, 0.8] as [number, number, number],
};

export interface OpticsWorkspace {
  left: ReactNode;
  main: ReactNode;
  right: ReactNode;
  /** The head as edited here (`null` until edited): the Rigs workspace seeds its unit from it. */
  edited: HeadParams | null;
  /** The head shown: as edited, else the project's. */
  params: HeadParams | null;
}

export function useOpticsWorkspace({
  studio,
  project,
  current,
  loading,
  parent,
  onPlaced,
}: {
  studio: Studio;
  project: Project | null;
  current: Baked | null;
  loading: boolean;
  /** The frame a placed head attaches to (the selection's, else the world). */
  parent: string;
  /** A head was placed into the cell, its camera `id`. */
  onPlaced: (id: string) => void;
}): OpticsWorkspace {
  const [head, setHead] = useState<HeadParams | null>(null);
  const startHead = useMemo(() => (project ? initialHead(project) : null), [project]);
  const params = head ?? startHead;
  const analysis = useMemo(() => (params ? analyse(params) : null), [params]);

  // Under the selected frame, or above the world origin looking down; its camera then selected.
  const place = (p: HeadParams) => {
    if (!current) return;
    const scene = current.loaded.scene;
    const pose = parent === "world" ? LOOKING_DOWN : IDENTITY;
    const cameraId = uniqueId(scene, "head_cam");
    const laserId = uniqueId(
      { ...scene, cameras: [...(scene.cameras ?? []), { id: cameraId } as never] },
      "head_laser",
    );
    const { camera, laser } = placement(p, parent, pose, cameraId, laserId);
    const ok = studio.edit(`place ${p.name}`, (d) => ({
      ...d,
      scene: addEntity(addEntity(d.scene, "cameras", camera), "lasers", laser),
    }));
    if (ok) onPlaced(cameraId);
  };

  const left =
    params && analysis ? (
      <OpticsForm
        params={params}
        onChange={setHead}
        analysis={analysis}
        project={project}
        onSave={project?.write ? (p) => studio.saveLibraryItem(toItem(p)) : null}
        onPlace={current ? place : null}
      />
    ) : null;
  const main =
    params && analysis ? (
      typeof analysis === "string" ? (
        <div className="p-4">
          <Callout tone="warning" title="The kernel refused this head">
            <p>{analysis}</p>
          </Callout>
        </div>
      ) : (
        <div className="grid h-full min-h-0 grid-rows-[3fr_2fr] divide-y divide-line">
          <HeadView analysis={analysis} params={params} onChange={setHead} />
          <div className="grid min-h-0 grid-cols-[minmax(0,3fr)_minmax(0,2fr)] divide-x divide-line">
            <PlaneMap
              map={analysis.map}
              range={analysis.range}
              width={params.width_m}
              blurLimitPx={params.max_blur_px}
            />
            <RangeViews
              map={analysis.map}
              range={analysis.range}
              resolution={analysis.design.camera.resolution}
              blurLimitPx={params.max_blur_px}
              focus={params.laser.beam_waist_m !== null ? params.laser.waist_distance_m : null}
            />
          </div>
        </div>
      )
    ) : (
      <Empty>{loading ? "Loading…" : "The Optics workspace needs a project's parts."}</Empty>
    );
  const right = params && analysis ? <OpticsReadouts params={params} analysis={analysis} /> : null;
  return { left, main, right, edited: head, params };
}
