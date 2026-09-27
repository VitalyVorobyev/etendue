import {
  BufferGeometry,
  type ColorRepresentation,
  Float32BufferAttribute,
  Group,
  LineBasicMaterial,
  LineSegments,
} from "three";

/** A light's emitter shape (etendue `LightShape`), tagged on `type`. */
export type LightShapeLike =
  | { type: "point"; radius_m?: number }
  | { type: "spot"; cone_angle: number; blend?: number }
  | { type: "area"; size_m: ArrayLike<number> };

/**
 * A wireframe symbol for a light, in the light's frame (directional lights emit along +Z):
 * a star for a point light, a cone for a spot, a rectangle with a normal for an area light.
 * `size` scales the symbol (metres).
 */
export class LightGizmo extends Group {
  readonly #material: LineBasicMaterial;

  constructor(shape: LightShapeLike, color: ColorRepresentation, size = 0.05) {
    super();
    const v: number[] = [];
    if (shape.type === "point") {
      const r = Math.max(size / 2, shape.radius_m ?? 0);
      for (const [x, y, z] of [
        [1, 0, 0],
        [0, 1, 0],
        [0, 0, 1],
        [0.577, 0.577, 0.577],
        [-0.577, 0.577, 0.577],
        [0.577, -0.577, 0.577],
        [0.577, 0.577, -0.577],
      ] as const) {
        v.push(-x * r, -y * r, -z * r, x * r, y * r, z * r);
      }
    } else if (shape.type === "spot") {
      const len = size * 2;
      const rad = len * Math.tan(shape.cone_angle / 2);
      const n = 16;
      for (let i = 0; i < n; i++) {
        const a0 = (2 * Math.PI * i) / n;
        const a1 = (2 * Math.PI * (i + 1)) / n;
        v.push(rad * Math.cos(a0), rad * Math.sin(a0), len, rad * Math.cos(a1), rad * Math.sin(a1), len);
        if (i % 4 === 0) v.push(0, 0, 0, rad * Math.cos(a0), rad * Math.sin(a0), len);
      }
    } else {
      const x = (shape.size_m[0] ?? 0) / 2;
      const y = (shape.size_m[1] ?? 0) / 2;
      v.push(-x, -y, 0, x, -y, 0, x, -y, 0, x, y, 0, x, y, 0, -x, y, 0, -x, y, 0, -x, -y, 0);
      v.push(0, 0, 0, 0, 0, size);
    }
    const geometry = new BufferGeometry();
    geometry.setAttribute("position", new Float32BufferAttribute(v, 3));
    this.#material = new LineBasicMaterial({ color });
    this.add(new LineSegments(geometry, this.#material));
  }

  /** Change the colour. */
  setColor(color: ColorRepresentation): void {
    this.#material.color.set(color);
  }
}
