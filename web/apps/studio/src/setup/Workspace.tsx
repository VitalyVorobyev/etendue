/**
 * The Setup workspace: the cell's frame tree (add, remove, robots from the library), the
 * Inspector of the selection, and the handle that moves it in the 3-D view. While the cell builder
 * is open its steps replace the tree and the Inspector.
 */

import type { Playhead } from "@vitavision/workbench";
import { type ReactNode, useCallback, useMemo, useState } from "react";
import library, { type LibraryRobot } from "virtual:etendue-robots";

import type { useCellBuilder } from "../cell/CellBuilder";
import { type Project, defaultOf } from "../io/project";
import type { Baked, KernelFailure } from "../kernel/etendue";
import type { PoseSource } from "../kernel/posed";
import { Inspector } from "../panels/Inspector";
import { Navigator } from "../panels/Navigator";
import type { GizmoTarget } from "../panels/Viewport";
import { RobotLibrary } from "../robots/RobotLibrary";
import {
  type EntityList,
  IDENTITY,
  addEntity,
  entityOf,
  newEntity,
  removeEntity,
  removeRobot,
  uniqueId,
  updateEntity,
} from "../scene/edit";
import { sceneTree } from "../scene/model";
import type { Preview, Studio } from "../state/studio";

export interface SetupWorkspace {
  left: ReactNode;
  right: ReactNode;
  /** The selection's handle: an entity on its frame, a robot on its base. */
  gizmo: GizmoTarget | null;
  /** The robot library, as a dialog. */
  dialog: ReactNode;
}

export function useSetupWorkspace({
  active,
  studio,
  current,
  editing,
  project,
  failure,
  selected,
  onSelect,
  cellBuilder,
  poses,
  playhead,
}: {
  /** Whether the workspace shows. */
  active: boolean;
  studio: Studio;
  current: Baked | null;
  /** The edit in progress, if any. */
  editing: Preview | null;
  project: Project | null;
  failure: KernelFailure | null;
  /** The selected frame or robot. */
  selected: string | null;
  onSelect: (id: string | null) => void;
  cellBuilder: ReturnType<typeof useCellBuilder>;
  /** The bodies' world poses (what the Inspector shows and the handle moves). */
  poses: PoseSource | null;
  playhead: Playhead;
}): SetupWorkspace {
  const [robotLibrary, setRobotLibrary] = useState(false);
  const tree = useMemo(() => (current ? sceneTree(current.loaded.scene, current.baked) : null), [current]);
  const live = useMemo(() => ({ preview: studio.preview, commit: studio.commit, cancel: studio.cancel }), [studio]);

  const isRobot = (id: string | null) => id !== null && (current?.loaded.scene.robots ?? []).some((r) => r.id === id);
  // What is added attaches to the selected frame (a link, an entity), else to the world.
  const parent = selected !== null && current?.baked.frames.includes(selected) ? selected : "world";
  const add = (list: EntityList) => {
    if (!current) return;
    const entity = newEntity(current.loaded.scene, list, parent, {
      camera: defaultOf(project, "camera"),
      laser: defaultOf(project, "laser"),
      target: defaultOf(project, "target"),
    });
    if (studio.edit(`add ${entity.id}`, (d) => ({ ...d, scene: addEntity(d.scene, list, entity) }))) {
      onSelect(entity.id);
    }
  };
  const addRobot = (lib: LibraryRobot) => {
    if (!current) return;
    const id = uniqueId(current.loaded.scene, lib.id);
    void studio
      .addRobot(lib.manifest, id, (d, robot) => ({
        ...d,
        scene: {
          ...d.scene,
          robots: [
            ...(d.scene.robots ?? []),
            {
              id,
              parent,
              parent_se3_self: IDENTITY,
              // Repository-relative: the loader finds it in the robot library, and Save rebases it.
              manifest: lib.manifest,
              // Every joint at 0, or at the nearest limit.
              initial_q: robot.manifest.joints.map((j) => Math.min(Math.max(0, j.lower), j.upper)),
            },
          ],
        },
      }))
      .then((ok) => ok && onSelect(id));
  };
  const replaceRobot = (lib: LibraryRobot) => {
    if (!current || selected === null || !isRobot(selected)) return;
    const id = selected;
    void studio.replaceRobot(lib.manifest, id, (d, robot) => ({
      ...d,
      scene: {
        ...d.scene,
        robots: (d.scene.robots ?? []).map((r) =>
          r.id === id
            ? {
                ...r,
                manifest: lib.manifest,
                initial_q: robot.manifest.joints.map((j) => Math.min(Math.max(0, j.lower), j.upper)),
              }
            : r,
        ),
      },
    }));
  };
  const loadLibraryRobot = useCallback((r: LibraryRobot) => studio.loadLibraryRobot(r.manifest, r.id), [studio]);
  const removable =
    current !== null && selected !== null && (entityOf(current.loaded.scene, selected) !== null || isRobot(selected));
  const remove = () => {
    if (!current || selected === null) return;
    const ok = isRobot(selected)
      ? studio.edit(`remove ${selected}`, (d) => ({
          ...d,
          ...removeRobot(d.scene, d.scenario, selected),
          robots: d.robots.filter((r) => r.id !== selected),
        }))
      : studio.edit(`remove ${selected}`, (d) => ({ ...d, scene: removeEntity(d.scene, selected) }));
    if (ok) onSelect(null);
  };

  // The selection's handle: an entity on its frame, a robot on its base.
  const shownScene = editing?.doc.scene ?? current?.loaded.scene;
  const handled =
    active && shownScene && selected !== null
      ? (entityOf(shownScene, selected) ?? shownScene.robots?.find((r) => r.id === selected) ?? null)
      : null;
  const handleFrame =
    handled && isRobot(selected)
      ? `${selected}/${current?.loaded.robots.find((r) => r.id === selected)?.manifest.base_link}`
      : selected;
  const gizmo: GizmoTarget | null =
    handled && handleFrame !== null && selected !== null
      ? {
          frame: handleFrame,
          parent: handled.parent,
          placement: handled.parent_se3_self,
          onPreview: (pose) =>
            studio.preview(`move ${selected}`, (d) => ({
              ...d,
              scene: isRobot(selected)
                ? {
                    ...d.scene,
                    robots: (d.scene.robots ?? []).map((r) =>
                      r.id === selected ? { ...r, parent_se3_self: pose } : r,
                    ),
                  }
                : updateEntity(d.scene, selected, (e) => ({ ...e, parent_se3_self: pose })),
            })),
          onCommit: studio.commit,
          onCancel: studio.cancel,
        }
      : null;

  const left = !current ? null : cellBuilder.active ? (
    cellBuilder.left
  ) : tree ? (
    <Navigator
      tree={tree}
      selected={selected}
      onSelect={onSelect}
      onAdd={add}
      robots={library}
      onAddRobot={addRobot}
      onBrowseRobots={() => setRobotLibrary(true)}
      onRemove={removable ? remove : null}
      onNewCell={() => {
        onSelect(null);
        cellBuilder.start();
      }}
    />
  ) : null;
  const right = !current ? null : cellBuilder.active ? (
    cellBuilder.right
  ) : (
    <Inspector
      current={current}
      preview={editing}
      poses={poses}
      selected={selected}
      failure={failure}
      playhead={playhead}
      live={live}
      onEdit={studio.edit}
      onRenamed={onSelect}
    />
  );
  const dialog = (
    <RobotLibrary
      open={robotLibrary}
      onOpenChange={setRobotLibrary}
      robots={library}
      load={loadLibraryRobot}
      attachTo={parent}
      replace={isRobot(selected) ? selected : null}
      onAdd={addRobot}
      onReplace={replaceRobot}
    />
  );
  return { left, right, gizmo, dialog };
}
