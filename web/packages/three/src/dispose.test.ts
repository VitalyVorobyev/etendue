import { BoxGeometry, DataTexture, Group, Mesh, MeshBasicMaterial } from "three";
import { describe, expect, it, vi } from "vitest";

import { disposeObject } from "./dispose";

describe("disposeObject", () => {
  it("disposes geometries, materials, and their textures", () => {
    const texture = new DataTexture();
    const material = new MeshBasicMaterial({ map: texture });
    const geometry = new BoxGeometry();
    const other = [new MeshBasicMaterial(), new MeshBasicMaterial()];
    const root = new Group();
    root.add(new Mesh(geometry, material), new Mesh(new BoxGeometry(), other));
    const spies = [vi.spyOn(texture, "dispose"), vi.spyOn(material, "dispose"), vi.spyOn(geometry, "dispose"), vi.spyOn(other[1]!, "dispose")];
    disposeObject(root);
    for (const s of spies) expect(s).toHaveBeenCalledOnce();
  });
});
