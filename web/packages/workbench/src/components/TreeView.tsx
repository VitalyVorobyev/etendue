/*
 * A hierarchy you can walk with the keyboard.
 *
 * Built for the navigator column of a studio — a robot cell's frame tree, world → robots →
 * links → rigs → cameras — where the tree is data the app already has, the icons are the
 * app's own, and selection is shared with a viewport that can also pick things. So the tree
 * is data-driven, selection is controlled, and selecting something hidden opens the way to it.
 *
 * Rendered flat: one `treeitem` per visible row, carrying its level, position and set size
 * (the WAI-ARIA tree pattern's "flat" form). The focus ring then belongs to one row rather
 * than to a subtree, and the keyboard contract is a function over the same list the DOM is
 * — see `treeModel.ts`.
 */

import { ChevronRight } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { KeyboardEvent, MouseEvent } from "react";

import { byDensity, cn, useDensity } from "@vitavision/ui";

import {
  type TreeNode,
  type TreeRow,
  ancestorIds,
  parentIds,
  treeKeyAction,
  visibleRows,
} from "./treeModel";

/** Props of `TreeView`. */
export interface TreeViewProps {
  /** The roots of the tree. */
  nodes: readonly TreeNode[];
  /** The selected node's id, controlled; `null` or absent for none. */
  selectedId?: string | null | undefined;
  /** Called with the node's id (and the node) on click, Enter or Space. Not called for a disabled node. */
  onSelect?: ((id: string, node: TreeNode) => void) | undefined;
  /** The expanded parents' ids, controlled. Pair with `onExpandedChange`. */
  expanded?: readonly string[] | undefined;
  /** The initially expanded parents' ids, when uncontrolled. */
  defaultExpanded?: readonly string[] | undefined;
  /** Start with every parent expanded, when uncontrolled (overrides `defaultExpanded`). */
  defaultExpandAll?: boolean | undefined;
  /** Called with the new set of expanded ids whenever a parent opens or closes. */
  onExpandedChange?: ((expanded: string[]) => void) | undefined;
  /** Names the tree for assistive technology. */
  "aria-label": string;
  /** Merged with the tree's own classes through `cn`. */
  className?: string | undefined;
}

/**
 * A data-driven tree (the WAI-ARIA tree pattern) with single selection.
 *
 * Keyboard: the tree is one Tab stop (roving `tabindex`). ↓/↑ move between visible rows, →
 * expands a parent or enters it, ← collapses a parent or goes up to its parent, Home/End jump
 * to the ends, Enter/Space select, `*` expands the focused row's siblings, and a letter jumps
 * to the next row starting with it. Clicking a row selects it; clicking its chevron toggles it.
 *
 * Selection is controlled (`selectedId`/`onSelect`). When `selectedId` changes to a node
 * inside a collapsed parent, its ancestors are expanded (through `onExpandedChange` when
 * `expanded` is controlled) and the row is scrolled into view — so a pick in a viewport can
 * drive the tree.
 *
 * Each row is a `treeitem` with `aria-level`, `aria-posinset`, `aria-setsize`,
 * `aria-selected` and, for parents, `aria-expanded`; also `data-selected` and, for parents,
 * `data-state` (`open`/`closed`). Row height follows the density in force.
 */
export function TreeView({
  nodes,
  selectedId = null,
  onSelect,
  expanded: controlledExpanded,
  defaultExpanded,
  defaultExpandAll = false,
  onExpandedChange,
  "aria-label": ariaLabel,
  className,
}: TreeViewProps) {
  const density = useDensity();
  const [internalExpanded, setInternalExpanded] = useState<readonly string[]>(() =>
    defaultExpandAll ? parentIds(nodes) : (defaultExpanded ?? []),
  );
  const expandedIds = controlledExpanded ?? internalExpanded;
  const expandedSet = new Set(expandedIds);
  const rows = visibleRows(nodes, expandedSet);
  const [focusedId, setFocusedId] = useState<string | null>(null);
  const rowElementsRef = useRef(new Map<string, HTMLDivElement>());

  const setExpanded = (next: readonly string[]) => {
    if (controlledExpanded === undefined) setInternalExpanded(next);
    onExpandedChange?.([...next]);
  };

  const toggle = (id: string, open: boolean) => {
    if (open === expandedSet.has(id)) return;
    setExpanded(open ? [...expandedIds, id] : expandedIds.filter((other) => other !== id));
  };

  // Reveal a selection made elsewhere: open its ancestors and bring its row into view.
  const latestRef = useRef({ nodes, expandedIds, setExpanded });
  useEffect(() => {
    latestRef.current = { nodes, expandedIds, setExpanded };
  });
  useEffect(() => {
    if (selectedId === null) return;
    const { nodes: tree, expandedIds: open, setExpanded: update } = latestRef.current;
    const missing = ancestorIds(tree, selectedId).filter((id) => !open.includes(id));
    if (missing.length > 0) update([...open, ...missing]);
  }, [selectedId]);
  useEffect(() => {
    if (selectedId === null) return;
    const element = rowElementsRef.current.get(selectedId);
    // `scrollIntoView` is absent in some test DOMs.
    if (typeof element?.scrollIntoView === "function") element.scrollIntoView({ block: "nearest" });
  }, [selectedId, rows.length]);

  const isVisible = (id: string | null) => id !== null && rows.some((row) => row.node.id === id);
  const tabStop = isVisible(focusedId) ? focusedId : isVisible(selectedId) ? selectedId : (rows[0]?.node.id ?? null);

  const focusRow = (id: string) => {
    setFocusedId(id);
    rowElementsRef.current.get(id)?.focus();
  };

  const select = (row: TreeRow) => {
    if (!row.node.disabled) onSelect?.(row.node.id, row.node);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.altKey || event.ctrlKey || event.metaKey) return;
    const id = (event.target as HTMLElement).dataset["treeId"];
    if (id === undefined) return;
    const action = treeKeyAction(rows, id, event.key);
    if (action === null) return;
    event.preventDefault();
    switch (action.type) {
      case "focus":
        focusRow(action.id);
        break;
      case "expand":
        toggle(action.id, true);
        break;
      case "collapse":
        toggle(action.id, false);
        break;
      case "select": {
        const row = rows.find((candidate) => candidate.node.id === action.id);
        if (row) select(row);
        break;
      }
      case "expand-siblings":
        setExpanded([...expandedIds, ...action.ids]);
        break;
    }
  };

  const onChevron = (event: MouseEvent, row: TreeRow) => {
    event.stopPropagation();
    toggle(row.node.id, !row.isExpanded);
    focusRow(row.node.id);
  };

  return (
    <div
      role="tree"
      aria-label={ariaLabel}
      onKeyDown={onKeyDown}
      className={cn("flex flex-col py-1 select-none", className)}
    >
      {rows.map((row) => {
        const { node } = row;
        const selected = node.id === selectedId;
        return (
          <div
            key={node.id}
            ref={(element) => {
              if (element) rowElementsRef.current.set(node.id, element);
              else rowElementsRef.current.delete(node.id);
            }}
            role="treeitem"
            aria-level={row.level}
            aria-posinset={row.posInSet}
            aria-setsize={row.setSize}
            aria-selected={selected}
            aria-expanded={row.isParent ? row.isExpanded : undefined}
            aria-disabled={node.disabled || undefined}
            tabIndex={node.id === tabStop ? 0 : -1}
            data-tree-id={node.id}
            data-selected={selected ? "" : undefined}
            data-state={row.isParent ? (row.isExpanded ? "open" : "closed") : undefined}
            onFocus={(event) => {
              if (event.target === event.currentTarget) setFocusedId(node.id);
            }}
            onClick={() => {
              focusRow(node.id);
              select(row);
            }}
            style={{ paddingLeft: `calc(${row.level - 1} * 0.875rem + 0.25rem)` }}
            className={cn(
              "mx-1 flex shrink-0 cursor-default items-center gap-1.5 rounded-control pr-2 text-fg transition-colors",
              "outline-none focus-visible:outline-2 focus-visible:outline-offset-0 focus-visible:outline-signal",
              byDensity(density, "h-7 text-sm", "h-6 text-xs"),
              selected ? "bg-signal/12" : "hover:bg-raised",
              node.disabled && "text-fg-subtle",
            )}
          >
            {row.isParent ? (
              <span
                aria-hidden
                onClick={(event) => onChevron(event, row)}
                className="grid size-4 shrink-0 place-items-center rounded-sm text-fg-subtle hover:text-fg"
              >
                <ChevronRight
                  className={cn("size-3.5 transition-transform", row.isExpanded && "rotate-90")}
                />
              </span>
            ) : (
              <span aria-hidden className="size-4 shrink-0" />
            )}
            {node.icon !== undefined && (
              <span
                aria-hidden
                className={cn(
                  "grid shrink-0 place-items-center [&>svg]:size-3.5",
                  selected ? "text-signal" : "text-fg-muted",
                )}
              >
                {node.icon}
              </span>
            )}
            <span className="min-w-0 flex-1 truncate">{node.label}</span>
            {node.meta !== undefined && (
              <span className="shrink-0 font-mono text-[11px] text-fg-subtle">{node.meta}</span>
            )}
          </div>
        );
      })}
    </div>
  );
}
