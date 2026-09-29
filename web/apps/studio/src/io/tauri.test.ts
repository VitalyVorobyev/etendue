import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";

import { type Progress, generateDataset } from "../kernel/native";
import { loadScene } from "./load";
import { absolute, fsSource, isTauri, parentDir, splitAbsolute } from "./tauri";

interface Internals {
  runCallback: (id: number, data: unknown) => void;
}

afterEach(() => clearMocks());

describe("absolute paths", () => {
  it("split into a source root and a relative path, and back", () => {
    expect(splitAbsolute("/Users/me/scene.json")).toEqual({ root: "/", relative: "Users/me/scene.json" });
    expect(splitAbsolute("C:\\data\\scene.json")).toEqual({ root: "", relative: "C:/data/scene.json" });
    expect(absolute("/", "Users/me/scene.json")).toBe("/Users/me/scene.json");
    expect(absolute("", "C:/data/scene.json")).toBe("C:/data/scene.json");
    expect(absolute("/repo/", "examples/x/scene.json")).toBe("/repo/examples/x/scene.json");
    expect(parentDir("/a/b/scene.json")).toBe("/a/b");
    expect(parentDir("/scene.json")).toBe("/");
    expect(parentDir("C:\\a\\scene.json")).toBe("C:/a");
  });
});

describe("the Tauri shell", () => {
  it("is detected by its IPC internals", () => {
    expect(isTauri()).toBe(false);
    mockIPC(() => null);
    expect(isTauri()).toBe(true);
  });

  it("loads a scene from the filesystem, with its absolute path", async () => {
    const files: Record<string, string> = {
      "/work/cell/scene.json": JSON.stringify({ version: 1, robots: [{ id: "ur", manifest: "../robots/ur/robot.json" }] }),
      "/work/robots/ur/robot.json": JSON.stringify({ urdf: "robot.urdf", joints: [] }),
      "/work/robots/ur/robot.urdf": "<robot/>",
    };
    const asked: string[] = [];
    mockIPC((cmd, args) => {
      const path = (args as { path: string }).path;
      asked.push(`${cmd} ${path}`);
      if (cmd === "path_exists") return path in files;
      if (cmd === "read_text") return files[path];
      return null;
    });
    const { root, relative } = splitAbsolute("/work/cell/scene.json");
    const loaded = await loadScene(fsSource(root), relative, null);
    expect(loaded.origin).toBe("/work/cell/scene.json");
    expect(loaded.robots[0]!.urdf).toBe("<robot/>");
    expect(asked).toContain("read_text /work/robots/ur/robot.json");
  });

  it("streams dataset progress through a channel", async () => {
    const seen: Progress[] = [];
    mockIPC((cmd, args) => {
      if (cmd !== "generate_dataset") return null;
      const { onProgress, request } = args as { onProgress: { id: number }; request: { output: string } };
      const internals = (window as unknown as { __TAURI_INTERNALS__: Internals }).__TAURI_INTERNALS__;
      internals.runCallback(onProgress.id, { index: 0, message: { kind: "step", stage: "render", done: 1, total: 2 } });
      internals.runCallback(onProgress.id, { index: 1, message: { kind: "log", line: "etendue: rendered x" } });
      return { kind: "ok", output: request.output, gt: { captures: 1, views: 1, visible: 4, points: 4 }, render: null, detection: [] };
    });
    const outcome = await generateDataset(
      "run-1",
      {
        scene: "/s/scene.json",
        scenario: { version: 1, dt: 0.01, steps: [] },
        output: "/s/out",
        render: false,
        samples: 16,
        supersample: 4,
        sensor: null,
        cameras: [],
        cpu: false,
        blender: null,
        allowBlenderVersion: false,
      },
      (p) => seen.push(p),
    );
    expect(outcome.kind).toBe("ok");
    expect(seen).toEqual([
      { kind: "step", stage: "render", done: 1, total: 2 },
      { kind: "log", line: "etendue: rendered x" },
    ]);
  });
});
