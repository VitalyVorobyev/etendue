import type { Object3D } from "three";

/**
 * Layer of the physical world (robots, targets, parts): what a sensor sees. three's default.
 */
export const PHYSICAL_LAYER = 0;

/**
 * Layer of viewer-only gizmos (frusta, laser fans, light symbols, axes, grids). A viewport
 * camera enables it; a {@link SensorView} does not, so synthetic images show only the world.
 */
export const GIZMO_LAYER = 1;

/** Put `root` and all its descendants on `layer` only. */
export function setLayer(root: Object3D, layer: number): void {
  root.traverse((o) => o.layers.set(layer));
}
