/**
 * Robot visuals: one mesh per link, attached to the link frames of a
 * {@link FrameTreeRuntime}. Link meshes are glTF binaries in the link frame (etendue
 * `robot.json` `visuals`; URDF convention, +Z up, no glTF Y-up conversion), so they need no
 * transform of their own — the baked link pose places them.
 */

import { type ColorRepresentation, Mesh, MeshStandardMaterial, type Object3D } from "three";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";

import type { FrameTreeRuntime } from "./frameTree";

/** A link's visual mesh (etendue `ManifestVisual`). */
export interface RobotVisual {
  /** URDF link name. */
  link: string;
  /** Mesh path, relative to the manifest. */
  mesh: string;
}

/** Loads one mesh file. */
export interface MeshLoader {
  /** Load the mesh at `url`; reject if it cannot be loaded. */
  load(url: string): Promise<Object3D>;
}

/** A {@link MeshLoader} for glTF / GLB files. */
export function gltfMeshLoader(): MeshLoader {
  const loader = new GLTFLoader();
  return { load: async (url) => (await loader.loadAsync(url)).scene };
}

/** The outcome of {@link loadRobotVisuals}. */
export interface RobotVisuals {
  /** Loaded meshes by link name. */
  meshes: Map<string, Object3D>;
  /** Links whose mesh failed to load, with the reason. */
  failed: { link: string; error: string }[];
}

/**
 * Load every link mesh. `resolve` maps a manifest-relative mesh path to a URL. A mesh that
 * fails to load is reported in `failed`, not thrown: a robot without meshes still animates
 * (draw its link frames instead).
 */
export async function loadRobotVisuals(
  visuals: readonly RobotVisual[],
  resolve: (meshPath: string) => string,
  loader: MeshLoader = gltfMeshLoader(),
): Promise<RobotVisuals> {
  const meshes = new Map<string, Object3D>();
  const failed: { link: string; error: string }[] = [];
  await Promise.all(
    visuals.map(async ({ link, mesh }) => {
      try {
        const object = await loader.load(resolve(mesh));
        object.name = link;
        meshes.set(link, object);
      } catch (e) {
        failed.push({ link, error: e instanceof Error ? e.message : String(e) });
      }
    }),
  );
  failed.sort((a, b) => a.link.localeCompare(b.link));
  return { meshes, failed };
}

/**
 * Add each link mesh to frame `"<robotId>/<link>"` of `runtime`. Returns the links that have
 * no such frame (none, for a runtime baked from the same scene).
 */
export function attachRobotVisuals(
  runtime: FrameTreeRuntime,
  robotId: string,
  meshes: ReadonlyMap<string, Object3D>,
): string[] {
  const missing: string[] = [];
  for (const [link, object] of meshes) {
    const frame = runtime.frame(`${robotId}/${link}`);
    if (frame) frame.add(object);
    else missing.push(link);
  }
  return missing;
}

/**
 * Replace every mesh material under `roots` with one shared matte material of `color`, so
 * robots read as one family whatever colours their source meshes carry. Returns the material
 * (change its colour on a theme switch; dispose it with the robot).
 */
export function applyRobotMaterial(roots: Iterable<Object3D>, color: ColorRepresentation): MeshStandardMaterial {
  const material = new MeshStandardMaterial({ color, metalness: 0.1, roughness: 0.65 });
  for (const root of roots) {
    root.traverse((node) => {
      if (node instanceof Mesh) {
        const old = node.material as { dispose(): void } | { dispose(): void }[];
        for (const m of Array.isArray(old) ? old : [old]) m.dispose();
        node.material = material;
      }
    });
  }
  return material;
}
