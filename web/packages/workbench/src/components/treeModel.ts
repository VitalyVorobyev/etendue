/*
 * The model behind `TreeView`: which rows are visible, where each sits in its set, and what
 * each key does (the WAI-ARIA tree pattern).
 *
 * Kept out of the component so the keyboard contract — the part a tree most often gets
 * subtly wrong — is tested as a function from (rows, focus, key) to an action.
 */

import type { ReactNode } from "react";

/** One node of a `TreeView`. */
export interface TreeNode {
  /** Identifies the node to `selectedId`, `onSelect` and `expanded`. Unique in the tree. */
  id: string;
  /** The node's name: shown, read out, and matched by type-ahead. */
  label: string;
  /** A leading icon (decorative) — typically a lucide icon at `size-3.5`. */
  icon?: ReactNode;
  /** Child nodes. A node with a (possibly empty) array is a parent; without, a leaf. */
  children?: readonly TreeNode[] | undefined;
  /** A quiet trailing annotation — a type, a count, a frame name. Shown in mono. */
  meta?: ReactNode;
  /** Shown and focusable, but not selectable. */
  disabled?: boolean | undefined;
}

/** A node as it is laid out: one visible row of the tree. */
export interface TreeRow {
  /** The node. */
  node: TreeNode;
  /** Depth, from 1 at the root (`aria-level`). */
  level: number;
  /** The parent's id, or `null` at the root. */
  parentId: string | null;
  /** 1-based position among its siblings (`aria-posinset`). */
  posInSet: number;
  /** How many siblings, itself included (`aria-setsize`). */
  setSize: number;
  /** Whether the node has a children array. */
  isParent: boolean;
  /** Whether the node is a parent and expanded. */
  isExpanded: boolean;
}

/**
 * The rows a reader can see: every root, and the children of every expanded parent, in
 * document order.
 *
 * @param nodes - The roots.
 * @param expanded - The ids of the expanded parents.
 * @returns The visible rows, in order.
 */
export function visibleRows(nodes: readonly TreeNode[], expanded: ReadonlySet<string>): TreeRow[] {
  const rows: TreeRow[] = [];
  const walk = (siblings: readonly TreeNode[], level: number, parentId: string | null) => {
    siblings.forEach((node, index) => {
      const isParent = node.children !== undefined;
      const isExpanded = isParent && expanded.has(node.id);
      rows.push({
        node,
        level,
        parentId,
        posInSet: index + 1,
        setSize: siblings.length,
        isParent,
        isExpanded,
      });
      if (isExpanded && node.children) walk(node.children, level + 1, node.id);
    });
  };
  walk(nodes, 1, null);
  return rows;
}

/**
 * Every parent's id — the expanded set for "everything open".
 *
 * @param nodes - The roots.
 * @returns The ids of every node with a children array, at any depth.
 */
export function parentIds(nodes: readonly TreeNode[]): string[] {
  const ids: string[] = [];
  const walk = (siblings: readonly TreeNode[]) => {
    for (const node of siblings) {
      if (node.children === undefined) continue;
      ids.push(node.id);
      walk(node.children);
    }
  };
  walk(nodes);
  return ids;
}

/**
 * The ancestors of a node, root first — what must be expanded for it to be visible.
 *
 * @param nodes - The roots.
 * @param id - The node.
 * @returns The ancestor ids, root first; empty for a root or an id not in the tree.
 */
export function ancestorIds(nodes: readonly TreeNode[], id: string): string[] {
  const walk = (siblings: readonly TreeNode[], path: string[]): string[] | null => {
    for (const node of siblings) {
      if (node.id === id) return path;
      if (node.children) {
        const found = walk(node.children, [...path, node.id]);
        if (found) return found;
      }
    }
    return null;
  };
  return walk(nodes, []) ?? [];
}

/** What a key press on a tree asks for. */
export type TreeKeyAction =
  | { type: "focus"; id: string }
  | { type: "expand"; id: string }
  | { type: "collapse"; id: string }
  | { type: "select"; id: string }
  | { type: "expand-siblings"; ids: string[] };

/**
 * The tree's keyboard (the WAI-ARIA tree pattern), as a pure function.
 *
 * ↓/↑ move to the next/previous visible row; → expands a closed parent, or moves into an
 * open one's first child; ← collapses an open parent, or moves to the parent; Home/End go to
 * the first/last visible row; Enter and Space select; `*` expands every sibling of the
 * focused row; a printable character moves to the next row whose label starts with it
 * (type-ahead, wrapping).
 *
 * @param rows - The visible rows, from `visibleRows`.
 * @param focusedId - The row with focus.
 * @param key - `KeyboardEvent.key`.
 * @returns The action, or `null` when the key does nothing here.
 */
export function treeKeyAction(rows: readonly TreeRow[], focusedId: string, key: string): TreeKeyAction | null {
  const index = rows.findIndex((row) => row.node.id === focusedId);
  const row = rows[index];
  if (row === undefined) return null;
  const focus = (target: TreeRow | undefined): TreeKeyAction | null =>
    target === undefined || target === row ? null : { type: "focus", id: target.node.id };

  switch (key) {
    case "ArrowDown":
      return focus(rows[index + 1]);
    case "ArrowUp":
      return focus(rows[index - 1]);
    case "Home":
      return focus(rows[0]);
    case "End":
      return focus(rows[rows.length - 1]);
    case "ArrowRight":
      if (!row.isParent) return null;
      if (!row.isExpanded) return { type: "expand", id: row.node.id };
      // An open parent's first child is the next row, if it has one.
      return rows[index + 1]?.parentId === row.node.id ? focus(rows[index + 1]) : null;
    case "ArrowLeft":
      if (row.isExpanded) return { type: "collapse", id: row.node.id };
      return row.parentId === null
        ? null
        : focus(rows.find((candidate) => candidate.node.id === row.parentId));
    case "Enter":
    case " ":
      return row.node.disabled ? null : { type: "select", id: row.node.id };
    case "*": {
      const ids = rows
        .filter((candidate) => candidate.parentId === row.parentId && candidate.isParent && !candidate.isExpanded)
        .map((candidate) => candidate.node.id);
      return ids.length === 0 ? null : { type: "expand-siblings", ids };
    }
    default:
      return typeAhead(rows, index, key);
  }
}

/** The next row after `from` (wrapping) whose label starts with `key`, for a single printable character. */
function typeAhead(rows: readonly TreeRow[], from: number, key: string): TreeKeyAction | null {
  if (key.length !== 1 || key.trim() === "") return null;
  const needle = key.toLocaleLowerCase();
  for (let offset = 1; offset <= rows.length; offset += 1) {
    const candidate = rows[(from + offset) % rows.length];
    if (candidate?.node.label.toLocaleLowerCase().startsWith(needle)) {
      return offset === rows.length ? null : { type: "focus", id: candidate.node.id };
    }
  }
  return null;
}
