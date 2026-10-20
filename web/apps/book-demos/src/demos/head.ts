/**
 * A laser-triangulation head over its window: blur, depth resolution, lateral resolution and
 * triangulation angle at every point of the laser plane, with the head's geometry on sliders and
 * the optimiser one click away (`headMap`, `headOptimise`).
 */

import { type HeadMap, headMap, headOptimise } from "@etendue/wasm";

import { type Head, DEFAULT_HEAD, toDesign, windowRange } from "../head";
import { kernel } from "../kernel";
import { colourBar, heatCanvas, svg } from "../plot";
import { button, card, el, figure, fmt, perFrame, readouts, select, setDrawing, showError, slider } from "../ui";

type Metric = "blur" | "depth" | "lateral" | "angle";

const METRICS: Record<Metric, { label: string; unit: string; of: (m: HeadMap, i: number) => number }> = {
  blur: { label: "total blur", unit: "px", of: (m, i) => m.blur_px[i]! },
  depth: { label: "depth per pixel of stripe shift", unit: "µm", of: (m, i) => m.depth_m_per_px[i]! * 1e6 },
  lateral: { label: "lateral size of a pixel", unit: "µm", of: (m, i) => m.lateral_m_per_px[i]! * 1e6 },
  angle: { label: "triangulation angle", unit: "°", of: (m, i) => (m.triangulation_rad[i]! * 180) / Math.PI },
};

const ACROSS = 41;
const ALONG = 61;
const START: Head = { ...DEFAULT_HEAD };

/** The worst blur over the visible samples, and the share of samples seen and within the limit. */
export function summary(map: HeadMap, limit: number): { worst: number; seen: number; sharp: number } {
  let worst = 0;
  let seen = 0;
  let sharp = 0;
  const n = map.across * map.along;
  for (let i = 0; i < n; i++) {
    if (!map.visible[i]) continue;
    seen++;
    worst = Math.max(worst, map.blur_px[i]!);
    if (map.blur_px[i]! <= limit) sharp++;
  }
  return { worst, seen: seen / n, sharp: sharp / n };
}

export default async function head(host: HTMLElement): Promise<void> {
  await kernel();
  const c = card(host, "A laser-triangulation head over its window");
  const h: Head = { ...START, tilt: [...START.tilt] };
  let metric: Metric = "blur";
  let last = "";
  const fig = figure(
    c.figures,
    "The laser plane face on: across the laser line (horizontal) by range along the laser (vertical, far end up). Grey: the camera does not see it.",
    true,
  );

  const draw = perFrame(() => {
    try {
      const design = toDesign(h);
      const map = headMap(design, null, ACROSS, ALONG);
      const spec = METRICS[metric];
      const values = new Float64Array(ACROSS * ALONG);
      let lo = Infinity;
      let hi = -Infinity;
      for (let i = 0; i < values.length; i++) {
        const v = map.visible[i] ? spec.of(map, i) : NaN;
        values[i] = v;
        if (Number.isFinite(v)) [lo, hi] = [Math.min(lo, v), Math.max(hi, v)];
      }
      if (metric === "blur") [lo, hi] = [0, Math.max(hi, 2 * h.max_blur_px)];
      if (!Number.isFinite(lo)) [lo, hi] = [0, 1];
      const canvas = heatCanvas({ values, cols: ACROSS, rows: ALONG, range: [lo, hi], flipY: true }, 6);
      // To scale: the window's width against its depth.
      canvas.style.height = "300px";
      canvas.style.width = `${Math.min(680, Math.max(48, (300 * h.width_m) / h.depth_m)).toFixed(0)}px`;
      canvas.style.margin = "0 auto";
      const [near, far] = windowRange(h);
      const wrap = el("div");
      wrap.style.display = "grid";
      wrap.style.gridTemplateColumns = "auto 1fr";
      wrap.style.alignItems = "center";
      wrap.style.gap = "6px";
      const axis = svg("svg", { viewBox: "0 0 40 300", width: 40, height: 300, "font-size": 11, fill: "currentColor" });
      axis.append(svg("text", { x: 2, y: 12 }, `${(far * 1000).toFixed(0)}`));
      axis.append(svg("text", { x: 2, y: 296 }, `${(near * 1000).toFixed(0)}`));
      axis.append(svg("text", { x: 2, y: 154 }, "mm"));
      axis.style.cssText = "width: 40px; border: none; background: none;";
      wrap.append(axis, canvas);
      setDrawing(fig, wrap, colourBarElement([lo, hi], `${spec.label} (${spec.unit})`));
      const s = summary(map, h.max_blur_px);
      showError(c, null);
      readouts(c, [
        ["window", `${fmt.mm(near, 0)} – ${fmt.mm(far, 0)} × ${fmt.mm(h.width_m, 0)}`],
        ["worst blur", s.seen > 0 ? fmt.px(s.worst) : "—"],
        ["window seen", `${(100 * s.seen).toFixed(0)} %`],
        ["within the blur limit", `${(100 * s.sharp).toFixed(0)} %`],
        ["sensor tilt τx", fmt.deg(h.tilt[0], 2)],
        ["focus, f-number", `${fmt.m(h.focus_distance_m)}, ${fmt.f(h.f_number)}`],
      ]);
      c.note.textContent = last;
    } catch (e) {
      showError(c, e);
    }
  });

  select(c.controls, {
    label: "show",
    options: (Object.keys(METRICS) as Metric[]).map((k) => [k, METRICS[k].label]),
    value: metric,
    onChange: (v) => ((metric = v), draw()),
  });
  const sliders = {
    baseline_m: slider(c.controls, {
      label: "baseline (laser off axis)",
      min: 0.02,
      max: 0.25,
      step: 0.002,
      value: h.baseline_m,
      format: (v) => fmt.mm(v, 0),
      onInput: (v) => ((h.baseline_m = v), draw()),
    }),
    aim_m: slider(c.controls, {
      label: "aim point on the axis",
      min: 0.15,
      max: 0.8,
      step: 0.005,
      value: h.aim_m,
      format: (v) => fmt.mm(v, 0),
      onInput: (v) => ((h.aim_m = v), draw()),
    }),
    depth_m: slider(c.controls, {
      label: "window depth",
      min: 0.02,
      max: 0.3,
      step: 0.005,
      value: h.depth_m,
      format: (v) => fmt.mm(v, 0),
      onInput: (v) => ((h.depth_m = v), draw()),
    }),
    focus_distance_m: slider(c.controls, {
      label: "focus distance",
      min: 0.15,
      max: 0.9,
      step: 0.002,
      value: h.focus_distance_m,
      format: (v) => fmt.m(v),
      onInput: (v) => ((h.focus_distance_m = v), draw()),
    }),
    f_number: slider(c.controls, {
      label: "f-number",
      min: 1.4,
      max: 16,
      step: 0.1,
      value: h.f_number,
      format: fmt.f,
      onInput: (v) => ((h.f_number = v), draw()),
    }),
  };
  const sync = () => {
    for (const [k, s] of Object.entries(sliders)) s.value = h[k as keyof typeof sliders];
    draw();
  };
  button(c.buttons, "Optimise tilt, focus and f-number", () => {
    try {
      const t0 = performance.now();
      const o = headOptimise(toDesign(h), { grid: [9, 9], f_number_range: [1.4, 16], tilt_free: true });
      h.tilt = [o.best.tilt[0], o.best.tilt[1]];
      h.focus_distance_m = o.best.focus_distance_m;
      h.f_number = o.best.f_number;
      last = `Optimised in ${(performance.now() - t0).toFixed(0)} ms: worst blur ${fmt.px(o.start_worst_blur_px)} at the Scheimpflug start, ${fmt.px(o.worst_blur_px)} at the optimum.`;
      sync();
    } catch (e) {
      showError(c, e);
    }
  });
  button(c.buttons, "Square sensor", () => {
    h.tilt = [0, 0];
    last = "";
    sync();
  });
  button(c.buttons, "Reset", () => {
    Object.assign(h, START, { tilt: [...START.tilt] });
    last = "";
    sync();
  });
  draw();
}

function colourBarElement(range: [number, number], label: string): HTMLElement {
  const wrap = el("div");
  wrap.append(colourBar(range, label, 720));
  return wrap;
}
