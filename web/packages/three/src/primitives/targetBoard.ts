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
} from "three";

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
    this.add(new LineLoop(outline, this.#edge));
  }

  /** Change the colours. */
  setColors(color: ColorRepresentation, edgeColor: ColorRepresentation): void {
    this.#base.color.set(color);
    this.#dark.color.set(edgeColor);
    this.#edge.color.set(edgeColor);
  }
}
