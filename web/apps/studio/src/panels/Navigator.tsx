import { Button, DropdownMenu, MenuItem, MenuLabel, MenuSeparator } from "@vitavision/ui";
import { TreeView, type TreeNode } from "@vitavision/workbench";
import {
  Axis3d,
  Bot,
  Boxes,
  Camera,
  Globe,
  Grid3x3,
  Lightbulb,
  Link2,
  type LucideIcon,
  Package,
  Plus,
  Trash2,
  Zap,
} from "lucide-react";
import { useMemo } from "react";
import type { LibraryRobot } from "virtual:etendue-robots";

import type { EntityList } from "../scene/edit";
import type { NodeKind, SceneNode } from "../scene/model";

/** What “Add” offers, in order. */
const ADDABLE: { list: EntityList; label: string; icon: LucideIcon }[] = [
  { list: "frames", label: "Frame", icon: Axis3d },
  { list: "rigs", label: "Rig", icon: Boxes },
  { list: "cameras", label: "Camera", icon: Camera },
  { list: "lasers", label: "Laser", icon: Zap },
  { list: "targets", label: "Target", icon: Grid3x3 },
  { list: "lights", label: "Light", icon: Lightbulb },
];

const ICONS: Record<NodeKind, LucideIcon> = {
  world: Globe,
  frame: Axis3d,
  robot: Bot,
  links: Link2,
  link: Link2,
  rig: Boxes,
  camera: Camera,
  laser: Zap,
  light: Lightbulb,
  target: Grid3x3,
  part: Package,
};

function toTreeNode(node: SceneNode): TreeNode {
  const Icon = ICONS[node.kind];
  return {
    id: node.id,
    label: node.kind === "links" ? "links" : node.kind === "link" ? node.id.slice(node.id.indexOf("/") + 1) : node.id,
    icon: <Icon className="size-3.5" aria-hidden />,
    meta: node.kind === "links" ? node.children.length : node.kind,
    ...(node.kind === "links" ? { disabled: true } : {}),
    ...(node.children.length > 0 ? { children: node.children.map(toTreeNode) } : {}),
  };
}

/**
 * The scene's frame tree; selecting a node selects that thing everywhere. Above it, Add (attached
 * to the selected frame, else the world) and Remove (the selected entity with what hangs on it).
 */
export function Navigator({
  tree,
  selected,
  onSelect,
  onAdd,
  robots,
  onAddRobot,
  onRemove,
}: {
  tree: SceneNode;
  selected: string | null;
  onSelect: (id: string) => void;
  /** Add an entity of `list`; `null` when the scene cannot be edited. */
  onAdd: ((list: EntityList) => void) | null;
  /** The robot library. */
  robots: readonly LibraryRobot[];
  onAddRobot: (robot: LibraryRobot) => void;
  /** Remove the selection; `null` when it is not removable. */
  onRemove: (() => void) | null;
}) {
  const nodes = useMemo(() => [toTreeNode(tree)], [tree]);
  // Everything open except the groups of bare robot links, which are long and rarely needed.
  const expanded = useMemo(() => {
    const ids: string[] = [];
    const visit = (n: SceneNode) => {
      if (n.kind !== "links") ids.push(n.id);
      n.children.forEach(visit);
    };
    visit(tree);
    return ids;
  }, [tree]);
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-9 shrink-0 items-center gap-1 border-b border-line px-2">
        <span className="flex-1 text-xs font-medium text-fg-muted">Scene</span>
        <DropdownMenu
          align="end"
          trigger={
            <Button
              variant="ghost"
              size="sm"
              icon={<Plus className="size-4" />}
              aria-label="Add to the scene"
              title={`Add, attached to ${selected ?? "world"}`}
              disabled={onAdd === null}
            />
          }
        >
          <MenuLabel>Attach to {selected ?? "world"}</MenuLabel>
          {ADDABLE.map(({ list, label, icon: Icon }) => (
            <MenuItem key={list} icon={<Icon className="size-4" />} onSelect={() => onAdd?.(list)}>
              {label}
            </MenuItem>
          ))}
          {robots.length > 0 && (
            <>
              <MenuSeparator />
              <MenuLabel>Robot</MenuLabel>
              {robots.map((r) => (
                <MenuItem key={r.manifest} icon={<Bot className="size-4" />} onSelect={() => onAddRobot(r)}>
                  {r.local ? `${r.name} (local only)` : r.name}
                </MenuItem>
              ))}
            </>
          )}
        </DropdownMenu>
        <Button
          variant="ghost"
          size="sm"
          icon={<Trash2 className="size-4" />}
          aria-label={selected ? `Remove ${selected}` : "Remove"}
          title={onRemove ? `Remove ${selected} and what is attached to it` : "Select an entity to remove it"}
          disabled={onRemove === null}
          onClick={() => onRemove?.()}
        />
      </div>
      <div className="min-h-0 flex-1 overflow-auto">
        <TreeView
          key={tree.id + expanded.length}
          nodes={nodes}
          selectedId={selected}
          onSelect={onSelect}
          defaultExpanded={expanded}
          aria-label="Scene frames"
          className="p-1"
        />
      </div>
    </div>
  );
}
