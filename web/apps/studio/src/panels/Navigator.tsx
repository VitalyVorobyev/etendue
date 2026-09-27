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
  Zap,
} from "lucide-react";
import { useMemo } from "react";

import type { NodeKind, SceneNode } from "../scene/model";

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
    label:
      node.kind === "links" ? "links" : node.kind === "link" ? node.id.slice(node.id.indexOf("/") + 1) : node.id,
    icon: <Icon className="size-3.5" aria-hidden />,
    meta: node.kind === "links" ? node.children.length : node.kind,
    ...(node.kind === "links" ? { disabled: true } : {}),
    ...(node.children.length > 0 ? { children: node.children.map(toTreeNode) } : {}),
  };
}

/** The scene's frame tree; selecting a node selects that thing everywhere. */
export function Navigator({
  tree,
  selected,
  onSelect,
}: {
  tree: SceneNode;
  selected: string | null;
  onSelect: (id: string) => void;
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
    <TreeView
      key={tree.id + expanded.length}
      nodes={nodes}
      selectedId={selected}
      onSelect={onSelect}
      defaultExpanded={expanded}
      aria-label="Scene frames"
      className="p-1"
    />
  );
}
