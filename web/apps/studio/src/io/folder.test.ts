import { describe, expect, it } from "vitest";

import { type DirectoryHandle, folderSource } from "./folder";

/** An in-memory directory handle, enough of the File System Access API for `folderSource`. */
function memoryFolder(name: string, files: Map<string, string>, prefix = ""): DirectoryHandle {
  const notFound = () => new DOMException("not found", "NotFoundError");
  const children = () => {
    const out = new Map<string, "file" | "directory">();
    for (const path of files.keys()) {
      if (!path.startsWith(prefix)) continue;
      const [head, ...rest] = path.slice(prefix.length).split("/");
      out.set(head!, rest.length > 0 ? "directory" : "file");
    }
    return out;
  };
  const file = (path: string) => ({
    kind: "file" as const,
    name: path.split("/").at(-1)!,
    getFile: () => Promise.resolve(new File([files.get(path) ?? ""], path)),
    createWritable: () => {
      let text = "";
      return Promise.resolve({
        write: (data: string) => {
          text += data;
          return Promise.resolve();
        },
        close: () => {
          files.set(path, text);
          return Promise.resolve();
        },
      });
    },
  });
  return {
    kind: "directory",
    name,
    getDirectoryHandle(child, options) {
      const kind = children().get(child);
      if (kind === "file" || (kind === undefined && !options?.create)) return Promise.reject(notFound());
      return Promise.resolve(memoryFolder(child, files, `${prefix}${child}/`));
    },
    getFileHandle(child, options) {
      const path = `${prefix}${child}`;
      if (!files.has(path) && !options?.create) return Promise.reject(notFound());
      return Promise.resolve(file(path));
    },
    values() {
      const entries = [...children()].map(([child, kind]) =>
        kind === "file" ? file(`${prefix}${child}`) : memoryFolder(child, files, `${prefix}${child}/`),
      );
      return (async function* () {
        for (const e of entries) yield await Promise.resolve(e);
      })();
    },
  };
}

describe("a project folder opened in the browser", () => {
  it("reads, lists and writes, creating folders, and refuses paths out of it", async () => {
    const files = new Map([
      ["etendue-project.json", "{}"],
      ["library/cameras/a.json", "A"],
      ["library/cameras/b.json", "B"],
      ["library/cameras/sub/c.json", "C"],
    ]);
    const source = folderSource(memoryFolder("proj", files));
    expect(source.name).toBe("proj");
    expect(await source.readText("library/cameras/b.json")).toBe("B");
    expect(await source.has("library/cameras/a.json")).toBe(true);
    expect(await source.has("library/lenses/x.json")).toBe(false);
    expect(await source.list!("library/cameras")).toEqual(["a.json", "b.json"]);
    expect(await source.list!("library/lasers")).toEqual([]);
    await source.write("cell_1/scene.json", "{ }");
    expect(files.get("cell_1/scene.json")).toBe("{ }");
    await expect(source.readText("../outside.json")).rejects.toThrow(/outside the project/);
  });
});
