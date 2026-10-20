/**
 * A ring of cameras converging on a working point: how many see each point sharply, and where a
 * camera sees its own laser line (`rigCoverage`, `rigSharpness`), in a side and a top slice.
 */

import { type RigCoverage, type RigLayout, deriveOptics, rigCoverage, rigSharpness } from "@etendue/wasm";

import { kernel } from "../kernel";
import { heatCanvas, viridis } from "../plot";
import { card, el, figure, fmt, legend, perFrame, readouts, select, setDrawing, showError, slider } from "../ui";

interface Ring {
  count: number;
  radius_m: number;
  working_distance_m: number;
  f_number: number;
  max_blur_px: number;
  lasers: boolean;
}

const START: Ring = { count: 3, radius_m: 0.08, working_distance_m: 0.5, f_number: 8, max_blur_px: 3, lasers: true };
const RES: [number, number] = [1936, 1216];
const N = 96;
/** The slices' half-size across the axis and half-depth along it (m). */
const HALF = 0.15;
const DEPTH = 0.1;

/** The ring's layout: IMX174 cameras behind 25 mm lenses, focused on the working point. */
export function layoutOf(r: Ring): RigLayout {
  const optics = {
    sensor: { pixel_pitch_m: 5.86e-6 },
    lens: { focal_length_m: 0.025, f_number: r.f_number },
    focus_distance_m: Math.hypot(r.radius_m, r.working_distance_m),
  };
  const { params } = deriveOptics(optics, {
    projection: { type: "pinhole" },
    distortion: { type: "none" },
    sensor: { type: "identity" },
    intrinsics: { type: "fx_fy_cx_cy_skew", fx: 1, fy: 1, cx: RES[0] / 2, cy: RES[1] / 2, skew: 0 },
  });
  return {
    count: r.count,
    radius_m: r.count === 1 ? 0 : r.radius_m,
    working_distance_m: r.working_distance_m,
    camera: { params, resolution: RES, optics },
    ...(r.lasers
      ? {
          lasers: {
            radius_m: r.count === 1 ? 0.05 : r.radius_m / 2,
            azimuth_rad: r.count === 1 ? 0 : Math.PI / r.count,
            fan_half_angle: 0.44,
            fan_length: 1.5,
            wavelength_nm: 660,
            beam_waist_m: 0.0005,
          },
        }
      : {}),
  };
}

/** Colour by camera count (0 none), with lit voxels in orange. */
function paint(
  cov: RigCoverage,
  count: number,
): { values: Float64Array; colour: (v: number) => [number, number, number] } {
  const values = new Float64Array(cov.views.length);
  for (let i = 0; i < values.length; i++) {
    values[i] = cov.lit[i]! > 0 ? -1 : cov.views[i]! > 0 ? cov.views[i]! : NaN;
  }
  return {
    values,
    colour: (v) => (v < 0 ? [240, 120, 40] : viridis(count <= 1 ? 0.6 : (v - 1) / (count - 1))),
  };
}

export default async function coverage(host: HTMLElement): Promise<void> {
  await kernel();
  const c = card(host, "Rig coverage: where a ring of cameras sees sharply");
  const r: Ring = { ...START };
  const side = figure(c.figures, "Side slice through the rig axis (x across, depth down the page from the rig).");
  const top = figure(c.figures, "Top slice at the working distance (x across, y up).");

  const draw = perFrame(() => {
    try {
      const layout = layoutOf(r);
      const wd = r.working_distance_m;
      const criteria = { max_blur_px: r.max_blur_px };
      const sideCov = rigCoverage(
        layout,
        { min: [-HALF, -0.0005, wd - DEPTH], max: [HALF, 0.0005, wd + DEPTH], n: [N, 1, N] },
        criteria,
      );
      const topCov = rigCoverage(
        layout,
        { min: [-HALF, -HALF, wd - 0.0005], max: [HALF, HALF, wd + 0.0005], n: [N, N, 1] },
        criteria,
      );
      const s = paint(sideCov, r.count);
      const t = paint(topCov, r.count);
      const items: [string, string][] = [];
      for (let k = 1; k <= r.count; k++) {
        const [R, G, B] = viridis(r.count <= 1 ? 0.6 : (k - 1) / (r.count - 1));
        items.push([`${k} camera${k > 1 ? "s" : ""}`, `rgb(${R | 0},${G | 0},${B | 0})`]);
      }
      if (r.lasers) items.push(["a camera sees its laser", "rgb(240,120,40)"]);
      const nan: [number, number, number, number] = [128, 128, 128, 25];
      const sideCanvas = heatCanvas(
        { values: s.values, cols: N, rows: N, range: [0, r.count], colour: s.colour, nanColour: nan, flipY: false },
        4,
      );
      sideCanvas.style.aspectRatio = `${2 * HALF} / ${2 * DEPTH}`;
      setDrawing(side, sideCanvas, legend(items));
      setDrawing(
        top,
        heatCanvas(
          { values: t.values, cols: N, rows: N, range: [0, r.count], colour: t.colour, nanColour: nan, flipY: true },
          4,
        ),
      );
      const sharp = rigSharpness(layout, criteria)[0]!;
      const multi = sideCov.views.reduce((a, v) => a + (v >= 2 ? 1 : 0), 0);
      const all = sideCov.views.reduce((a, v) => a + (v >= r.count ? 1 : 0), 0);
      const cell = ((2 * HALF) / N) * ((2 * DEPTH) / N) * 1e6;
      showError(c, null);
      readouts(c, [
        [
          "each camera sharp along its axis",
          sharp.kind === "between"
            ? `${fmt.mm(sharp.near, 0)} – ${sharp.far === null ? "∞" : fmt.mm(sharp.far, 0)}`
            : sharp.kind,
        ],
        ["camera tilt toward the axis", fmt.deg(Math.atan2(r.count === 1 ? 0 : r.radius_m, wd))],
        ["side slice seen by ≥ 2 cameras", `${((multi * cell) / 100).toFixed(1)} cm²`],
        [`… by all ${r.count}`, `${((all * cell) / 100).toFixed(1)} cm²`],
      ]);
    } catch (e) {
      showError(c, e);
    }
  });

  const sliders = {
    count: slider(c.controls, {
      label: "cameras",
      min: 1,
      max: 6,
      step: 1,
      value: r.count,
      format: (v) => v.toFixed(0),
      onInput: (v) => ((r.count = v), draw()),
    }),
    radius_m: slider(c.controls, {
      label: "ring radius",
      min: 0.02,
      max: 0.2,
      step: 0.005,
      value: r.radius_m,
      format: (v) => fmt.mm(v, 0),
      onInput: (v) => ((r.radius_m = v), draw()),
    }),
    working_distance_m: slider(c.controls, {
      label: "working distance",
      min: 0.25,
      max: 1,
      step: 0.01,
      value: r.working_distance_m,
      format: (v) => fmt.m(v, 2),
      onInput: (v) => ((r.working_distance_m = v), draw()),
    }),
    f_number: slider(c.controls, {
      label: "f-number",
      min: 2,
      max: 16,
      step: 0.1,
      value: r.f_number,
      format: fmt.f,
      onInput: (v) => ((r.f_number = v), draw()),
    }),
    max_blur_px: slider(c.controls, {
      label: "blur limit",
      min: 1,
      max: 6,
      step: 0.1,
      value: r.max_blur_px,
      format: (v) => fmt.px(v, 1),
      onInput: (v) => ((r.max_blur_px = v), draw()),
    }),
  };
  select(c.controls, {
    label: "lasers",
    options: [
      ["on", "one line laser per camera"],
      ["off", "none"],
    ],
    value: r.lasers ? "on" : "off",
    onChange: (v) => ((r.lasers = v === "on"), draw()),
  });
  const reset = el("button", "", "Reset");
  reset.type = "button";
  reset.addEventListener("click", () => {
    Object.assign(r, START);
    for (const [k, s] of Object.entries(sliders)) s.value = r[k as keyof typeof sliders];
    draw();
  });
  c.buttons.append(reset);
  c.note.textContent =
    "IMX174 cameras behind 25 mm lenses, each focused on the working point. A point counts as seen when it is in a camera's view and its blur (defocus and diffraction) is within the limit. Stop down or relax the limit to deepen the sharp region; spread the ring to raise the triangulation angle, at the cost of overlap.";
  draw();
}
