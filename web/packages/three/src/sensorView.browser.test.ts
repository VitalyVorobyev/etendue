import { DoubleSide, Matrix4, Mesh, MeshBasicMaterial, PlaneGeometry, Scene, WebGLRenderTarget, WebGLRenderer } from "three";
import { afterAll, describe, expect, it } from "vitest";

import { GIZMO_LAYER } from "./layers";
import { type RemapTable, SensorView } from "./sensorView";

const W = 64;
const H = 48;
const renderer = new WebGLRenderer();
afterAll(() => renderer.dispose());

/**
 * A CV camera at the origin looking down +Z at z = 1: red where x < 0 (image left), green
 * where x > 0; with y < 0 (image top) blue over the red half. A gizmo-layer quad covers
 * everything and must not show.
 */
function world(): Scene {
  const scene = new Scene();
  const quad = (color: string, x: number, y: number, w: number, h: number, z = 1) => {
    const m = new Mesh(new PlaneGeometry(w, h), new MeshBasicMaterial({ color, side: DoubleSide }));
    m.position.set(x, y, z);
    scene.add(m);
    return m;
  };
  quad("red", -1, 0.5, 2, 1);
  quad("blue", -1, -0.5, 2, 1);
  quad("lime", 1, 0, 2, 2);
  quad("white", 0, 0, 10, 10, 0.5).layers.set(GIZMO_LAYER);
  return scene;
}

function lut(map: (i: number, j: number) => [number, number]): RemapTable {
  const data = new Float32Array(2 * W * H);
  for (let j = 0; j < H; j++) for (let i = 0; i < W; i++) data.set(map(i, j), 2 * (j * W + i));
  return { width: W, height: H, data, pixelCentre: "integer" };
}

/** RGB of target pixel (i, j), row 0 at the top. */
function render(table: RemapTable): (i: number, j: number) => [number, number, number] {
  const view = new SensorView({ canonical: { width: W, height: H, focalPx: 40 }, lut: table, samples: 0 });
  const target = new WebGLRenderTarget(W, H);
  view.render(renderer, world(), new Matrix4(), target);
  const px = new Uint8Array(4 * W * H);
  renderer.readRenderTargetPixels(target, 0, 0, W, H, px);
  view.dispose();
  target.dispose();
  return (i, j) => {
    const k = 4 * ((H - 1 - j) * W + i);
    return [px[k]!, px[k + 1]!, px[k + 2]!];
  };
}

const RED = [255, 0, 0];
const BLUE = [0, 0, 255];
const GREEN = [0, 255, 0];

describe("SensorView", () => {
  it("an identity LUT shows the canonical view in CV orientation, gizmos excluded", () => {
    const at = render(lut((i, j) => [i, j]));
    expect(at(8, 40)).toEqual(RED); // left, bottom
    expect(at(8, 6)).toEqual(BLUE); // left, top
    expect(at(56, 24)).toEqual(GREEN); // right
  });

  it("samples wherever the LUT points", () => {
    // Mirror left-right: the left of the output shows the right of the canonical view.
    const at = render(lut((i, j) => [W - 1 - i, j]));
    expect(at(8, 24)).toEqual(GREEN);
    expect(at(56, 40)).toEqual(RED);
    expect(at(56, 6)).toEqual(BLUE);
  });

  it("paints the background where the LUT has no entry", () => {
    const view = new SensorView({
      canonical: { width: W, height: H, focalPx: 40 },
      lut: lut((i, j) => (i < 32 ? [Number.NaN, Number.NaN] : [i, j])),
      samples: 0,
    });
    view.setBackground("magenta");
    const target = new WebGLRenderTarget(W, H);
    view.render(renderer, world(), new Matrix4(), target);
    const px = new Uint8Array(4);
    renderer.readRenderTargetPixels(target, 4, 24, 1, 1, px);
    expect([px[0], px[1], px[2]]).toEqual([255, 0, 255]);
    view.dispose();
    target.dispose();
  });

  it("box-filters the footprint when asked", () => {
    // A 2× canonical view of the same world through a LUT scaled by 2: with 2 × 2 taps the
    // output still shows each half in its colour, averaged over the pixel.
    const view = new SensorView({
      canonical: { width: 2 * W, height: 2 * H, focalPx: 80 },
      lut: lut((i, j) => [2 * i + 0.5, 2 * j + 0.5]),
      samples: 0,
      taps: 2,
    });
    const target = new WebGLRenderTarget(W, H);
    view.render(renderer, world(), new Matrix4(), target);
    const px = new Uint8Array(4);
    renderer.readRenderTargetPixels(target, 56, 24, 1, 1, px);
    expect([px[0], px[1], px[2]]).toEqual([0, 255, 0]);
    view.dispose();
    target.dispose();
  });

  it("rejects a LUT of the wrong size", () => {
    expect(
      () =>
        new SensorView({
          canonical: { width: W, height: H, focalPx: 40 },
          lut: { width: W, height: H, data: new Float32Array(3), pixelCentre: "half" },
        }),
    ).toThrow(/LUT has 3 values/);
  });
});
