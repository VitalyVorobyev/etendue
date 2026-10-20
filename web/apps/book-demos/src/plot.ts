/**
 * Small SVG and canvas plots: line charts with axes, point clouds, and a heat map with a colour
 * bar. Drawn in the page's colours (`currentColor` for ink).
 */

const NS = "http://www.w3.org/2000/svg";

/** An SVG element with attributes. */
export function svg<K extends keyof SVGElementTagNameMap>(
  tag: K,
  attrs: Record<string, string | number> = {},
  ...children: (SVGElement | string)[]
): SVGElementTagNameMap[K] {
  const e = document.createElementNS(NS, tag);
  for (const [k, v] of Object.entries(attrs)) {
    // A theme colour (`var(--…)`) works as a style property, not as a presentation attribute.
    if (typeof v === "string" && v.startsWith("var(")) e.style.setProperty(k, v);
    else e.setAttribute(k, String(v));
  }
  for (const c of children) e.append(c);
  return e;
}

/** "Nice" tick values covering `[lo, hi]`, about `count` of them. */
export function ticks(lo: number, hi: number, count = 5): number[] {
  if (!(hi > lo)) return [lo];
  const raw = (hi - lo) / count;
  const mag = 10 ** Math.floor(Math.log10(raw));
  const norm = raw / mag;
  const step = (norm >= 5 ? 10 : norm >= 2 ? 5 : norm >= 1 ? 2 : 1) * mag;
  const out: number[] = [];
  for (let t = Math.ceil(lo / step - 1e-9) * step; t <= hi + step * 1e-9; t += step) {
    out.push(Math.abs(t) < step * 1e-9 ? 0 : t);
  }
  return out;
}

/** A tick label: as few decimals as the step needs. */
export function tickLabel(v: number, step: number): string {
  const digits = Math.max(0, Math.min(6, -Math.floor(Math.log10(step) + 1e-9)));
  return v.toFixed(digits);
}

export interface Series {
  name: string;
  colour: string;
  x: ArrayLike<number>;
  y: ArrayLike<number>;
  dashed?: boolean;
}

export interface LineChart {
  series: Series[];
  xLabel: string;
  yLabel: string;
  /** Fixed y range; default the data's, from 0 when `fromZero`. */
  yRange?: [number, number];
  fromZero?: boolean;
  /** Horizontal guide lines (e.g. a limit). */
  hLines?: { y: number; label: string }[];
  /** Vertical guide lines (e.g. the focus distance). */
  vLines?: { x: number; label: string }[];
  width?: number;
  height?: number;
}

/** Finite min and max of a set of arrays. */
function extent(arrays: ArrayLike<number>[]): [number, number] {
  let lo = Infinity;
  let hi = -Infinity;
  for (const a of arrays) {
    for (let i = 0; i < a.length; i++) {
      const v = a[i]!;
      if (Number.isFinite(v)) {
        if (v < lo) lo = v;
        if (v > hi) hi = v;
      }
    }
  }
  if (!Number.isFinite(lo)) return [0, 1];
  if (hi === lo) return [lo - 1, hi + 1];
  return [lo, hi];
}

/** A line chart; gaps where `y` is not finite. */
export function lineChart(c: LineChart): SVGSVGElement {
  const W = c.width ?? 520;
  const H = c.height ?? 260;
  const m = { l: 52, r: 12, t: 10, b: 38 };
  const [x0, x1] = extent(c.series.map((s) => s.x));
  let [y0, y1] = c.yRange ?? extent([...c.series.map((s) => s.y), ...(c.hLines ?? []).map((h) => [h.y])]);
  if (!c.yRange) {
    if (c.fromZero) y0 = Math.min(0, y0);
    const pad = (y1 - y0) * 0.06;
    y1 += pad;
    if (!c.fromZero) y0 -= pad;
  }
  const sx = (x: number) => m.l + ((x - x0) / (x1 - x0)) * (W - m.l - m.r);
  const sy = (y: number) => H - m.b - ((Math.min(Math.max(y, y0), y1) - y0) / (y1 - y0)) * (H - m.t - m.b);
  const root = svg("svg", { viewBox: `0 0 ${W} ${H}`, role: "img", "font-size": 11, fill: "currentColor" });
  const ink = { stroke: "currentColor", "stroke-opacity": 0.25 };
  const xt = ticks(x0, x1, 6);
  const yt = ticks(y0, y1, 5);
  const xs = xt.length > 1 ? xt[1]! - xt[0]! : 1;
  const ys = yt.length > 1 ? yt[1]! - yt[0]! : 1;
  for (const t of xt) {
    root.append(svg("line", { x1: sx(t), x2: sx(t), y1: m.t, y2: H - m.b, ...ink, "stroke-opacity": 0.1 }));
    root.append(svg("text", { x: sx(t), y: H - m.b + 14, "text-anchor": "middle" }, tickLabel(t, xs)));
  }
  for (const t of yt) {
    root.append(svg("line", { x1: m.l, x2: W - m.r, y1: sy(t), y2: sy(t), ...ink, "stroke-opacity": 0.1 }));
    root.append(svg("text", { x: m.l - 6, y: sy(t) + 4, "text-anchor": "end" }, tickLabel(t, ys)));
  }
  root.append(svg("rect", { x: m.l, y: m.t, width: W - m.l - m.r, height: H - m.t - m.b, fill: "none", ...ink }));
  root.append(svg("text", { x: (m.l + W - m.r) / 2, y: H - 6, "text-anchor": "middle" }, c.xLabel));
  root.append(
    svg(
      "text",
      { x: 12, y: (m.t + H - m.b) / 2, "text-anchor": "middle", transform: `rotate(-90 12 ${(m.t + H - m.b) / 2})` },
      c.yLabel,
    ),
  );
  for (const h of c.hLines ?? []) {
    if (h.y < y0 || h.y > y1) continue;
    root.append(
      svg("line", {
        x1: m.l,
        x2: W - m.r,
        y1: sy(h.y),
        y2: sy(h.y),
        stroke: "currentColor",
        "stroke-dasharray": "4 3",
        "stroke-opacity": 0.6,
      }),
    );
    root.append(svg("text", { x: W - m.r - 4, y: sy(h.y) - 4, "text-anchor": "end", "fill-opacity": 0.8 }, h.label));
  }
  for (const v of c.vLines ?? []) {
    if (v.x < x0 || v.x > x1) continue;
    root.append(
      svg("line", {
        x1: sx(v.x),
        x2: sx(v.x),
        y1: m.t,
        y2: H - m.b,
        stroke: "currentColor",
        "stroke-dasharray": "2 3",
        "stroke-opacity": 0.6,
      }),
    );
    root.append(svg("text", { x: sx(v.x) + 4, y: m.t + 12, "fill-opacity": 0.8 }, v.label));
  }
  for (const s of c.series) {
    let d = "";
    let pen = false;
    for (let i = 0; i < s.x.length; i++) {
      const x = s.x[i]!;
      const y = s.y[i]!;
      if (!Number.isFinite(x) || !Number.isFinite(y)) {
        pen = false;
        continue;
      }
      d += `${pen ? "L" : "M"}${sx(x).toFixed(1)},${sy(y).toFixed(1)}`;
      pen = true;
    }
    root.append(
      svg("path", {
        d,
        fill: "none",
        stroke: s.colour,
        "stroke-width": 2,
        ...(s.dashed ? { "stroke-dasharray": "5 3" } : {}),
      }),
    );
  }
  return root;
}

/** A 2-D view box mapping world `[x, y]` (y up) into an SVG of `width × height`. */
export interface Frame {
  root: SVGSVGElement;
  /** World to SVG. */
  x: (x: number) => number;
  y: (y: number) => number;
  /** SVG pixels per world unit. */
  scale: number;
}

/** An SVG fitting the world box `[x0, x1] × [y0, y1]` with equal axes, y up. */
export function frame(box: [number, number, number, number], width = 520, height = 300, margin = 14): Frame {
  const [x0, x1, y0, y1] = box;
  const scale = Math.min((width - 2 * margin) / (x1 - x0), (height - 2 * margin) / (y1 - y0));
  const cx = (x0 + x1) / 2;
  const cy = (y0 + y1) / 2;
  const root = svg("svg", { viewBox: `0 0 ${width} ${height}`, role: "img", "font-size": 11, fill: "currentColor" });
  return {
    root,
    x: (x) => width / 2 + (x - cx) * scale,
    y: (y) => height / 2 - (y - cy) * scale,
    scale,
  };
}

/** A perceptual colour map (viridis, sampled), `t` in [0, 1]. */
const VIRIDIS = [
  [68, 1, 84],
  [72, 40, 120],
  [62, 74, 137],
  [49, 104, 142],
  [38, 130, 142],
  [31, 158, 137],
  [53, 183, 121],
  [110, 206, 88],
  [181, 222, 43],
  [253, 231, 37],
] as const;

export function viridis(t: number): [number, number, number] {
  const u = Math.min(1, Math.max(0, t)) * (VIRIDIS.length - 1);
  const i = Math.min(VIRIDIS.length - 2, Math.floor(u));
  const f = u - i;
  const a = VIRIDIS[i]!;
  const b = VIRIDIS[i + 1]!;
  return [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f, a[2] + (b[2] - a[2]) * f];
}

export interface Heat {
  /** Row-major values, `cols × rows`, row 0 drawn at the bottom when `flipY`. */
  values: ArrayLike<number>;
  cols: number;
  rows: number;
  range: [number, number];
  /** Colour of a value; default viridis over `range`. NaN draws as `nanColour`. */
  colour?: (v: number) => [number, number, number];
  nanColour?: [number, number, number, number];
  flipY?: boolean;
}

/** A heat map as a canvas, `cols × rows` cells scaled up (crisp). */
export function heatCanvas(h: Heat, pixelsPerCell = 8): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  canvas.width = h.cols * pixelsPerCell;
  canvas.height = h.rows * pixelsPerCell;
  const g = canvas.getContext("2d");
  if (!g) return canvas;
  const img = g.createImageData(h.cols, h.rows);
  const [lo, hi] = h.range;
  const nan = h.nanColour ?? [128, 128, 128, 40];
  for (let r = 0; r < h.rows; r++) {
    for (let c = 0; c < h.cols; c++) {
      const v = h.values[r * h.cols + c]!;
      const row = h.flipY ? h.rows - 1 - r : r;
      const o = (row * h.cols + c) * 4;
      if (!Number.isFinite(v)) {
        img.data.set(nan, o);
        continue;
      }
      const [R, G, B] = h.colour ? h.colour(v) : viridis((v - lo) / (hi - lo || 1));
      img.data.set([R, G, B, 255], o);
    }
  }
  const tmp = document.createElement("canvas");
  tmp.width = h.cols;
  tmp.height = h.rows;
  tmp.getContext("2d")?.putImageData(img, 0, 0);
  g.imageSmoothingEnabled = false;
  g.drawImage(tmp, 0, 0, canvas.width, canvas.height);
  return canvas;
}

/** A horizontal colour bar for `range` with a label. */
export function colourBar(range: [number, number], label: string, width = 520): SVGSVGElement {
  const H = 42;
  const root = svg("svg", { viewBox: `0 0 ${width} ${H}`, role: "img", "font-size": 11, fill: "currentColor" });
  const id = `ed-grad-${Math.random().toString(36).slice(2)}`;
  const grad = svg("linearGradient", { id });
  for (let i = 0; i <= 10; i++) {
    const [r, g, b] = viridis(i / 10);
    grad.append(svg("stop", { offset: `${i * 10}%`, "stop-color": `rgb(${r | 0},${g | 0},${b | 0})` }));
  }
  root.append(svg("defs", {}, grad));
  root.append(svg("rect", { x: 10, y: 4, width: width - 20, height: 8, fill: `url(#${id})` }));
  const t = ticks(range[0], range[1], 6);
  const step = t.length > 1 ? t[1]! - t[0]! : 1;
  for (const v of t) {
    const x = 10 + ((v - range[0]) / (range[1] - range[0] || 1)) * (width - 20);
    root.append(svg("text", { x, y: 24, "text-anchor": "middle" }, tickLabel(v, step)));
  }
  root.append(svg("text", { x: width / 2, y: 39, "text-anchor": "middle", "fill-opacity": 0.8 }, label));
  return root;
}
