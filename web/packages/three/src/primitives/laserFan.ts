import {
  BufferGeometry,
  type ColorRepresentation,
  DoubleSide,
  Float32BufferAttribute,
  Group,
  Line,
  LineBasicMaterial,
  Mesh,
  MeshBasicMaterial,
} from "three";

/** Options of {@link LaserFan}. */
export interface LaserFanOptions {
  /** Fan half-angle in radians, in `(0, π/2)`. */
  halfAngle: number;
  /** Reach along the central ray, in metres. */
  length: number;
  /** Colour. */
  color: ColorRepresentation;
  /** Arc segments. */
  segments?: number;
}

/**
 * A line laser's light sheet: a translucent circular sector in the laser's local `x = 0`
 * plane, opening symmetrically about +Z (the etendue laser frame), with its outline.
 */
export class LaserFan extends Group {
  readonly #fill: MeshBasicMaterial;
  readonly #edge: LineBasicMaterial;

  constructor({ halfAngle, length, color, segments = 32 }: LaserFanOptions) {
    super();
    const arc: [number, number, number][] = [];
    for (let i = 0; i <= segments; i++) {
      const phi = -halfAngle + (2 * halfAngle * i) / segments;
      arc.push([0, length * Math.sin(phi), length * Math.cos(phi)]);
    }
    const fan: number[] = [];
    for (let i = 0; i < segments; i++) fan.push(0, 0, 0, ...arc[i]!, ...arc[i + 1]!);
    const fanGeometry = new BufferGeometry();
    fanGeometry.setAttribute("position", new Float32BufferAttribute(fan, 3));
    const outline = [0, 0, 0, ...arc.flat(), 0, 0, 0];
    const outlineGeometry = new BufferGeometry();
    outlineGeometry.setAttribute("position", new Float32BufferAttribute(outline, 3));

    this.#fill = new MeshBasicMaterial({
      color,
      transparent: true,
      opacity: 0.18,
      side: DoubleSide,
      depthWrite: false,
    });
    this.#edge = new LineBasicMaterial({ color, transparent: true, opacity: 0.9 });
    this.add(new Mesh(fanGeometry, this.#fill), new Line(outlineGeometry, this.#edge));
  }

  /** Change the colour. */
  setColor(color: ColorRepresentation): void {
    this.#fill.color.set(color);
    this.#edge.color.set(color);
  }
}
