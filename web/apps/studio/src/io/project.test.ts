import { describe, expect, it, vi } from "vitest";

import type { FileSource } from "./source";

// The kernel's checks are tested in Rust; here only the reading.
vi.mock("@etendue/wasm", () => ({
  validateLibraryItem: (text: string) => {
    const item = JSON.parse(text) as { version: number };
    if (item.version !== 1 && item.version !== 2) throw new Error(`library item version ${item.version}`);
    return item;
  },
  // A stand-in hash: the item's JSON without origin and version (the kernel's is SHA-256).
  libraryItemHash: (item: { origin?: unknown; version?: number }) => {
    const { origin: _origin, version: _version, ...rest } = item;
    return JSON.stringify(rest);
  },
}));

const { defaultOf, itemsOf, loadProject } = await import("./project");

function memory(files: Record<string, string>): FileSource {
  return {
    name: "memory",
    has: (path) => path in files,
    readText: (path) => (path in files ? Promise.resolve(files[path]!) : Promise.reject(new Error(`${path}: missing`))),
    url: (path) => path,
    list: (dir) =>
      Promise.resolve(
        Object.keys(files)
          .filter((p) => p.startsWith(`${dir}/`) && !p.slice(dir.length + 1).includes("/"))
          .map((p) => p.slice(dir.length + 1))
          .sort(),
      ),
  };
}

const item = (id: string, name: string, part: string, version = 1) => JSON.stringify({ version, id, name, [part]: {} });

describe("projects", () => {
  it("reads cells, the library and the defaults, and reports what does not read", async () => {
    const source = memory({
      "p/etendue-project.json": JSON.stringify({
        version: 1,
        name: "p",
        cells: ["a", "b", "gone"],
        defaults: { camera: "cameras/cam1", target: "targets/missing" },
      }),
      "p/a/scene.json": JSON.stringify({ description: "first" }),
      "p/a/scenario.json": "{}",
      "p/b/scene.json": "{}",
      "p/library/cameras/cam1.json": item("cam1", "Zeta", "camera"),
      "p/library/cameras/cam2.json": item("cam2", "Alpha", "camera"),
      "p/library/cameras/misnamed.json": item("other", "X", "camera"),
      "p/library/lenses/new.json": item("new", "Future", "lens", 3),
    });
    const project = await loadProject(source, "p");
    expect(project.cells).toEqual([
      { id: "a", description: "first", scene: "p/a/scene.json", scenario: "p/a/scenario.json" },
      { id: "b", description: "", scene: "p/b/scene.json", scenario: null },
    ]);
    expect(itemsOf(project, "camera").map(([ref]) => ref)).toEqual(["cameras/cam2", "cameras/cam1"]);
    expect(defaultOf(project, "camera")?.id).toBe("cam1");
    expect(defaultOf(project, "target")).toBeUndefined();
    expect(project.problems.map((p) => p.path).sort()).toEqual([
      "p/gone/scene.json",
      "p/library/cameras/misnamed.json",
      "p/library/lenses/new.json",
    ]);
  });

  it("refuses a manifest of another version", async () => {
    const source = memory({ "etendue-project.json": JSON.stringify({ version: 2, name: "x" }) });
    await expect(loadProject(source, "")).rejects.toThrow(/project version 2/);
  });
});

describe("writing projects", () => {
  it("creates a project from a starter, adds a cell, saves a library item", async () => {
    const { addCell, createProject, saveLibraryItem } = await import("./project");
    const starterFiles = {
      "s/etendue-project.json": JSON.stringify({ version: 1, name: "s", cells: [], defaults: { camera: "cameras/c" } }),
      "s/library/cameras/c.json": item("c", "Camera", "camera"),
    };
    const starter = await loadProject(memory(starterFiles), "s");
    const written = new Map<string, string>();
    const write = (path: string, text: string) => {
      written.set(path, text);
      return Promise.resolve();
    };
    await createProject(write, "bench", starter);
    expect(JSON.parse(written.get("etendue-project.json")!)).toEqual({
      version: 1,
      name: "bench",
      cells: [],
      defaults: { camera: "cameras/c" },
    });
    // The project copies what its defaults name, with the copy's origin.
    expect(JSON.parse(written.get("library/cameras/c.json")!)).toMatchObject({
      id: "c",
      version: 2,
      camera: {},
      origin: { layer: "builtin", reference: "cameras/c" },
    });

    const files = Object.fromEntries(written);
    let project = await loadProject(memory(files), "", write, "bench");
    project = await addCell(
      project,
      "ring3",
      { version: 1, description: "a ring" },
      { version: 1, dt: 0.01, steps: [] },
    );
    expect(project.cells.map((c) => [c.id, c.description, c.scenario])).toEqual([
      ["ring3", "a ring", "ring3/scenario.json"],
    ]);
    expect((JSON.parse(written.get("etendue-project.json")!) as { cells: string[] }).cells).toEqual(["ring3"]);
    await expect(addCell(project, "ring3", {}, null)).rejects.toThrow(/already has a cell/);

    project = await saveLibraryItem(project, { version: 1, id: "l1", name: "Laser", laser: {} } as never);
    expect(written.has("library/lasers/l1.json")).toBe(true);
    expect(project.library.get("lasers/l1")?.name).toBe("Laser");

    const readOnly = { ...project, write: null };
    await expect(saveLibraryItem(readOnly, { version: 1, id: "x", name: "X", laser: {} } as never)).rejects.toThrow(
      /read-only/,
    );
  });
});

describe("layers (ADR 0010)", () => {
  it("resolves project, then bank, then built-in; copies an item into the project; notices a stale copy", async () => {
    const { isStale, loadBank, saveToBank, useInProject, withLayers } = await import("./project");
    const builtin = await loadProject(
      memory({
        "b/etendue-project.json": JSON.stringify({ version: 1, name: "builtin" }),
        "b/library/lenses/l25.json": item("l25", "Built-in 25", "lens"),
        "b/library/lenses/l12.json": item("l12", "Built-in 12", "lens"),
      }),
      "b",
    );
    const bankFiles: Record<string, string> = {
      "etendue-bank.json": JSON.stringify({ version: 1, name: "mine" }),
      "library/lenses/l25.json": item("l25", "My 25", "lens", 2),
      "library/sensors/s1.json": item("s1", "My sensor", "sensor", 2),
    };
    const bankWrites = new Map<string, string>();
    let bank = await loadBank(memory(bankFiles), (path, text) => {
      bankWrites.set(path, text);
      return Promise.resolve();
    });
    const projectWrites = new Map<string, string>();
    const own = await loadProject(
      memory({
        "etendue-project.json": JSON.stringify({ version: 1, name: "p" }),
        "library/lenses/l12.json": item("l12", "Project 12", "lens"),
      }),
      "",
      (path, text) => {
        projectWrites.set(path, text);
        return Promise.resolve();
      },
    );
    let project = withLayers(own, bank, builtin.own);
    expect(project.library.get("lenses/l12")?.name).toBe("Project 12");
    expect(project.layers.get("lenses/l12")).toEqual({ layer: "project", hides: ["builtin"] });
    expect(project.library.get("lenses/l25")?.name).toBe("My 25");
    expect(project.layers.get("lenses/l25")).toEqual({ layer: "bank", hides: ["builtin"] });
    expect(project.layers.get("sensors/s1")?.layer).toBe("bank");
    // The built-in project is its own layer.
    expect(withLayers(builtin, bank, builtin.own, true).layers.get("lenses/l12")).toEqual({
      layer: "builtin",
      hides: [],
    });

    project = await useInProject(project, "lenses/l25");
    const copy = JSON.parse(projectWrites.get("library/lenses/l25.json")!) as { origin: unknown; name: string };
    expect(copy).toMatchObject({ name: "My 25", origin: { layer: "bank", reference: "lenses/l25" } });
    expect(project.layers.get("lenses/l25")).toEqual({ layer: "project", hides: ["bank", "builtin"] });
    expect(isStale(project, bank.library, "lenses/l25")).toBe(false);
    // Editing the bank's item makes the project's copy stale; the copy itself is unchanged.
    bank = await saveToBank(bank, { ...bank.library.get("lenses/l25")!, name: "My 25, measured" });
    expect(JSON.parse(bankWrites.get("library/lenses/l25.json")!)).toMatchObject({
      version: 2,
      name: "My 25, measured",
    });
    expect(isStale(project, bank.library, "lenses/l25")).toBe(true);
    expect(withLayers(project, bank, builtin.own).library.get("lenses/l25")?.name).toBe("My 25");
    // Using an item the project owns changes nothing.
    expect(await useInProject(project, "lenses/l12")).toBe(project);
  });

  it("refuses a bank manifest of another version", async () => {
    const { loadBank } = await import("./project");
    await expect(
      loadBank(memory({ "etendue-bank.json": JSON.stringify({ version: 2, name: "x" }) }), null),
    ).rejects.toThrow(/bank version 2/);
  });
});
