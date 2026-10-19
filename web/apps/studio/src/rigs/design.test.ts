import { describe, expect, it, vi } from "vitest";

import type { RigParams } from "./design";

// The kernel derives the params; here only the layout is under test.
vi.mock("@etendue/wasm", () => ({ deriveOptics: (_: unknown, params: unknown) => ({ params }) }));

const { fromItem, toItem, toLayout } = await import("./design");

const camera = {
  params: {
    projection: { type: "pinhole" as const },
    distortion: { type: "none" as const },
    sensor: { type: "identity" as const },
    intrinsics: { type: "fx_fy_cx_cy_skew" as const, fx: 1, fy: 1, cx: 640, cy: 512, skew: 0 },
  },
  resolution: [1280, 1024] as [number, number],
  optics: {
    sensor: { pixel_pitch_m: 5e-6 },
    lens: { focal_length_m: 0.025, f_number: 4 },
    focus_distance_m: 1,
  },
};

const stereo: RigParams = {
  name: "Pair",
  id: "pair",
  unit: { kind: "camera", name: "cam", camera, laser: null },
  arrangement: "stereo",
  count: 5,
  radius_m: 0.03,
  converge: true,
  working_distance_m: 0.4,
  tilt_rad: 0,
  phase_rad: 1,
  refocus: true,
  image_x: "radial",
  box: { half_m: 0.1, near_m: 0.3, far_m: 0.5 },
  max_blur_px: 2,
  resolution: 32,
};

describe("rig designs", () => {
  it("lay a stereo pair out on the rig's x axis, refocused on the working point", () => {
    const layout = toLayout(stereo);
    expect(layout).toMatchObject({ count: 2, image_x: "common", phase_rad: 0, radius_m: 0.03 });
    expect(layout.tilt_rad).toBeUndefined();
    // The camera axis crosses the working distance 0.4 m down the rig axis at hypot(ρ, d).
    expect(layout.camera.optics!.focus_distance_m).toBeCloseTo(Math.hypot(0.03, 0.4), 12);
    expect(toLayout({ ...stereo, refocus: false }).camera.optics!.focus_distance_m).toBe(1);
    expect(toLayout({ ...stereo, converge: false, tilt_rad: 0 }).tilt_rad).toBe(0);
  });

  it("survive the library round trip", () => {
    const ring: RigParams = { ...stereo, arrangement: "ring", count: 4, converge: false, tilt_rad: 0.2 };
    const back = fromItem(toItem(ring))!;
    expect(back).toMatchObject({ arrangement: "ring", count: 4, converge: false, tilt_rad: 0.2, image_x: "radial" });
    expect(fromItem(toItem(stereo))!.arrangement).toBe("stereo");
  });
});
