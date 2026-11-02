/**
 * A tolerance-aware deep comparison of JSON documents, for gate G15.1: numbers within `tol`,
 * everything else equal — the same keys, the same array lengths, the same strings. A scene's
 * robot manifest path is compared as the library robot it names (`assets/robots/<id>/robot.json`),
 * since it is written relative to wherever the scene is saved.
 */

/** `…/assets/robots/<id>/robot.json` as `assets/robots/<id>/robot.json`. */
export function robotOf(manifest: string): string {
  return /(?:^|\/)(assets\/robots(?:-local)?\/[^/]+\/robot\.json)$/.exec(manifest)?.[1] ?? manifest;
}

/** Every way `actual` differs from `expected`, as `path: what` lines; none when they match. */
export function differences(actual: unknown, expected: unknown, tol = 1e-12, path = "$"): string[] {
  if (typeof expected === "number" && typeof actual === "number") {
    return Math.abs(actual - expected) <= tol ? [] : [`${path}: ${actual} ≠ ${expected} (Δ ${actual - expected})`];
  }
  if (typeof expected === "string" && typeof actual === "string" && path.endsWith(".manifest")) {
    return robotOf(actual) === robotOf(expected) ? [] : [`${path}: ${actual} names another robot than ${expected}`];
  }
  if (Array.isArray(expected)) {
    if (!Array.isArray(actual)) return [`${path}: not an array`];
    if (actual.length !== expected.length) return [`${path}: ${actual.length} entries, not ${expected.length}`];
    return expected.flatMap((e, i) => differences(actual[i], e, tol, `${path}[${i}]`));
  }
  if (typeof expected === "object" && expected !== null) {
    if (typeof actual !== "object" || actual === null || Array.isArray(actual)) return [`${path}: not an object`];
    const a = actual as Record<string, unknown>;
    const e = expected as Record<string, unknown>;
    const keys = [...new Set([...Object.keys(a), ...Object.keys(e)])].sort();
    return keys.flatMap((k) =>
      !(k in a)
        ? [`${path}.${k}: missing`]
        : !(k in e)
          ? [`${path}.${k}: not expected`]
          : differences(a[k], e[k], tol, `${path}.${k}`),
    );
  }
  return actual === expected ? [] : [`${path}: ${JSON.stringify(actual)} ≠ ${JSON.stringify(expected)}`];
}

/** The largest `|actual − expected|` over the numbers both documents hold at the same place. */
export function largestDeviation(actual: unknown, expected: unknown): number {
  if (typeof expected === "number" && typeof actual === "number") return Math.abs(actual - expected);
  if (Array.isArray(expected) && Array.isArray(actual)) {
    return expected.reduce((m: number, e, i) => Math.max(m, largestDeviation(actual[i], e)), 0);
  }
  if (typeof expected === "object" && expected !== null && typeof actual === "object" && actual !== null) {
    const a = actual as Record<string, unknown>;
    return Object.entries(expected).reduce((m, [k, e]) => Math.max(m, largestDeviation(a[k], e)), 0);
  }
  return 0;
}
