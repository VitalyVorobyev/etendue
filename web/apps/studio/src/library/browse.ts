/**
 * What the Library workspace lists: every item of every layer the open project sees (its own,
 * the user's bank, the built-ins), with which one the project resolves when an id is in several
 * layers, and whether a project copy's source has changed since it was copied.
 */

import type { LibraryItem } from "@etendue/wasm";

import { type Bank, LIBRARY_FOLDERS, type Layer, type LibraryKind, type Project, isStale, kindOf } from "../io/project";

/** One item of one layer. */
export interface Entry {
  /** Unique among the entries: `<layer>:<ref>`. */
  key: string;
  layer: Layer;
  /** `<folder>/<id>`. */
  ref: string;
  kind: LibraryKind;
  item: LibraryItem;
  /** The lower layers that have this reference too: the project sees this entry, not theirs. */
  hides: Layer[];
  /** The higher layer whose item the project sees instead of this one, if any. */
  hiddenBy: Layer | null;
  /** A project copy whose source (in the bank or the built-ins) has changed since it was copied. */
  stale: boolean;
}

/** The layers, highest first. */
export const LAYERS: readonly Layer[] = ["project", "bank", "builtin"];

/** A layer's name, for people. */
export const LAYER_LABEL: Record<Layer, string> = { project: "project", bank: "bank", builtin: "built-in" };

/** The kinds, in the library's order. */
export const KINDS = Object.keys(LIBRARY_FOLDERS) as LibraryKind[];

/** A kind's name, for people, singular and plural. */
export const KIND_LABEL: Record<LibraryKind, [string, string]> = {
  sensor: ["Sensor", "Sensors"],
  lens: ["Lens", "Lenses"],
  laser: ["Laser", "Lasers"],
  target: ["Target", "Targets"],
  camera: ["Camera", "Cameras"],
  head: ["Head", "Heads"],
  rig: ["Rig", "Rigs"],
  mount: ["Mount", "Mounts"],
  procedure: ["Procedure", "Procedures"],
};

/**
 * Every entry of `project`'s own library, `bank` and `builtin`, in kind, then name, then layer
 * order. The reference project (the bundled examples) is the built-in layer itself: its own
 * items are `builtin` (`project.own === builtin`), and it has no project layer.
 */
export function entries(
  project: Project | null,
  bank: Bank | null,
  builtin: ReadonlyMap<string, LibraryItem> | null,
): Entry[] {
  const isBuiltin = project !== null && builtin !== null && project.own === builtin;
  const stack: [Layer, ReadonlyMap<string, LibraryItem>][] = [];
  if (project && !isBuiltin) stack.push(["project", project.own]);
  if (bank) stack.push(["bank", bank.library]);
  if (builtin) stack.push(["builtin", builtin]);
  const sourcesOf = (layer: "bank" | "builtin") => (layer === "bank" ? bank?.library : builtin) ?? new Map();
  const out: Entry[] = [];
  stack.forEach(([layer, items], i) => {
    for (const [ref, item] of items) {
      const kind = kindOf(item);
      if (!kind) continue;
      const above = stack.slice(0, i).find(([, m]) => m.has(ref));
      const origin = layer === "project" ? item.origin : null;
      out.push({
        key: `${layer}:${ref}`,
        layer,
        ref,
        kind,
        item,
        hides: stack.slice(i + 1).flatMap(([l, m]) => (m.has(ref) ? [l] : [])),
        hiddenBy: above ? above[0] : null,
        stale: project !== null && origin != null && isStale(project, sourcesOf(origin.layer), ref),
      });
    }
  });
  const order = (e: Entry) => KINDS.indexOf(e.kind);
  return out.sort(
    (a, b) =>
      order(a) - order(b) ||
      a.item.name.localeCompare(b.item.name) ||
      a.ref.localeCompare(b.ref) ||
      LAYERS.indexOf(a.layer) - LAYERS.indexOf(b.layer),
  );
}

/** What the list shows. */
export interface Filter {
  /** Matched, ignoring case, against the name, id, description and source. */
  text: string;
  kind: LibraryKind | "all";
  layer: Layer | "all";
}

export const NO_FILTER: Filter = { text: "", kind: "all", layer: "all" };

/** The entries `filter` keeps. */
export function filterEntries(all: readonly Entry[], filter: Filter): Entry[] {
  const words = filter.text.toLowerCase().split(/\s+/).filter(Boolean);
  return all.filter((e) => {
    if (filter.kind !== "all" && e.kind !== filter.kind) return false;
    if (filter.layer !== "all" && e.layer !== filter.layer) return false;
    if (words.length === 0) return true;
    const { name, id, description, source } = e.item;
    const hay = [name, id, description, source?.vendor, source?.part_number, source?.notes]
      .filter((s): s is string => typeof s === "string")
      .join(" ")
      .toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}

/** `entries` grouped by kind, in the library's kind order (kinds without entries left out). */
export function byKind(all: readonly Entry[]): [LibraryKind, Entry[]][] {
  return KINDS.map((k): [LibraryKind, Entry[]] => [k, all.filter((e) => e.kind === k)]).filter(([, es]) => es.length);
}

/** `text` as an id: letters, digits, `_` and `-`, with spaces and other characters as `_`. */
export function idFrom(text: string): string {
  return text
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, "_")
    .replace(/^_+|_+$/g, "");
}

/** `base` (as an id), or `base_2`, `base_3`, … : the first that `folder` does not have among `taken`. */
export function freeId(base: string, folder: string, taken: ReadonlyMap<string, unknown>): string {
  const id = idFrom(base) || "item";
  if (!taken.has(`${folder}/${id}`)) return id;
  for (let n = 2; ; n++) if (!taken.has(`${folder}/${id}_${n}`)) return `${id}_${n}`;
}

/**
 * Why the project's own item at `ref` cannot be deleted, or `null`: a project default must keep
 * resolving, so it can go only when a lower layer has the same reference.
 */
export function deleteBlocker(project: Project, ref: string, lower: readonly Entry[]): string | null {
  const defaults = project.manifest.defaults ?? {};
  const roles = (Object.keys(defaults) as (keyof typeof defaults)[]).filter((k) => defaults[k] === ref);
  if (roles.length === 0) return null;
  if (lower.some((e) => e.ref === ref && e.layer !== "project")) return null;
  return `It is the project's default ${roles.join(" and ")}: new ${roles.join(" and ")}s are made from it.`;
}

/**
 * The entry to select after `entries` changed: `key` if it is still there; else the entry of the
 * same reference in another layer (an item renamed or deleted shows its lower copy); else none.
 */
export function reselect(all: readonly Entry[], key: string | null): Entry | null {
  if (key === null) return null;
  const same = all.find((e) => e.key === key);
  if (same) return same;
  const ref = key.slice(key.indexOf(":") + 1);
  return all.find((e) => e.ref === ref && e.hiddenBy === null) ?? null;
}
