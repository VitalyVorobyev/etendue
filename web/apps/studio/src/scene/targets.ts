/**
 * What the camera view draws of a target besides its corners, in the target frame (metres, the
 * `z = 0` plane): the outline of a planar target, densely sampled so its projection shows lens
 * distortion. A PuzzlePole has none (its silhouette is not a fixed curve on it).
 */

/** The outline of a planar target of `extent`, as one closed polyline (flat `[x0, y0, 0, …]`). */
export function targetOutline(extent: readonly [number, number], samplesPerEdge = 24): Float64Array {
  const [w, h] = extent;
  const corners: [number, number][] = [
    [-w / 2, -h / 2],
    [w / 2, -h / 2],
    [w / 2, h / 2],
    [-w / 2, h / 2],
    [-w / 2, -h / 2],
  ];
  const out = new Float64Array((4 * samplesPerEdge + 1) * 3);
  let k = 0;
  for (let e = 0; e < 4; e++) {
    const [x0, y0] = corners[e]!;
    const [x1, y1] = corners[e + 1]!;
    for (let i = 0; i < samplesPerEdge + (e === 3 ? 1 : 0); i++) {
      const s = i / samplesPerEdge;
      out.set([x0 + (x1 - x0) * s, y0 + (y1 - y0) * s, 0], 3 * k++);
    }
  }
  return out;
}
