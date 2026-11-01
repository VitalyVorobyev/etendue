import type { LibraryItem } from "@etendue/wasm";
import { describe, expect, it, vi } from "vitest";

import type { Bank, Project } from "../io/project";

// The kernel's hash is tested in Rust; a stand-in: the item's JSON without origin and version.
vi.mock("@etendue/wasm", () => ({
  libraryItemHash: (item: { origin?: unknown; version?: number }) => {
    const { origin: _origin, version: _version, ...rest } = item;
    return JSON.stringify(rest);
  },
}));

const { byKind, deleteBlocker, entries, filterEntries, freeId, idFrom, reselect, NO_FILTER } = await import("./browse");
const { libraryItemHash } = await import("@etendue/wasm");

const lens = (id: string, name: string, extra: Partial<LibraryItem> = {}): LibraryItem => ({
  version: 2,
  id,
  name,
  lens: { focal_length_m: 0.025, min_f_number: 1.4, mount: "c_mount" },
  ...extra,
});
const sensor = (id: string, name: string): LibraryItem => ({
  version: 2,
  id,
  name,
  sensor: { pixel_pitch_m: 3e-6, resolution: [100, 100] },
});

function layers() {
  const builtin = new Map<string, LibraryItem>([
    ["lenses/l25", lens("l25", "Kowa 25")],
    ["sensors/imx174", sensor("imx174", "IMX174")],
  ]);
  const bankLens = lens("l16", "My 16", { source: { vendor: "Edmund", part_number: "86-571" } });
  const bank = {
    library: new Map<string, LibraryItem>([
      ["lenses/l16", bankLens],
      ["lenses/l25", lens("l25", "Kowa 25, measured")],
    ]),
  } as Bank;
  const copy = {
    ...bankLens,
    origin: { layer: "bank" as const, reference: "lenses/l16", sha256: libraryItemHash(bankLens) },
  };
  const project = {
    own: new Map<string, LibraryItem>([
      ["lenses/l16", copy],
      ["lenses/mine", lens("mine", "A project lens")],
    ]),
    manifest: { version: 1, name: "p", defaults: { camera: null, laser: null, target: null } },
  } as unknown as Project;
  return { builtin, bank, project };
}

describe("the library's entries", () => {
  it("lists every layer's items, with what hides what", () => {
    const { builtin, bank, project } = layers();
    const all = entries(project, bank, builtin);
    // Kinds in the library's order (sensors first), then names, then layers.
    expect(all.map((e) => e.key)).toEqual([
      "builtin:sensors/imx174",
      "project:lenses/mine",
      "builtin:lenses/l25",
      "bank:lenses/l25",
      "project:lenses/l16",
      "bank:lenses/l16",
    ]);
    const by = new Map(all.map((e) => [e.key, e]));
    expect(by.get("project:lenses/l16")).toMatchObject({ hides: ["bank"], hiddenBy: null, stale: false });
    expect(by.get("bank:lenses/l16")).toMatchObject({ hides: [], hiddenBy: "project" });
    expect(by.get("bank:lenses/l25")).toMatchObject({ hides: ["builtin"], hiddenBy: null });
    expect(by.get("builtin:lenses/l25")).toMatchObject({ hides: [], hiddenBy: "bank" });
  });

  it("marks a project copy stale when its source changed", () => {
    const { builtin, bank, project } = layers();
    const changed = { ...bank, library: new Map(bank.library) };
    changed.library.set("lenses/l16", lens("l16", "My 16, remeasured"));
    const stale = entries(project, changed, builtin).find((e) => e.key === "project:lenses/l16");
    expect(stale?.stale).toBe(true);
    // A source that is gone is not a change.
    const gone = { ...bank, library: new Map() };
    expect(entries(project, gone, builtin).find((e) => e.key === "project:lenses/l16")?.stale).toBe(false);
  });

  it("treats the bundled examples as the built-in layer", () => {
    const { builtin, bank } = layers();
    const examples = { own: builtin, manifest: { version: 1, name: "examples" } } as unknown as Project;
    const all = entries(examples, bank, builtin);
    expect(all.filter((e) => e.layer === "project")).toEqual([]);
    expect(all.filter((e) => e.layer === "builtin").map((e) => e.ref)).toEqual(["sensors/imx174", "lenses/l25"]);
    expect(entries(null, null, null)).toEqual([]);
  });

  it("filters by text, kind and layer, and groups by kind", () => {
    const { builtin, bank, project } = layers();
    const all = entries(project, bank, builtin);
    expect(filterEntries(all, NO_FILTER)).toHaveLength(all.length);
    expect(filterEntries(all, { ...NO_FILTER, kind: "sensor" }).map((e) => e.ref)).toEqual(["sensors/imx174"]);
    expect(filterEntries(all, { ...NO_FILTER, layer: "bank" }).map((e) => e.key)).toEqual([
      "bank:lenses/l25",
      "bank:lenses/l16",
    ]);
    // Words match the name, id and source, in any order, ignoring case.
    expect(filterEntries(all, { ...NO_FILTER, text: "edmund 16" }).map((e) => e.key)).toEqual([
      "project:lenses/l16",
      "bank:lenses/l16",
    ]);
    expect(filterEntries(all, { ...NO_FILTER, text: "86-571", layer: "project" }).map((e) => e.key)).toEqual([
      "project:lenses/l16",
    ]);
    expect(filterEntries(all, { ...NO_FILTER, text: "nothing like it" })).toEqual([]);
    expect(byKind(all).map(([kind, es]) => [kind, es.length])).toEqual([
      ["sensor", 1],
      ["lens", 5],
    ]);
  });
});

describe("library bookkeeping", () => {
  it("makes ids from names, and free ones", () => {
    expect(idFrom("  Kowa LM25HC (25 mm) ")).toBe("kowa_lm25hc_25_mm");
    expect(idFrom("***")).toBe("");
    const taken = new Map([
      ["lenses/l25", 1],
      ["lenses/l25_2", 1],
    ]);
    expect(freeId("l25", "lenses", taken)).toBe("l25_3");
    expect(freeId("l16", "lenses", taken)).toBe("l16");
    expect(freeId("", "lenses", taken)).toBe("item");
  });

  it("keeps a project default resolving when it is deleted", () => {
    const { builtin, bank, project } = layers();
    const withDefault = {
      ...project,
      manifest: { ...project.manifest, defaults: { camera: null, laser: "lenses/mine", target: "lenses/l16" } },
    } as Project;
    const all = entries(withDefault, bank, builtin);
    expect(deleteBlocker(withDefault, "lenses/mine", all)).toMatch(/default laser/);
    // The bank has the same reference: the default resolves to it once the project's goes.
    expect(deleteBlocker(withDefault, "lenses/l16", all)).toBeNull();
    expect(deleteBlocker(project, "lenses/mine", all)).toBeNull();
  });

  it("reselects an item after it moved or went", () => {
    const { builtin, bank, project } = layers();
    const all = entries(project, bank, builtin);
    expect(reselect(all, "bank:lenses/l25")?.key).toBe("bank:lenses/l25");
    // The project's copy deleted: the bank's item shows.
    const without = entries({ ...project, own: new Map([...project.own].slice(1)) }, bank, builtin);
    expect(reselect(without, "project:lenses/l16")?.key).toBe("bank:lenses/l16");
    expect(reselect(all, "project:lenses/none")).toBeNull();
    expect(reselect(all, null)).toBeNull();
  });
});
