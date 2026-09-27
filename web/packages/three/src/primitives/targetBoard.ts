import {
  BufferGeometry,
  type ColorRepresentation,
  DoubleSide,
  Float32BufferAttribute,
  Group,
  LineBasicMaterial,
  LineLoop,
  Mesh,
  MeshBasicMaterial,
  type Object3D,
} from "three";

const NO_RAYCAST: Object3D["raycast"] = () => undefined;

/** Options of {@link TargetBoard}. */
export interface TargetBoardOptions {
  /** Extent along local X, metres. */
  width: number;
  /** Extent along local Y, metres. */
  height: number;
  /** Base (light) colour. */
  color: ColorRepresentation;
  /** Outline and dark-square colour. */
  edgeColor: ColorRepresentation;
  /**
   * Draw a checkerboard of `cols × rows` squares (columns along X), the dark square at the
   * −X/−Y corner. Omit for a plain surface.
   */
  checker?: { cols: number; rows: number } | undefined;
}

/**
 * A planar target: a `width × height` rectangle centred on the local origin in the `z = 0`
 * plane, facing +Z (the etendue target frame), optionally with a checkerboard pattern.
 */
export class TargetBoard extends Group {
  readonly #base: MeshBasicMaterial;
  readonly #dark: MeshBasicMaterial;
  readonly #edge: LineBasicMaterial;
  #active = false;

  constructor({ width, height, color, edgeColor, checker }: TargetBoardOptions) {
    super();
    const w = width / 2;
    const h = height / 2;
    const base = new BufferGeometry();
    base.setAttribute("position", new Float32BufferAttribute([-w, -h, 0, w, -h, 0, w, h, 0, -w, -h, 0, w, h, 0, -w, h, 0], 3));
    this.#base = new MeshBasicMaterial({ color, side: DoubleSide, polygonOffset: true, polygonOffsetFactor: 1 });
    this.#dark = new MeshBasicMaterial({ color: edgeColor, side: DoubleSide });
    this.#edge = new LineBasicMaterial({ color: edgeColor });
    this.add(new Mesh(base, this.#base));

    if (checker && checker.cols > 0 && checker.rows > 0) {
      const sx = width / checker.cols;
      const sy = height / checker.rows;
      const squares: number[] = [];
      for (let r = 0; r < checker.rows; r++) {
        for (let c = 0; c < checker.cols; c++) {
          if ((r + c) % 2 !== 0) continue;
          const x0 = -w + c * sx;
          const y0 = -h + r * sy;
          const x1 = x0 + sx;
          const y1 = y0 + sy;
          squares.push(x0, y0, 0, x1, y0, 0, x1, y1, 0, x0, y0, 0, x1, y1, 0, x0, y1, 0);
        }
      }
      const dark = new BufferGeometry();
      dark.setAttribute("position", new Float32BufferAttribute(squares, 3));
      this.add(new Mesh(dark, this.#dark));
    }

    const outline = new BufferGeometry();
    outline.setAttribute("position", new Float32BufferAttribute([-w, -h, 0, w, -h, 0, w, h, 0, -w, h, 0], 3));
    const loop = new LineLoop(outline, this.#edge);
    // Lines are hit within a world-unit threshold: picks must land on the surface instead.
    loop.raycast = NO_RAYCAST;
    this.add(loop);
  }

  /** Whether the board is drawn emphasised (selected). */
  get active(): boolean {
    return this.#active;
  }

  /**
   * Draw the board emphasised or not: the outline is drawn over everything while active, so a
   * selected board stays findable behind other geometry.
   */
  setActive(value: boolean): void {
    this.#active = value;
    this.#edge.depthTest = !value;
    this.#edge.needsUpdate = true;
  }

  /** Make the whole board translucent (`1` = opaque). */
  setOpacity(opacity: number): void {
    for (const m of [this.#base, this.#dark]) {
      m.opacity = opacity;
      m.transparent = opacity < 1;
      m.depthWrite = opacity >= 1;
      m.needsUpdate = true;
    }
  }

  /** Change the colours. */
  setColors(color: ColorRepresentation, edgeColor: ColorRepresentation): void {
    this.#base.color.set(color);
    this.#dark.color.set(edgeColor);
    this.#edge.color.set(edgeColor);
  }
}
