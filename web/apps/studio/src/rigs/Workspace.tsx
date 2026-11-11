/**
 * The Rigs workspace (P11): k identical units as a stereo pair or a ring, and what they cover. Its
 * rig starts as a stereo pair of the Optics workspace's head (or the project's default camera);
 * Place puts it into the open cell.
 */

import { Callout, Empty } from "@vitavision/ui";
import { type ReactNode, useMemo, useState } from "react";

import type { Project } from "../io/project";
import { type Baked, expandRig } from "../kernel/etendue";
import type { HeadParams } from "../optics/design";
import { LOOKING_DOWN } from "../optics/Workspace";
import { IDENTITY, applyLayout, uniqueId } from "../scene/edit";
import type { Studio } from "../state/studio";
import { analyse } from "./analysis";
import { type RigParams, initialRig, toItem, toLayout } from "./design";
import { RigForm, RigReadouts, TiltChart } from "./RigPanels";
import { RigView, type VoxelShow } from "./RigView";
import { CoverageLegend, SliceMaps } from "./SliceMaps";

export interface RigsWorkspace {
  left: ReactNode;
  main: ReactNode;
  right: ReactNode;
}

export function useRigsWorkspace({
  studio,
  project,
  head,
  shownHead,
  current,
  loading,
  parent,
  onPlaced,
}: {
  studio: Studio;
  project: Project | null;
  /** The Optics workspace's edited head (`null` until edited): it seeds the rig's unit. */
  head: HeadParams | null;
  /** The Optics workspace's head as shown, which the rig form offers. */
  shownHead: HeadParams | null;
  current: Baked | null;
  loading: boolean;
  /** The frame a placed rig attaches to (the selection's, else the world). */
  parent: string;
  /** A rig was placed into the cell, as `id`. */
  onPlaced: (id: string) => void;
}): RigsWorkspace {
  const [rig, setRig] = useState<RigParams | null>(null);
  const [voxels, setVoxels] = useState<VoxelShow>({ kind: "views", min: 2 });
  const [selected, setSelected] = useState<string | null>(null);
  const startRig = useMemo(
    () => (project ? initialRig(project, head) : null),
    // A head edited later does not replace the rig's unit; only the first one seeds it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [project],
  );
  const params = rig ?? startRig;
  const analysis = useMemo(() => (params ? analyse(params) : null), [params]);

  // A rig frame under the selected frame (or above the world origin, looking down), its cameras
  // and lasers expanded from the layout.
  const place = (p: RigParams) => {
    if (!current) return;
    const pose = parent === "world" ? LOOKING_DOWN : IDENTITY;
    const id = uniqueId(current.loaded.scene, p.id || "rig");
    const layout = toLayout(p);
    const ok = studio.edit(`place ${p.name}`, (d) => {
      const withRig = { ...d.scene, rigs: [...(d.scene.rigs ?? []), { id, parent, parent_se3_self: pose, layout }] };
      return { ...d, scene: applyLayout(withRig, id, layout, expandRig(layout, id)) };
    });
    if (ok) onPlaced(id);
  };

  const left =
    params && analysis ? (
      <RigForm
        params={params}
        onChange={setRig}
        analysis={analysis}
        project={project}
        head={shownHead}
        onSave={project?.write ? (p) => studio.saveLibraryItem(toItem(p)) : null}
        onPlace={current ? place : null}
      />
    ) : null;
  const main =
    params && analysis ? (
      typeof analysis === "string" ? (
        <div className="p-4">
          <Callout tone="warning" title="The kernel refused this rig">
            <p>{analysis}</p>
          </Callout>
        </div>
      ) : (
        <div className="grid h-full min-h-0 grid-rows-[3fr_2fr] divide-y divide-line">
          <div className="relative min-h-0">
            <RigView analysis={analysis} show={voxels} selected={selected} onSelect={setSelected} />
            <div className="pointer-events-none absolute bottom-2 left-3 rounded bg-surface/80 px-2 py-1">
              <CoverageLegend cameras={analysis.cameras.length} pairs={analysis.fans.length} />
            </div>
          </div>
          <div className="grid min-h-0 grid-cols-[2fr_1fr] divide-x divide-line">
            <SliceMaps analysis={analysis} params={params} onChange={setRig} />
            <div className="min-h-0 p-2" data-testid="rig-tilt-chart">
              <TiltChart params={params} />
            </div>
          </div>
        </div>
      )
    ) : (
      <Empty>{loading ? "Loading…" : "The Rigs workspace needs a camera or a head in the project's library."}</Empty>
    );
  const right =
    params && analysis ? <RigReadouts params={params} analysis={analysis} show={voxels} onShow={setVoxels} /> : null;
  return { left, main, right };
}
