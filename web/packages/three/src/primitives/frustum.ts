import {
  BufferGeometry,
  type ColorRepresentation,
  DoubleSide,
  Float32BufferAttribute,
  Group,
  LineBasicMaterial,
  LineSegments,
  Mesh,
  MeshBasicMaterial,
  type Object3D,
} from "three";

/** Opts a visual-only object out of raycasting, so picks land on the hitbox. */
const NO_RAYCAST: Object3D["raycast"] = () => undefined;

/**
 * Pixel coordinates around the image border, clockwise from the top-left corner `(0, 0)`,
 * `perEdge` samples per edge (flat `[u0, v0, u1, v1, …]`, `4 · perEdge` points). Corners are
 * at point indices `0, perEdge, 2·perEdge, 3·perEdge`. The border is the outer image rectangle
 * `[0, width] × [0, height]`.
 *
 * Back-project these (`@etendue/wasm` `backprojectPixels`) to get the border rays of a camera's
 * true field of view, distortion included.
 */
export function imageBorderPixels(width: number, height: number, perEdge = 8): Float64Array {
  const n = Math.max(1, Math.floor(perEdge));
  const out = new Float64Array(8 * n);
  const corners: [number, number][] = [
    [0, 0],
    [width, 0],
    [width, height],
    [0, height],
  ];
  for (let e = 0; e < 4; e++) {
    const [u0, v0] = corners[e]!;
    const [u1, v1] = corners[(e + 1) % 4]!;
    for (let i = 0; i < n; i++) {
      const s = i / n;
      out[2 * (e * n + i)] = u0 + (u1 - u0) * s;
      out[2 * (e * n + i) + 1] = v0 + (v1 - v0) * s;
    }
  }
  return out;
}

/** Options of {@link CameraFrustum}. */
export interface CameraFrustumOptions {
  /**
   * The image border's viewing rays as camera-frame points on the `z = 1` plane, flat
   * `[x0, y0, 1, x1, …]`, in order around the border (see {@link imageBorderPixels}).
   */
  borderRays: ArrayLike<number>;
  /** Point indices of the border drawn as edges from the apex; default: the 4 corners of a
   * border sampled by {@link imageBorderPixels}. */
  apexEdges?: readonly number[];
  /** Depth of the far outline along the optical axis, in metres. */
  depth: number;
  /** Line and fill colour. */
  color: ColorRepresentation;
}

/**
 * A camera's field of view: edges from the optical centre to the image border at `depth`,
 * the border outline, a faint far face, and an invisible hull for picking
 * ({@link CameraFrustum.hitbox}). Built in the CV camera frame (+Z forward), so add it to
 * the camera's frame object as is.
 */
export class CameraFrustum extends Group {
  /** The invisible, raycastable hull (sides and far face). Picks resolve to it. */
  readonly hitbox: Mesh;
  readonly #lines: LineBasicMaterial;
  readonly #face: MeshBasicMaterial;
  #active = false;

  constructor(options: CameraFrustumOptions) {
    super();
    const { borderRays, depth } = options;
    const count = Math.floor(borderRays.length / 3);
    if (count < 3) throw new Error("a frustum needs at least 3 border rays");
    const far: [number, number, number][] = [];
    for (let i = 0; i < count; i++) {
      const z = borderRays[3 * i + 2]!;
      const k = depth / z;
      far.push([borderRays[3 * i]! * k, borderRays[3 * i + 1]! * k, depth]);
    }
    const quarter = count / 4;
    const apexEdges = options.apexEdges ?? [0, quarter, 2 * quarter, 3 * quarter].map(Math.round);

    const edges: number[] = [];
    for (const i of apexEdges) edges.push(0, 0, 0, ...far[i % count]!);
    for (let i = 0; i < count; i++) edges.push(...far[i]!, ...far[(i + 1) % count]!);
    const edgeGeometry = new BufferGeometry();
    edgeGeometry.setAttribute("position", new Float32BufferAttribute(edges, 3));

    // The far face as a fan around its centroid; the hull adds the sides.
    const centre = far.reduce((c, p) => [c[0] + p[0] / count, c[1] + p[1] / count, depth], [0, 0, depth]);
    const face: number[] = [];
    const hull: number[] = [];
    for (let i = 0; i < count; i++) {
      const a = far[i]!;
      const b = far[(i + 1) % count]!;
      face.push(...centre, ...a, ...b);
      hull.push(...centre, ...a, ...b, 0, 0, 0, ...a, ...b);
    }
    const faceGeometry = new BufferGeometry();
    faceGeometry.setAttribute("position", new Float32BufferAttribute(face, 3));
    const hullGeometry = new BufferGeometry();
    hullGeometry.setAttribute("position", new Float32BufferAttribute(hull, 3));

    this.#lines = new LineBasicMaterial({ color: options.color, transparent: true });
    this.#face = new MeshBasicMaterial({
      color: options.color,
      transparent: true,
      side: DoubleSide,
      depthWrite: false,
    });
    const lines = new LineSegments(edgeGeometry, this.#lines);
    lines.raycast = NO_RAYCAST;
    const farFace = new Mesh(faceGeometry, this.#face);
    farFace.raycast = NO_RAYCAST;
    this.hitbox = new Mesh(
      hullGeometry,
      new MeshBasicMaterial({ transparent: true, opacity: 0, side: DoubleSide, depthWrite: false }),
    );
    this.hitbox.name = "hitbox";
    this.add(lines, farFace, this.hitbox);
    this.setActive(false);
  }

  /** Whether the frustum is drawn emphasised (selected). */
  get active(): boolean {
    return this.#active;
  }

  /** Draw the frustum emphasised (selected) or not. */
  setActive(value: boolean): void {
    this.#active = value;
    this.#lines.opacity = value ? 1 : 0.6;
    this.#face.opacity = value ? 0.14 : 0.04;
  }

  /** Change the colour. */
  setColor(color: ColorRepresentation): void {
    this.#lines.color.set(color);
    this.#face.color.set(color);
  }
}
