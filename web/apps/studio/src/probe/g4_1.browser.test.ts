/**
 * Gate G4.1, web backend (docs/pivot/PLAN.md P4-2): small emissive spheres at known 3D points,
 * rendered by `SensorView` (canonical pinhole + remap LUT from `@etendue/wasm`), must image
 * with their intensity-weighted centroid within 0.01 px of the kernel's projection of their
 * centre — under both pixel-centre conventions, each read out in its own. The Blender side is
 * `etendue measure g4-1`; the probe design (sphere profile, sizes) matches it.
 */

import init, { type CameraParams, EtendueScene, type PixelCentre } from "@etendue/wasm";
import { SensorView } from "@vitavision/three";
import {
  FloatType,
  Matrix4,
  Mesh,
  Scene,
  ShaderMaterial,
  SphereGeometry,
  WebGLRenderTarget,
  WebGLRenderer,
} from "three";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

const W = 1280;
const H = 1024;
const DEPTH = 0.5;
const RADIUS = 0.001;
const WINDOW = 10;
const SUPERSAMPLE = 4;
const GATE_PX = 0.01;

const I = { rotation: [0, 0, 0, 1], translation: [0, 0, 0] };
const K = (fx: number, fy: number, cx: number, cy: number, skew: number) =>
  ({ type: "fx_fy_cx_cy_skew", fx, fy, cx, cy, skew }) as const;
const brown = (k1: number, k2: number, p1: number, p2: number) =>
  ({ type: "brown_conrady5", k1, k2, k3: 0, p1, p2, iters: 8 }) as const;

const CAMERAS: Record<string, CameraParams> = {
  brown: {
    projection: { type: "pinhole" },
    distortion: brown(-0.08, 0.02, 0, 0),
    sensor: { type: "identity" },
    intrinsics: K(2318.8, 2318.8, 640, 512, 0),
  } as unknown as CameraParams,
  offcentre_skew_tilt: {
    projection: { type: "pinhole" },
    distortion: brown(-0.05, 0.01, 2e-4, -1e-4),
    sensor: { type: "scheimpflug", tilt_x: (2 * Math.PI) / 180, tilt_y: -Math.PI / 180 },
    intrinsics: K(2300, 2296, 612.3, 541.7, 0.6),
  } as unknown as CameraParams,
};

/** Emission (N·V)², the Blender probe's profile: a smooth blob, zero at the limb. */
const emitter = new ShaderMaterial({
  vertexShader: /* glsl */ `
    varying vec3 vN; varying vec3 vV;
    void main() {
      vec4 p = modelViewMatrix * vec4(position, 1.0);
      vN = normalize(normalMatrix * normal); vV = normalize(-p.xyz);
      gl_Position = projectionMatrix * p;
    }`,
  fragmentShader: /* glsl */ `
    varying vec3 vN; varying vec3 vV;
    void main() { float c = max(dot(normalize(vN), normalize(vV)), 0.0); gl_FragColor = vec4(vec3(c * c), 1.0); }`,
});

let renderer: WebGLRenderer;
let scene: EtendueScene;

beforeAll(async () => {
  await init();
  scene = new EtendueScene(
    {
      version: 1,
      cameras: Object.entries(CAMERAS).map(([id, params]) => ({
        id,
        parent: "world",
        parent_se3_self: I,
        params,
        resolution: [W, H],
      })),
    } as never,
    [],
  );
  renderer = new WebGLRenderer();
});

afterAll(() => {
  renderer.dispose();
  scene.free();
});

function probe(camera: string, centre: PixelCentre) {
  // Sphere centres: a 7 × 5 grid of off-centre pixels back-projected to DEPTH.
  const px: number[] = [];
  for (let r = 0; r < 5; r++) {
    for (let c = 0; c < 7; c++) px.push(90 + ((W - 180) * c) / 6 + 0.37, 90 + ((H - 180) * r) / 4 + 0.21);
  }
  const rays = scene.backprojectPixels(camera, px);
  const centres: number[] = [];
  for (let i = 0; i < rays.length; i += 3) centres.push(rays[i]! * DEPTH, rays[i + 1]! * DEPTH, DEPTH);

  const world = new Scene();
  const geometry = new SphereGeometry(RADIUS, 48, 24);
  for (let i = 0; i < centres.length; i += 3) {
    const m = new Mesh(geometry, emitter);
    m.position.set(centres[i]!, centres[i + 1]!, centres[i + 2]);
    world.add(m);
  }

  const { canonical, lut } = scene.remap(camera, { supersample: SUPERSAMPLE }, centre);
  const view = new SensorView({
    canonical: {
      width: canonical.resolution[0],
      height: canonical.resolution[1],
      focalPx: (canonical.params.intrinsics as { fx: number }).fx,
    },
    lut: { width: W, height: H, data: lut, pixelCentre: centre },
    samples: 4,
    precision: "half",
    taps: SUPERSAMPLE,
  });
  const target = new WebGLRenderTarget(W, H, { type: FloatType });
  view.render(renderer, world, new Matrix4(), target);
  const rgba = new Float32Array(4 * W * H);
  renderer.readRenderTargetPixels(target, 0, 0, W, H, rgba);
  view.dispose();
  target.dispose();
  geometry.dispose();

  const off = centre === "integer" ? 0 : 0.5;
  // Rows come bottom-up from readPixels; image row j is buffer row H − 1 − j.
  const lum = (i: number, j: number) => {
    const k = 4 * ((H - 1 - j) * W + i);
    return rgba[k]! + rgba[k + 1]! + rgba[k + 2]!;
  };
  const projected = scene.projectPoints(camera, I as never, centres);
  const errors: [number, number][] = [];
  for (let s = 0; s < projected.length; s += 2) {
    const [u, v] = [projected[s]!, projected[s + 1]!];
    const [ci, cj] = [Math.round(u - off), Math.round(v - off)];
    let [sx, sy, sum] = [0, 0, 0];
    for (let j = cj - WINDOW; j <= cj + WINDOW; j++) {
      for (let i = ci - WINDOW; i <= ci + WINDOW; i++) {
        const y = lum(i, j);
        sx += y * (i + off);
        sy += y * (j + off);
        sum += y;
      }
    }
    errors.push([sx / sum - u, sy / sum - v]);
  }
  const n = errors.length;
  const mean = [errors.reduce((a, e) => a + e[0], 0) / n, errors.reduce((a, e) => a + e[1], 0) / n];
  const max = Math.max(...errors.map(([x, y]) => Math.hypot(x, y)));
  const rms = Math.sqrt(errors.reduce((a, [x, y]) => a + x * x + y * y, 0) / n);
  console.log(
    `G4.1 web ${camera} ${centre}: mean (${mean[0]!.toFixed(4)}, ${mean[1]!.toFixed(4)}) px, rms ${rms.toFixed(4)} px, max ${max.toFixed(4)} px`,
  );
  return { max, rms, mean };
}

describe("G4.1 web backend (SensorView)", () => {
  for (const camera of Object.keys(CAMERAS)) {
    for (const centre of ["integer", "half"] as const) {
      it(`${camera}, ${centre} pixel centres`, () => {
        const { max } = probe(camera, centre);
        expect(max).toBeLessThanOrEqual(GATE_PX);
      });
    }
  }
});
