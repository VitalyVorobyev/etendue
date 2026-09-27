import { describe, expect, it } from "vitest";

import { documentKind, libraryPath, loadScene } from "./load";
import { dirname, join } from "./paths";
import { type FileSource, filePath, filesSource } from "./source";

const file = (path: string, text: string) => {
  const f = new File([text], path.split("/").at(-1)!);
  if (path.includes("/")) Object.defineProperty(f, "webkitRelativePath", { value: path });
  return f;
};

describe("paths", () => {
  it("joins and folds relative segments", () => {
    expect(dirname("a/b/c.json")).toBe("a/b");
    expect(dirname("c.json")).toBe("");
    expect(join("examples/x", "../../assets/robots/ur5e/robot.json")).toBe("assets/robots/ur5e/robot.json");
    expect(join("", "./a/./b")).toBe("a/b");
    expect(join("a", "../../b")).toBe("../b");
  });
});

describe("filesSource", () => {
  it("finds files by exact path or by unique trailing components", async () => {
    const src = filesSource([
      file("bundle/scene.json", "{}"),
      file("bundle/robots/ur5e/robot.json", "ur5e"),
      file("bundle/robots/irb/robot.json", "irb"),
      file("loose.urdf", "<robot/>"),
    ]);
    expect(src.paths).toContain("bundle/scene.json");
    expect(filePath(new File([], "x.json"))).toBe("x.json");
    expect(await src.readText("bundle/scene.json")).toBe("{}");
    expect(await src.readText("assets/robots/ur5e/robot.json")).toBe("ur5e");
    expect(await src.readText("somewhere/loose.urdf")).toBe("<robot/>");
    // Two `robot.json`s match equally well: ambiguous, not found.
    expect(src.has("robot.json")).toBe(false);
    await expect(src.readText("nope.json")).rejects.toThrow(/not found/);
    expect(src.url("nope.json")).toBe("");
    const url = src.url("bundle/scene.json");
    expect(url).toMatch(/^blob:/);
    expect(src.url("bundle/scene.json")).toBe(url);
    src.dispose();
  });
});

describe("loadScene", () => {
  const manifest = { version: 1, id: "r", urdf: "robot.urdf", joints: [], visuals: [{ link: "a", mesh: "meshes/a.glb" }] };
  const scene = {
    version: 1,
    robots: [{ id: "arm", parent: "world", manifest: "../../assets/robots/r/robot.json" }],
  };

  function memory(name: string, files: Record<string, string>): FileSource {
    return {
      name,
      has: (p) => p in files,
      readText: (p) => (p in files ? Promise.resolve(files[p]!) : Promise.reject(new Error(`${p} missing`))),
      url: (p) => `${name}:${p}`,
    };
  }

  it("resolves manifests against the scene and meshes against the manifest", async () => {
    const src = memory("repo", {
      "examples/x/scene.json": JSON.stringify(scene),
      "examples/x/scenario.json": JSON.stringify({ version: 1, dt: 0.01, steps: [] }),
      "assets/robots/r/robot.json": JSON.stringify(manifest),
      "assets/robots/r/robot.urdf": "<robot/>",
    });
    const loaded = await loadScene(src, "examples/x/scene.json", "examples/x/scenario.json");
    expect(loaded.label).toBe("examples/x");
    expect(loaded.scenario).toEqual({ version: 1, dt: 0.01, steps: [] });
    expect(loaded.robots[0]!.urdf).toBe("<robot/>");
    expect(loaded.robots[0]!.meshUrl("meshes/a.glb")).toBe("repo:assets/robots/r/meshes/a.glb");
  });

  it("falls back to the robot library for loose scenes", async () => {
    const dropped = memory("dropped", { "scene.json": JSON.stringify(scene) });
    const library = memory("repo", {
      "assets/robots/r/robot.json": JSON.stringify(manifest),
      "assets/robots/r/robot.urdf": "<robot/>",
    });
    const loaded = await loadScene(dropped, "scene.json", null, library);
    expect(loaded.scenario).toBeNull();
    expect(loaded.robots[0]!.manifestPath).toBe("repo: assets/robots/r/robot.json");
    await expect(loadScene(dropped, "scene.json", null)).rejects.toThrow(/manifest .* not found/);
  });

  it("reports unparsable documents with their path", async () => {
    const src = memory("repo", { "s.json": "{" });
    await expect(loadScene(src, "s.json", null)).rejects.toThrow(/repo: s.json/);
  });

  it("maps manifest paths into the library", () => {
    expect(libraryPath("../../assets/robots/ur5e/robot.json")).toBe("assets/robots/ur5e/robot.json");
    expect(libraryPath("robots/ur5e/robot.json")).toBe("assets/robots/ur5e/robot.json");
    expect(libraryPath("robot.json")).toBeNull();
  });
});

describe("documentKind", () => {
  it("tells documents apart by their fields", () => {
    expect(documentKind({ version: 1, robots: [] })).toBe("scene");
    expect(documentKind({ version: 1, dt: 0.01, steps: [] })).toBe("scenario");
    expect(documentKind({ frames: [], samples: [] })).toBe("baked");
    expect(documentKind({ urdf: "r.urdf", joints: [] })).toBe("manifest");
    expect(documentKind({ hello: 1 })).toBeNull();
    expect(documentKind(null)).toBeNull();
  });
});
