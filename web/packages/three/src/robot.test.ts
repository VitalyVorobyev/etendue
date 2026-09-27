import { BoxGeometry, Group, Mesh, MeshBasicMaterial, type Object3D } from "three";
import { describe, expect, it, vi } from "vitest";

import { FrameTreeRuntime } from "./frameTree";
import { type MeshLoader, applyRobotMaterial, attachRobotVisuals, gltfMeshLoader, loadRobotVisuals } from "./robot";

const I = { rotation: [0, 0, 0, 1], translation: [0, 0, 0] };

const fakeLoad = (fail: string[] = []) =>
  vi.fn((url: string) =>
    fail.some((f) => url.endsWith(f))
      ? Promise.reject(new Error(`404 ${url}`))
      : Promise.resolve<Object3D>(new Mesh(new BoxGeometry(), new MeshBasicMaterial())),
  );

describe("robot visuals", () => {
  it("loads every link, reporting failures instead of throwing", async () => {
    const load = fakeLoad(["b.glb"]);
    const loader: MeshLoader = { load };
    const { meshes, failed } = await loadRobotVisuals(
      [
        { link: "a", mesh: "meshes/a.glb" },
        { link: "b", mesh: "meshes/b.glb" },
      ],
      (p) => `/robots/r/${p}`,
      loader,
    );
    expect(load).toHaveBeenCalledWith("/robots/r/meshes/a.glb");
    expect([...meshes.keys()]).toEqual(["a"]);
    expect(meshes.get("a")!.name).toBe("a");
    expect(failed).toEqual([{ link: "b", error: "404 /robots/r/meshes/b.glb" }]);
  });

  it("reports non-Error rejections", async () => {
    const { failed } = await loadRobotVisuals([{ link: "a", mesh: "a.glb" }], (p) => p, {
      // A loader that rejects with a non-Error is exactly what this test is about.
      // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors
      load: () => Promise.reject("plain"),
    });
    expect(failed).toEqual([{ link: "a", error: "plain" }]);
  });

  it("attaches meshes to link frames", () => {
    const runtime = new FrameTreeRuntime({
      dt: 1,
      frames: ["world", "r/a"],
      samples: [{ t: 0, world_se3_frame: [I, I] }],
    });
    const a = new Group();
    const missing = attachRobotVisuals(runtime, "r", new Map([["a", a], ["zz", new Group()]]));
    expect(a.parent).toBe(runtime.frame("r/a"));
    expect(missing).toEqual(["zz"]);
  });

  it("replaces materials with one shared material", () => {
    const root = new Group();
    const m1 = new Mesh(new BoxGeometry(), new MeshBasicMaterial());
    const m2 = new Mesh(new BoxGeometry(), [new MeshBasicMaterial(), new MeshBasicMaterial()]);
    root.add(m1);
    const material = applyRobotMaterial([root, m2], "gray");
    expect(m1.material).toBe(material);
    expect(m2.material).toBe(material);
  });

  it("builds a glTF loader", () => {
    expect(typeof gltfMeshLoader().load).toBe("function");
  });
});
