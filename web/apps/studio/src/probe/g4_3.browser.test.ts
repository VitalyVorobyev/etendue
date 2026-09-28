/**
 * Gate G4.3, cross-backend agreement (docs/pivot/PLAN.md P4-4): the same board, camera and
 * frames rendered by Blender (`etendue measure g4-2`) and by the web `SensorView`, run
 * through the same detector (chess-corners 1.2.0: the Rust crate there, its wasm build
 * `@vitavision/chess-corners` here), must give corners that differ by ≤ 0.05 px RMS.
 *
 * The scene comes from the Blender run's `target/g4_2/detections.json` (poses, camera,
 * board, ground truth, and Blender's per-corner errors), so both sides render one
 * definition. Without that file (CI has no Blender) the suite is skipped.
 */

import init, { type CameraParams, EtendueScene } from "@etendue/wasm";
import initDetector, {
  CenterOfMassConfig,
  ChessDetector,
  ChessRefiner,
  DetectorConfig,
  ForstnerConfig,
  SaddlePointConfig,
} from "@vitavision/chess-corners";
import { SensorView, matrixFromIso3 } from "@vitavision/three";
import {
  DoubleSide,
  FloatType,
  Group,
  Matrix4,
  Mesh,
  PlaneGeometry,
  Scene,
  ShaderMaterial,
  WebGLRenderTarget,
  WebGLRenderer,
} from "three";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

interface Iso3 {
  rotation: [number, number, number, number];
  translation: [number, number, number];
}

interface Detections {
  resolution: [number, number];
  camera: CameraParams;
  squares: [number, number];
  square_m: number;
  radiance: [number, number, number];
  chess_threshold: number;
  margin_px: number;
  match_px: number;
  poses: { name: string; camera_se3_target: Iso3 }[];
  /** Visible ground-truth corners per pose, in pose order. */
  truth: [number, number][][];
  /** Blender per-corner errors (detection − truth, or null) by supersampling, then refiner. */
  errors: Record<string, Record<string, ([number, number] | null)[]>>;
}

const FILE = import.meta.glob<string>("../../../../../target/g4_2/detections.json", {
  query: "?raw",
  import: "default",
  eager: true,
});
const text = Object.values(FILE)[0];
const data = text === undefined ? undefined : (JSON.parse(text) as Detections);

const SUPERSAMPLE = 4;
const GATE_PX = 0.05;
const REFINERS = ["center_of_mass", "forstner", "saddle_point"] as const;

/** Unlit constant radiance: what the Blender scene gives each surface under its white sky. */
const radiance = (value: number) =>
  new ShaderMaterial({
    side: DoubleSide,
    vertexShader: /* glsl */ `void main() { gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0); }`,
    fragmentShader: /* glsl */ `void main() { gl_FragColor = vec4(vec3(${value.toFixed(6)}), 1.0); }`,
  });

function detector(refiner: (typeof REFINERS)[number], threshold: number): ChessDetector {
  const r =
    refiner === "forstner"
      ? ChessRefiner.withForstner(new ForstnerConfig())
      : refiner === "saddle_point"
        ? ChessRefiner.withSaddlePoint(new SaddlePointConfig())
        : ChessRefiner.withCenterOfMass(new CenterOfMassConfig());
  return ChessDetector.withConfig(DetectorConfig.chess().withThreshold(threshold).withChessRefiner(r));
}

describe("G4.3 cross-backend agreement (web SensorView vs Blender)", () => {
  if (data === undefined) {
    it.skip("needs target/g4_2/detections.json from `etendue measure g4-2` (local, Blender)", () => {});
    return;
  }
  const d = data;
  const [W, H] = d.resolution;
  let renderer: WebGLRenderer;
  let scene: EtendueScene;
  /** 8-bit linear luminance per pose, as `etendue measure g4-2` feeds its detector. */
  const images: Uint8Array[] = [];

  beforeAll(async () => {
    await init();
    await initDetector();
    const I = { rotation: [0, 0, 0, 1], translation: [0, 0, 0] };
    scene = new EtendueScene(
      {
        version: 1,
        cameras: [{ id: "cam", parent: "world", parent_se3_self: I, params: d.camera, resolution: [W, H] }],
      } as never,
      [],
    );
    renderer = new WebGLRenderer();
    const { canonical, lut } = scene.remap("cam", { supersample: SUPERSAMPLE }, "integer");
    const view = new SensorView({
      canonical: {
        width: canonical.resolution[0],
        height: canonical.resolution[1],
        focalPx: (canonical.params.intrinsics as { fx: number }).fx,
      },
      lut: { width: W, height: H, data: lut, pixelCentre: "integer" },
      samples: 4,
      precision: "half",
      taps: SUPERSAMPLE,
    });
    const [cols, rows] = d.squares;
    const s = d.square_m;
    const [background, light, dark] = d.radiance;
    const lightMat = radiance(light);
    const darkMat = radiance(dark);
    const square = new PlaneGeometry(s, s);
    // The camera sits at the world origin in CV axes; a far white wall is the sky.
    const wall = new Mesh(new PlaneGeometry(40, 40), radiance(background));
    wall.position.set(0, 0, 8);
    const target = new WebGLRenderTarget(W, H, { type: FloatType });
    const rgba = new Float32Array(4 * W * H);
    for (const pose of d.poses) {
      const world = new Scene();
      world.add(wall);
      // Squares in the target frame: centred board, columns along X, dark where column + row
      // is even counting rows from the print's top edge at +Y — calib-targets' print, which the
      // job's board is built from (etendue_synth::board).
      const board = new Group();
      board.matrixAutoUpdate = false;
      board.matrix.copy(matrixFromIso3(pose.camera_se3_target));
      for (let r = 0; r < rows; r++) {
        for (let c = 0; c < cols; c++) {
          const m = new Mesh(square, (rows - 1 - r + c) % 2 === 0 ? darkMat : lightMat);
          m.position.set(-(cols * s) / 2 + (c + 0.5) * s, -(rows * s) / 2 + (r + 0.5) * s, 0);
          board.add(m);
        }
      }
      world.add(board);
      view.render(renderer, world, new Matrix4(), target);
      renderer.readRenderTargetPixels(target, 0, 0, W, H, rgba);
      // Rows come bottom-up from readPixels; image row j is buffer row H − 1 − j.
      const lum = new Float32Array(W * H);
      let max = 1e-6;
      for (let j = 0; j < H; j++) {
        for (let i = 0; i < W; i++) {
          const k = 4 * ((H - 1 - j) * W + i);
          const y = 0.2126 * rgba[k]! + 0.7152 * rgba[k + 1]! + 0.0722 * rgba[k + 2]!;
          lum[j * W + i] = y;
          max = Math.max(max, y);
        }
      }
      images.push(Uint8Array.from(lum, (y) => Math.round(Math.min(Math.max(y / max, 0), 1) * 240)));
    }
    view.dispose();
    target.dispose();
    square.dispose();
    lightMat.dispose();
    darkMat.dispose();
  });

  afterAll(() => {
    renderer?.dispose();
    scene?.free();
  });

  for (const refiner of REFINERS) {
    it(`${refiner}: web vs Blender (s = ${SUPERSAMPLE}) within ${GATE_PX} px RMS`, () => {
      const det = detector(refiner, d.chess_threshold);
      const web: ([number, number] | null)[] = [];
      d.truth.forEach((truth, p) => {
        const found = det.detect(images[p]!, W, H);
        for (const [u, v] of truth) {
          let best: [number, number] | null = null;
          for (let k = 0; k < found.length; k += 7) {
            const e: [number, number] = [found[k]! - u, found[k + 1]! - v];
            if (Math.hypot(...e) <= d.match_px && (best === null || Math.hypot(...e) < Math.hypot(...best))) best = e;
          }
          web.push(best);
        }
      });
      det.free();
      const blender = d.errors[String(SUPERSAMPLE)]![refiner]!;
      expect(blender.length).toBe(web.length);
      const diffs: number[] = [];
      const webErr: number[] = [];
      web.forEach((w, k) => {
        const b = blender[k];
        if (w !== null) webErr.push(Math.hypot(...w));
        if (w !== null && b !== null && b !== undefined) diffs.push(Math.hypot(w[0] - b[0], w[1] - b[1]));
      });
      const rms = (xs: number[]) => Math.sqrt(xs.reduce((a, x) => a + x * x, 0) / Math.max(xs.length, 1));
      const matched = web.filter((w) => w !== null).length;
      console.log(
        `G4.3 ${refiner}: web matched ${matched}/${web.length}, web vs GT RMS ${rms(webErr).toFixed(4)} px; ` +
          `web vs Blender over ${diffs.length} corners RMS ${rms(diffs).toFixed(4)} px, max ${Math.max(...diffs).toFixed(4)} px`,
      );
      expect(matched).toBe(web.length);
      expect(rms(diffs)).toBeLessThanOrEqual(GATE_PX);
    });
  }
});
