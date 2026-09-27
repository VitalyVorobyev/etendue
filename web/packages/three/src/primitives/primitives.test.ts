import { Color, type LineBasicMaterial, type Mesh, type MeshBasicMaterial, Raycaster, Vector3 } from "three";
import { describe, expect, it } from "vitest";

import { GIZMO_LAYER } from "../layers";
import { Axes } from "./axes";
import { CameraFrustum, imageBorderPixels } from "./frustum";
import { LaserFan } from "./laserFan";
import { LightGizmo } from "./lightGizmo";
import { TargetBoard } from "./targetBoard";

const positions = (m: { geometry: { getAttribute(n: string): { array: ArrayLike<number> } } }) =>
  Array.from(m.geometry.getAttribute("position").array);

/** Border rays of an ideal pinhole with a 90° × 90° field of view. */
function squareRays(perEdge: number): Float64Array {
  const px = imageBorderPixels(2, 2, perEdge);
  const rays = new Float64Array((px.length / 2) * 3);
  for (let i = 0; i < px.length / 2; i++) rays.set([px[2 * i]! - 1, px[2 * i + 1]! - 1, 1], 3 * i);
  return rays;
}

describe("imageBorderPixels", () => {
  it("walks the border clockwise from the origin with corners every perEdge points", () => {
    const px = imageBorderPixels(4, 2, 2);
    expect(Array.from(px)).toEqual([0, 0, 2, 0, 4, 0, 4, 1, 4, 2, 2, 2, 0, 2, 0, 1]);
    expect(imageBorderPixels(4, 2, 0).length).toBe(8);
  });
});

describe("CameraFrustum", () => {
  it("scales the border rays to the far depth and picks through its hull", () => {
    const f = new CameraFrustum({ borderRays: squareRays(2), depth: 2, color: "red" });
    const lines = f.children[0] as Mesh;
    const pts = positions(lines);
    // 4 apex edges then 8 border segments.
    expect(pts.length).toBe((4 + 8) * 6);
    expect(pts.slice(3, 6)).toEqual([-2, -2, 2]);
    const ray = new Raycaster(new Vector3(0, 0, -1), new Vector3(0, 0, 1));
    // Gizmos live on GIZMO_LAYER: a default raycaster (layer 0) does not see them.
    expect(ray.intersectObject(f, true)).toHaveLength(0);
    ray.layers.enable(GIZMO_LAYER);
    const hits = ray.intersectObject(f, true);
    expect(hits.length).toBeGreaterThan(0);
    expect(hits.every((h) => h.object === f.hitbox || h.object.parent === f.hitbox)).toBe(true);
  });

  it("toggles emphasis and colour", () => {
    const f = new CameraFrustum({ borderRays: squareRays(1), depth: 1, color: "red", apexEdges: [0, 2] });
    const lineMat = (f.children[0] as Mesh).material as LineBasicMaterial;
    expect(f.active).toBe(false);
    const dim = lineMat.opacity;
    f.setActive(true);
    expect(f.active).toBe(true);
    expect(lineMat.opacity).toBeGreaterThan(dim);
    f.setColor("blue");
    expect(lineMat.color.equals(new Color("blue"))).toBe(true);
    expect(positions(f.children[0] as Mesh).length).toBe((2 + 4) * 6);
  });

  it("pads its pick hull and makes the optical centre pickable", () => {
    const f = new CameraFrustum({ borderRays: squareRays(2), depth: 1, color: "red", pickPadding: 1.5 });
    const ray = new Raycaster(new Vector3(1.2, 0, -1), new Vector3(0, 0, 1));
    ray.layers.enable(GIZMO_LAYER);
    // x = 1.2 is outside the drawn far outline (±1) but inside the padded hull (±1.5).
    expect(ray.intersectObject(f, true).length).toBeGreaterThan(0);
    const side = new Raycaster(new Vector3(-1, 0, 0), new Vector3(1, 0, 0));
    side.layers.enable(GIZMO_LAYER);
    expect(side.intersectObject(f, true).some((h) => h.object.name === "hitbox-apex")).toBe(true);
  });

  it("needs a polygon", () => {
    expect(() => new CameraFrustum({ borderRays: [0, 0, 1, 1, 0, 1], depth: 1, color: "red" })).toThrow();
  });
});

describe("LaserFan", () => {
  it("lies in the x = 0 plane, symmetric about +Z, within its length", () => {
    const fan = new LaserFan({ halfAngle: Math.PI / 6, length: 0.5, color: "red", segments: 4 });
    const pts = positions(fan.children[0] as Mesh);
    for (let i = 0; i < pts.length; i += 3) {
      expect(pts[i]).toBe(0);
      expect(Math.hypot(pts[i + 1]!, pts[i + 2]!)).toBeLessThanOrEqual(0.5 + 1e-6);
      expect(pts[i + 2]!).toBeGreaterThanOrEqual(0);
    }
    const ys = pts.filter((_, i) => i % 3 === 1);
    expect(Math.min(...ys)).toBeCloseTo(-0.25, 6);
    expect(Math.max(...ys)).toBeCloseTo(0.25, 6);
    fan.setColor("green");
    const sheet = (fan.children[0] as Mesh).material as MeshBasicMaterial;
    const idle = sheet.opacity;
    fan.setActive(true);
    expect(fan.active).toBe(true);
    expect(sheet.opacity).toBeGreaterThan(idle);
    expect(((fan.children[0] as Mesh).material as MeshBasicMaterial).color.equals(new Color("green"))).toBe(true);
  });
});

describe("outlines are not pickable", () => {
  it("picks the board surface and the fan sheet, never their outlines", () => {
    const board = new TargetBoard({ width: 1, height: 1, color: "white", edgeColor: "black" });
    const fan = new LaserFan({ halfAngle: 0.5, length: 1, color: "red" });
    fan.layers.enableAll();
    // A ray that passes 0.5 m from the board's outline but misses its surface.
    const miss = new Raycaster(new Vector3(1, 0, 1), new Vector3(0, 0, -1));
    miss.layers.enableAll();
    expect(miss.intersectObject(board, true)).toHaveLength(0);
    expect(miss.intersectObject(fan, true)).toHaveLength(0);
    const hit = new Raycaster(new Vector3(0, 0, 1), new Vector3(0, 0, -1));
    expect(hit.intersectObject(board, true).length).toBeGreaterThan(0);
  });
});

describe("TargetBoard", () => {
  it("is centred in z = 0 with the dark square at −X/−Y", () => {
    const b = new TargetBoard({ width: 0.4, height: 0.2, color: "white", edgeColor: "black", checker: { cols: 4, rows: 2 } });
    expect(b.children).toHaveLength(3);
    const base = positions(b.children[0] as Mesh);
    expect(Math.min(...base.filter((_, i) => i % 3 === 0))).toBeCloseTo(-0.2, 6);
    expect(base.filter((_, i) => i % 3 === 2).every((z) => z === 0)).toBe(true);
    const dark = positions(b.children[1] as Mesh);
    // 4 of 8 squares, 6 vertices each; the first starts at the −X/−Y corner.
    expect(dark.length).toBe(4 * 6 * 3);
    expect(dark[0]).toBeCloseTo(-0.2, 6);
    expect(dark[1]).toBeCloseTo(-0.1, 6);
    b.setColors("gray", "red");
    expect(b.active).toBe(false);
    b.setActive(true);
    expect(b.active).toBe(true);
    b.setOpacity(0.5);
    expect(((b.children[0] as Mesh).material as MeshBasicMaterial).transparent).toBe(true);
    b.setOpacity(1);
    expect(((b.children[0] as Mesh).material as MeshBasicMaterial).transparent).toBe(false);
    expect(((b.children[1] as Mesh).material as MeshBasicMaterial).color.equals(new Color("red"))).toBe(true);
  });

  it("draws a plain surface without a checker", () => {
    expect(new TargetBoard({ width: 1, height: 1, color: "white", edgeColor: "black" }).children).toHaveLength(2);
  });
});

describe("LightGizmo", () => {
  it("draws each shape", () => {
    for (const shape of [
      { type: "point", radius_m: 0.01 },
      { type: "spot", cone_angle: 0.5 },
      { type: "area", size_m: [0.2, 0.1] },
    ] as const) {
      const g = new LightGizmo(shape, "orange");
      expect(positions(g.children[0] as Mesh).length).toBeGreaterThan(0);
      g.setColor("yellow");
      expect(((g.children[0] as Mesh).material as LineBasicMaterial).color.equals(new Color("yellow"))).toBe(true);
    }
  });
});

describe("Axes", () => {
  it("colours each axis", () => {
    const a = new Axes(0.1, { x: "red", y: "lime", z: "blue" });
    const c = a.geometry.getAttribute("color");
    expect([c.getX(0), c.getY(0), c.getZ(0)]).toEqual([1, 0, 0]);
    expect([c.getX(5), c.getY(5), c.getZ(5)]).toEqual([0, 0, 1]);
    expect(positions(a).slice(3, 6)).toEqual([0.1, 0, 0].map((v) => Math.fround(v)));
  });
});
