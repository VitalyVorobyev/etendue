import { describe, expect, it, vi } from "vitest";

// The kernel derives the params; here only the design's geometry is under test.
vi.mock("@etendue/wasm", () => ({ deriveOptics: (_: unknown, params: unknown) => ({ params }) }));

const { aimDistance, fromItem, initialHead, laserParams, laserPose, toItem, windowRange, withLens } =
  await import("./design");

describe("head designs", () => {
  it("place the laser off the axis, aimed at it, its line along camera x", () => {
    const p = { ...initialHead(null), baseline_m: 0.1, height_m: 0.05, aim_m: 0.35 };
    const { rotation: q, translation: t } = laserPose(p);
    expect(t).toEqual([0, -0.1, 0.05]);
    // The central ray (laser +z) from the origin passes through the aim point on the axis.
    const [x, y, z, w] = q;
    const central = [2 * (x * z + w * y), 2 * (y * z - w * x), 1 - 2 * (x * x + y * y)];
    const s = (0 - t[1]) / central[1]!;
    expect(t[0] + s * central[0]!).toBeCloseTo(0, 12);
    expect(t[2] + s * central[2]!).toBeCloseTo(0.35, 12);
    const [near, far] = windowRange({ ...p, depth_m: 0.1 });
    expect((near + far) / 2).toBeCloseTo(Math.hypot(0.1, 0.3), 12);
    expect(far - near).toBeCloseTo(0.1, 12);
  });

  it("survive the library round trip", () => {
    const p = {
      ...initialHead(null),
      baseline_m: 0.12,
      height_m: -0.02,
      aim_m: 0.6,
      depth_m: 0.2,
      width_m: 0.05,
      tilt: [0.1, -0.02] as [number, number],
    };
    const back = fromItem(toItem(p))!;
    for (const key of [
      "baseline_m",
      "height_m",
      "aim_m",
      "depth_m",
      "width_m",
      "focus_distance_m",
      "f_number",
    ] as const) {
      expect(back[key]).toBeCloseTo(p[key], 12);
    }
    expect(back.tilt).toEqual(p.tilt);
    expect(back.laser.waist_distance_m).toBe(p.laser.waist_distance_m);
  });

  it("take a library lens, keeping the f-number within it", () => {
    const head = { ...initialHead(null), f_number: 2 };
    const q = withLens(head, "50 mm", {
      mount: "c_mount",
      focal_length_m: 0.05,
      min_f_number: 2.8,
      principal_gap_m: 0.004,
    });
    expect(q.lens).toEqual({
      name: "50 mm",
      focal_length_m: 0.05,
      min_f_number: 2.8,
      principal_gap_m: 0.004,
      entrance_pupil_m: 0,
    });
    expect(q.f_number).toBe(2.8);
    expect(
      withLens({ ...head, f_number: 8 }, "50 mm", { mount: "c_mount", focal_length_m: 0.05, min_f_number: 2.8 })
        .f_number,
    ).toBe(8);
  });

  it("focus a laser part at the aim point unless it says where it is focused", () => {
    const head = initialHead(null);
    expect(head.laser.waist_distance_m).toBeCloseTo(aimDistance(head), 12);
    const part = { wavelength_nm: 660, fan_half_angle: 0.3, beam_waist_m: 5e-5 };
    const place = { baseline_m: 0.2, height_m: 0, aim_m: 0.5 };
    expect(laserParams({ name: "l", laser: part }, place).waist_distance_m).toBeCloseTo(Math.hypot(0.2, 0.5), 12);
    expect(laserParams({ name: "l", laser: { ...part, waist_distance_m: 0 } }, place).waist_distance_m).toBe(0);
  });
});
