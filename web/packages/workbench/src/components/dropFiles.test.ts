import { describe, expect, it } from "vitest";

import { type DropEntry, acceptsFile, carriesFiles, collectDroppedFiles, partitionFiles } from "./dropFiles";

const file = (name: string, type = "") => new File(["x"], name, { type });

describe("acceptsFile", () => {
  it("accepts everything without an accept string", () => {
    expect(acceptsFile(file("a.bin"), undefined)).toBe(true);
    expect(acceptsFile(file("a.bin"), " , ")).toBe(true);
  });

  it("matches extensions, MIME types and wildcards, case-insensitively", () => {
    const accept = ".json, image/*, model/gltf-binary";
    expect(acceptsFile(file("Scene.JSON"), accept)).toBe(true);
    expect(acceptsFile(file("shot.png", "image/png"), accept)).toBe(true);
    expect(acceptsFile(file("arm.glb", "model/gltf-binary"), accept)).toBe(true);
    expect(acceptsFile(file("notes.txt", "text/plain"), accept)).toBe(false);
  });
});

describe("partitionFiles", () => {
  it("splits by accept, keeping order", () => {
    const files = [file("a.json"), file("b.txt"), file("c.json")];
    const { accepted, rejected } = partitionFiles(files, ".json");
    expect(accepted.map((f) => f.name)).toEqual(["a.json", "c.json"]);
    expect(rejected.map((f) => f.name)).toEqual(["b.txt"]);
  });
});

describe("carriesFiles", () => {
  it("is true only for drags with Files among their types", () => {
    expect(carriesFiles({ types: ["Files"] })).toBe(true);
    expect(carriesFiles({ types: ["text/plain"] })).toBe(false);
    expect(carriesFiles(null)).toBe(false);
  });
});

/** A fake file entry. */
function fileEntry(path: string): DropEntry {
  const name = path.split("/").at(-1) ?? path;
  return {
    isFile: true,
    isDirectory: false,
    name,
    fullPath: `/${path}`,
    file: (success) => success(file(name)),
  };
}

/** A fake directory entry that returns its children in batches of `batch`. */
function dirEntry(path: string, children: DropEntry[], batch = 2): DropEntry {
  return {
    isFile: false,
    isDirectory: true,
    name: path.split("/").at(-1) ?? path,
    fullPath: `/${path}`,
    createReader: () => {
      let offset = 0;
      return {
        readEntries: (success) => {
          const next = children.slice(offset, offset + batch);
          offset += batch;
          success(next);
        },
      };
    },
  };
}

function transfer(entries: (DropEntry | null)[], files: File[] = []): Pick<DataTransfer, "files" | "items"> {
  const items = entries.map((entry) => ({ kind: "file", webkitGetAsEntry: () => entry }));
  return { files: files as unknown as FileList, items: items as unknown as DataTransferItemList };
}

describe("collectDroppedFiles", () => {
  it("returns the plain file list when nothing dropped is a folder", async () => {
    const plain = [file("a.json")];
    await expect(collectDroppedFiles(transfer([fileEntry("a.json")], plain))).resolves.toEqual(plain);
    await expect(collectDroppedFiles(transfer([], plain))).resolves.toEqual(plain);
  });

  it("walks folders, in batches, tagging each file with its relative path", async () => {
    const scenario = dirEntry("cell", [
      fileEntry("cell/scene.json"),
      fileEntry("cell/scenario.json"),
      dirEntry("cell/meshes", [fileEntry("cell/meshes/base.glb")]),
    ]);
    const files = await collectDroppedFiles(transfer([scenario, fileEntry("readme.md")]));
    expect(files.map((f) => f.name)).toEqual(["scene.json", "scenario.json", "base.glb", "readme.md"]);
    expect(files.slice(0, 3).map((f) => f.webkitRelativePath)).toEqual([
      "cell/scene.json",
      "cell/scenario.json",
      "cell/meshes/base.glb",
    ]);
    // A top-level file keeps the browser's own (empty) relative path.
    expect(Object.hasOwn(files[3] ?? {}, "webkitRelativePath")).toBe(false);
  });

  it("skips items without an entry when a folder is among them", async () => {
    const files = await collectDroppedFiles(transfer([dirEntry("d", [fileEntry("d/x.json")]), null]));
    expect(files.map((f) => f.name)).toEqual(["x.json"]);
  });
});
