import { describe, expect, it, vi } from "vitest";

import type { FileSource } from "./source";

// The kernel's checks are tested in Rust; here only the reading.
vi.mock("@etendue/wasm", () => ({
  validateLibraryItem: (text: string) => {
    const item = JSON.parse(text) as { version: number };
    if (item.version !== 1) throw new Error(`library item version ${item.version}`);
    return item;
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
      "p/library/lenses/new.json": item("new", "Future", "lens", 2),
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
