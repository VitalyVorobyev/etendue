/**
 * Robot meshes, fetched and parsed once per URL. Every bake builds a new frame tree, and
 * `Robot` loads its meshes again for it: from this cache that is a clone, not a download.
 *
 * A clone shares the cached geometry. `Robot` disposes what it attached when it unmounts, which
 * only frees the GPU copy: three.js uploads the geometry again when a later clone is drawn.
 */

import { type MeshLoader, gltfMeshLoader } from "@vitavision/three";
import type { Object3D } from "three";

const cache = new Map<string, Promise<Object3D>>();
let gltf: MeshLoader | undefined;

/** A glTF loader that downloads each URL once and hands out clones. */
export const cachedMeshLoader: MeshLoader = {
  load(url: string): Promise<Object3D> {
    let mesh = cache.get(url);
    if (!mesh) {
      gltf ??= gltfMeshLoader();
      // A failure is kept too: a missing mesh (robot assets not built) is not asked for again at
      // every bake. Reloading the app retries.
      mesh = gltf.load(url);
      cache.set(url, mesh);
    }
    return mesh.then((m) => m.clone(true));
  },
};
