// The typed layer of @etendue/wasm: documents go in and come out as objects. JSON text is
// the wire format underneath (parsed in Rust with `float_roundtrip`), so every f64 crosses
// the boundary bit-exact.

import init, { EtendueScene as RawScene, initSync } from "./etendue_wasm.js";

export { init as default, initSync };

const text = (doc) => (typeof doc === "string" ? doc : JSON.stringify(doc));

export class EtendueScene {
  #raw;

  constructor(scene, robots) {
    this.#raw = new RawScene(text(scene), text(robots));
  }

  bake(scenario) {
    return JSON.parse(this.#raw.bake(text(scenario)));
  }

  projectPoints(cameraId, worldSe3Camera, xyzWorld) {
    const { rotation: q, translation: t } = worldSe3Camera;
    const pose = new Float64Array([q[0], q[1], q[2], q[3], t[0], t[1], t[2]]);
    const xyz = xyzWorld instanceof Float64Array ? xyzWorld : Float64Array.from(xyzWorld);
    return this.#raw.projectPoints(cameraId, pose, xyz);
  }

  backprojectPixels(cameraId, uv) {
    return this.#raw.backprojectPixels(cameraId, uv instanceof Float64Array ? uv : Float64Array.from(uv));
  }

  remap(cameraId, spec = {}, pixelCentre) {
    const full = { supersample: 1, margin: 0.02, scan_step_px: 8, ...spec };
    const out = this.#raw.remap(cameraId, JSON.stringify(full), pixelCentre);
    return { canonical: JSON.parse(out.canonical), lut: out.lut };
  }

  targetExtent(targetId) {
    const extent = this.#raw.targetExtent(targetId);
    return extent === undefined ? undefined : [extent[0], extent[1]];
  }

  free() {
    this.#raw.free();
  }

  [Symbol.dispose]() {
    this.free();
  }
}

export function isEtendueError(error) {
  return error instanceof Error && typeof error.kind === "string";
}
