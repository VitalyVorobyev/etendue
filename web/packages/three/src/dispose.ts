import type { Material, Object3D } from "three";

/**
 * Free the GPU resources of `root` and its descendants: every geometry, material, and
 * material texture. Call it when an object built by this package is removed for good.
 */
export function disposeObject(root: Object3D): void {
  root.traverse((node) => {
    const n = node as Object3D & { geometry?: { dispose(): void }; material?: Material | Material[] };
    n.geometry?.dispose();
    const materials = Array.isArray(n.material) ? n.material : n.material ? [n.material] : [];
    for (const m of materials) {
      for (const value of Object.values(m)) {
        if (value !== null && typeof value === "object" && (value as { isTexture?: boolean }).isTexture) {
          (value as { dispose(): void }).dispose();
        }
      }
      m.dispose();
    }
  });
}
