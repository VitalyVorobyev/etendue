/**
 * The blur budget along a camera's view: geometric defocus (the circle of confusion), the Airy
 * disc of diffraction, and their root sum of squares, against depth; the depth of field for a
 * blur limit. Sampled by the kernel (`headMap`) along a laser ray that runs almost along the
 * camera's axis.
 */

import { type HeadMap, headMap } from "@etendue/wasm";

import { type Head, DEFAULT_HEAD, toDesign } from "../head";
import { kernel } from "../kernel";
import { lineChart } from "../plot";
import { card, figure, fmt, legend, perFrame, readouts, series, setDrawing, showError, slider } from "../ui";

/** A laser 10 mm off the axis aimed 0.5 m out: its central ray is about 1° off the axis. */
const START: Head = {
  ...DEFAULT_HEAD,
  baseline_m: 0.01,
  height_m: 0,
  aim_m: 0.5,
  focus_distance_m: 0.45,
  f_number: 4,
  depth_m: 0.5,
  width_m: 0.001,
};

const N = 241;

export default async function blur(host: HTMLElement): Promise<void> {
  await kernel();
  const c = card(host, "The blur budget: defocus, diffraction and depth of field");
  const h: Head = { ...START };
  const fig = figure(
    c.figures,
    "Blur along the camera's axis. The dashed line is the blur limit; between its crossings is the depth of field.",
    true,
  );

  const draw = perFrame(() => {
    try {
      const design = toDesign(h);
      const map: HeadMap = headMap(design, null, 1, N);
      const w = design.window!;
      // Depth along the camera axis of each sample on the (near-axial) laser ray.
      const d = Math.hypot(h.baseline_m, h.aim_m - h.height_m);
      const cosA = (h.aim_m - h.height_m) / d;
      const z = Array.from(
        { length: N },
        (_, j) => h.height_m + (w.near_m + ((w.far_m - w.near_m) * j) / (N - 1)) * cosA,
      );
      setDrawing(
        fig,
        lineChart({
          series: [
            { name: "defocus (CoC)", colour: series(0), x: z, y: map.coc_px },
            { name: "diffraction (Airy)", colour: series(1), x: z, y: map.airy_px, dashed: true },
            { name: "total", colour: series(2), x: z, y: map.blur_px },
          ],
          xLabel: "depth from the camera (m)",
          yLabel: "blur (px)",
          yRange: [0, Math.max(3 * h.max_blur_px, 1)],
          hLines: [{ y: h.max_blur_px, label: `limit ${h.max_blur_px.toFixed(1)} px` }],
          vLines: [{ x: h.focus_distance_m, label: "focus" }],
          width: 720,
          height: 280,
        }),
        legend([
          ["defocus (CoC)", series(0)],
          ["diffraction (Airy)", series(1)],
          ["total", series(2)],
        ]),
      );
      // Where the total blur is within the limit.
      let near = NaN;
      let far = NaN;
      for (let j = 0; j < N; j++) {
        if (map.blur_px[j]! <= h.max_blur_px) {
          if (Number.isNaN(near)) near = z[j]!;
          far = z[j]!;
        }
      }
      const sharp = Number.isFinite(near);
      const atEnd = sharp && far >= z[N - 1]! - 1e-9;
      showError(c, null);
      readouts(c, [
        ["Airy diameter", fmt.px(map.airy_px[Math.round((N - 1) / 2)]!)],
        ["sharp from", sharp ? fmt.m(near) : "nowhere"],
        ["sharp to", sharp ? (atEnd ? `beyond ${fmt.m(far)}` : fmt.m(far)) : "nowhere"],
        ["depth of field", sharp ? `${atEnd ? "≥ " : ""}${fmt.mm(far - near, 0)}` : "none"],
      ]);
    } catch (e) {
      showError(c, e);
    }
  });

  const sliders = {
    f_number: slider(c.controls, {
      label: "f-number",
      min: 1.4,
      max: 22,
      step: 0.1,
      value: h.f_number,
      format: fmt.f,
      onInput: (v) => ((h.f_number = v), draw()),
    }),
    focus_distance_m: slider(c.controls, {
      label: "focus distance",
      min: 0.25,
      max: 0.75,
      step: 0.005,
      value: h.focus_distance_m,
      format: (v) => fmt.m(v, 3),
      onInput: (v) => ((h.focus_distance_m = v), draw()),
    }),
    max_blur_px: slider(c.controls, {
      label: "blur limit",
      min: 0.5,
      max: 6,
      step: 0.1,
      value: h.max_blur_px,
      format: (v) => fmt.px(v, 1),
      onInput: (v) => ((h.max_blur_px = v), draw()),
    }),
    wavelength_nm: slider(c.controls, {
      label: "wavelength",
      min: 400,
      max: 950,
      step: 5,
      value: h.wavelength_nm,
      format: (v) => `${v.toFixed(0)} nm`,
      onInput: (v) => ((h.wavelength_nm = v), draw()),
    }),
  };
  const reset = document.createElement("button");
  reset.type = "button";
  reset.textContent = "Reset";
  reset.addEventListener("click", () => {
    Object.assign(h, START);
    for (const [k, s] of Object.entries(sliders)) s.value = h[k as keyof typeof sliders];
    draw();
  });
  c.buttons.append(reset);
  c.note.textContent =
    "IMX174 (5.86 µm pixels) behind a 25 mm lens. Stopping down shrinks the circle of confusion but grows the Airy disc: past some f-number, diffraction sets the blur everywhere.";
  draw();
}
