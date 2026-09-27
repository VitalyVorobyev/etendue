import {
  BufferGeometry,
  Color,
  type ColorRepresentation,
  Float32BufferAttribute,
  LineBasicMaterial,
  LineSegments,
} from "three";

import { GIZMO_LAYER } from "../layers";

/** Colours of the three axes. */
export interface AxesColors {
  /** The X axis (vitavision: `defect`). */
  x: ColorRepresentation;
  /** The Y axis (vitavision: `normal`). */
  y: ColorRepresentation;
  /** The Z axis (vitavision: `signal`). */
  z: ColorRepresentation;
}

/**
 * A frame's axes as three lines of length `size` from the origin. Colours come from the
 * caller — with vitavision tokens: X `defect`, Y `normal`, Z `signal`.
 */
export class Axes extends LineSegments<BufferGeometry, LineBasicMaterial> {
  constructor(size: number, colors: AxesColors) {
    const geometry = new BufferGeometry();
    geometry.setAttribute("position", new Float32BufferAttribute([0, 0, 0, size, 0, 0, 0, 0, 0, 0, size, 0, 0, 0, 0, 0, 0, size], 3));
    geometry.setAttribute("color", new Float32BufferAttribute(new Array<number>(18).fill(1), 3));
    super(geometry, new LineBasicMaterial({ vertexColors: true }));
    this.layers.set(GIZMO_LAYER);
    this.setColors(colors);
  }

  /** Change the colours. */
  setColors(colors: AxesColors): void {
    const attr = this.geometry.getAttribute("color");
    const c = new Color();
    (["x", "y", "z"] as const).forEach((axis, i) => {
      c.set(colors[axis]);
      attr.setXYZ(2 * i, c.r, c.g, c.b);
      attr.setXYZ(2 * i + 1, c.r, c.g, c.b);
    });
    attr.needsUpdate = true;
  }
}
