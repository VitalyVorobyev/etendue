// The typed layer of @etendue/wasm: documents go in and come out as objects. JSON text is
// the wire format underneath (parsed in Rust with `float_roundtrip`), so every f64 crosses
// the boundary bit-exact.

import init, {
  EtendueScene as RawScene,
  deriveOptics as rawDeriveOptics,
  expandRig as rawExpandRig,
  headApply as rawHeadApply,
  headMap as rawHeadMap,
  headOptimise as rawHeadOptimise,
  headScheimpflug as rawHeadScheimpflug,
  headSection as rawHeadSection,
  initSync,
  poleCircumferences as rawPoleCircumferences,
  rigCoverage as rawRigCoverage,
  validateLibraryItem as rawValidateLibraryItem,
} from "./etendue_wasm.js";

export { init as default, initSync };

const text = (doc) => (typeof doc === "string" ? doc : JSON.stringify(doc));

export class EtendueScene {
  #raw;

  constructor(scene, robots) {
    this.#raw = new RawScene(text(scene), text(robots));
  }

  updateScene(scene) {
    this.#raw.updateScene(text(scene));
  }

  mechSe3Projection(cameraId) {
    const w = this.#raw.mechSe3Projection(cameraId);
    return w === undefined ? undefined : { rotation: [w[0], w[1], w[2], w[3]], translation: [w[4], w[5], w[6]] };
  }

  robotLinks(robotId) {
    return this.#raw.robotLinks(robotId);
  }

  robotLinkPoses(robotId, q) {
    return this.#raw.robotLinkPoses(robotId, q instanceof Float64Array ? q : Float64Array.from(q));
  }

  observe(scenario, noise) {
    return JSON.parse(this.#raw.observe(text(scenario), text(noise)));
  }

  robotTcp(robotId, q) {
    return JSON.parse(this.#raw.robotTcp(robotId, q instanceof Float64Array ? q : Float64Array.from(q)));
  }

  robotIk(robotId, baseSe3Tcp, seed, options = {}) {
    const s = seed instanceof Float64Array ? seed : Float64Array.from(seed);
    return JSON.parse(this.#raw.robotIk(robotId, text(baseSe3Tcp), s, text(options)));
  }

  bake(scenario, options = {}) {
    return JSON.parse(this.#raw.bake(text(scenario), options.derivatives === true));
  }

  motionReport(scenario) {
    return JSON.parse(this.#raw.motionReport(text(scenario)));
  }

  resolveScenario(scenario) {
    return JSON.parse(this.#raw.resolveScenario(text(scenario)));
  }

  exportCaptures(scenario, robotId, format) {
    return this.#raw.exportCaptures(text(scenario), robotId, format);
  }

  generatePoses(spec, dt = 0.01) {
    return JSON.parse(this.#raw.generatePoses(text(spec), dt));
  }

  exportTrajectory(scenario, robotId) {
    return this.#raw.exportTrajectory(text(scenario), robotId);
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

  targetMesh(targetId) {
    return this.#raw.targetMesh(targetId);
  }

  targetPoints(targetId) {
    return JSON.parse(this.#raw.targetPoints(targetId));
  }

  projectTarget(cameraId, worldSe3Camera, targetId, worldSe3Target) {
    const wire = ({ rotation: q, translation: t }) => new Float64Array([q[0], q[1], q[2], q[3], t[0], t[1], t[2]]);
    return this.#raw.projectTarget(cameraId, wire(worldSe3Camera), targetId, wire(worldSe3Target));
  }

  laserStripe(cameraId, worldSe3Camera, laserId, worldSe3Laser, targetId, worldSe3Target) {
    const wire = ({ rotation: q, translation: t }) => new Float64Array([q[0], q[1], q[2], q[3], t[0], t[1], t[2]]);
    return this.#raw.laserStripe(
      cameraId,
      wire(worldSe3Camera),
      laserId,
      wire(worldSe3Laser),
      targetId,
      wire(worldSe3Target),
    );
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

export function poleCircumferences() {
  return JSON.parse(rawPoleCircumferences());
}

export function deriveOptics(optics, params) {
  return JSON.parse(rawDeriveOptics(text(optics), text(params)));
}

export function validateLibraryItem(text) {
  return JSON.parse(rawValidateLibraryItem(typeof text === "string" ? text : JSON.stringify(text)));
}

const HEAD_MAP_FIELDS = 10;

export function headMap(design, window, across, along) {
  const raw = rawHeadMap(text(design), window == null ? undefined : text(window), across, along);
  const n = across * along;
  const field = (k) => {
    const out = new Float64Array(n);
    for (let i = 0; i < n; i++) out[i] = raw[i * HEAD_MAP_FIELDS + k];
    return out;
  };
  return {
    across,
    along,
    visible: Uint8Array.from(field(0)),
    coc_px: field(1),
    airy_px: field(2),
    blur_px: field(3),
    lateral_m_per_px: field(4),
    depth_m_per_px: field(5),
    triangulation_rad: field(6),
    beam_px: field(7),
    pixel_u: field(8),
    pixel_v: field(9),
  };
}

export function rigCoverage(layout, grid, criteria = {}) {
  const raw = rawRigCoverage(text(layout), text(grid), text(criteria));
  const n = grid.n[0] * grid.n[1] * grid.n[2];
  return { grid, views: raw.subarray(0, n), lit: raw.subarray(n, 2 * n) };
}

export function headSection(design, options = {}) {
  return JSON.parse(rawHeadSection(text(design), text(options)));
}

export function headScheimpflug(design) {
  return JSON.parse(rawHeadScheimpflug(text(design)));
}

export function headOptimise(design, search) {
  return JSON.parse(rawHeadOptimise(text(design), text(search)));
}

export function headApply(design, setting) {
  return JSON.parse(rawHeadApply(text(design), text(setting)));
}

export function expandRig(layout, rigId) {
  return JSON.parse(rawExpandRig(text(layout), rigId));
}

export function isEtendueError(error) {
  return error instanceof Error && typeof error.kind === "string";
}
