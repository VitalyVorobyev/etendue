/**
 * The widgets' DOM: a card with controls, a figure and readouts, styled with mdBook's theme
 * variables so it follows the reader's theme (light, rust, coal, navy, ayu).
 */

import CSS from "./widgets.css?inline";

/** The colour of series `i`: one of six, defined in widgets.css. */
export const series = (i: number): string => `var(--ed-s${i % 6})`;

let styled = false;

/** Add the widgets' style sheet once. */
export function installStyle(): void {
  if (styled) return;
  styled = true;
  const style = document.createElement("style");
  style.textContent = CSS;
  document.head.appendChild(style);
}

/** An element with a class and children. */
export function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string,
  ...children: (Node | string)[]
): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (className) e.className = className;
  e.append(...children);
  return e;
}

/** A widget's frame: title, controls, buttons, figures, readouts, notes and an error line. */
export interface Card {
  root: HTMLElement;
  controls: HTMLElement;
  buttons: HTMLElement;
  figures: HTMLElement;
  readouts: HTMLElement;
  note: HTMLElement;
  error: HTMLElement;
}

/** Build a card in `host`, replacing its contents. */
export function card(host: HTMLElement, title: string): Card {
  installStyle();
  const controls = el("div", "ed-controls");
  const buttons = el("div", "ed-buttons");
  const figures = el("div", "ed-figures");
  const readouts = el("div", "ed-readouts");
  const note = el("div", "ed-note");
  const error = el("div", "ed-error");
  error.setAttribute("role", "alert");
  const root = el(
    "div",
    "ed-card",
    el("div", "ed-head", el("span", "ed-tag", "Try it"), el("span", "ed-title", title)),
    controls,
    buttons,
    figures,
    readouts,
    note,
    error,
  );
  host.replaceChildren(root);
  host.setAttribute("data-state", "ready");
  return { root, controls, buttons, figures, readouts, note, error };
}

/** A slider with a label and its formatted value. */
export interface Slider {
  input: HTMLInputElement;
  get value(): number;
  set value(v: number);
}

export function slider(
  parent: HTMLElement,
  opts: {
    label: string;
    min: number;
    max: number;
    step: number;
    value: number;
    format: (v: number) => string;
    onInput: (v: number) => void;
  },
): Slider {
  const input = el("input");
  input.type = "range";
  input.min = String(opts.min);
  input.max = String(opts.max);
  input.step = String(opts.step);
  input.value = String(opts.value);
  input.setAttribute("aria-label", opts.label);
  const out = el("output", "", opts.format(opts.value));
  const label = el("label", "", opts.label);
  parent.append(el("div", "ed-control", label, out, input));
  const show = () => {
    out.textContent = opts.format(Number(input.value));
  };
  input.addEventListener("input", () => {
    show();
    opts.onInput(Number(input.value));
  });
  return {
    input,
    get value() {
      return Number(input.value);
    },
    set value(v: number) {
      input.value = String(v);
      show();
    },
  };
}

/** A drop-down of named options. */
export function select<T extends string>(
  parent: HTMLElement,
  opts: { label: string; options: [T, string][]; value: T; onChange: (v: T) => void },
): HTMLSelectElement {
  const s = el("select");
  s.setAttribute("aria-label", opts.label);
  for (const [value, text] of opts.options) {
    const o = el("option", "", text);
    o.value = value;
    s.append(o);
  }
  s.value = opts.value;
  s.addEventListener("change", () => opts.onChange(s.value as T));
  parent.append(el("div", "ed-control", el("label", "", opts.label), el("output"), s));
  return s;
}

/** A button. */
export function button(parent: HTMLElement, text: string, onClick: () => void): HTMLButtonElement {
  const b = el("button", "", text);
  b.type = "button";
  b.addEventListener("click", onClick);
  parent.append(b);
  return b;
}

/** A figure with a caption; returns the figure, to fill. A `wide` one spans the whole row. */
export function figure(parent: HTMLElement, caption: string, wide = false): HTMLElement {
  const f = el("figure", wide ? "ed-figure ed-wide" : "ed-figure");
  const c = el("figcaption", "", caption);
  f.append(c);
  parent.append(f);
  return f;
}

/** Replace a figure's drawing (keeping its caption). */
export function setDrawing(fig: HTMLElement, drawing: Element, legend?: HTMLElement): void {
  for (const child of Array.from(fig.children)) if (child.tagName !== "FIGCAPTION") child.remove();
  fig.prepend(drawing);
  if (legend) drawing.after(legend);
}

/** A legend of coloured series names. */
export function legend(items: [string, string][]): HTMLElement {
  const l = el("div", "ed-legend");
  for (const [name, colour] of items) {
    const swatch = el("i");
    swatch.style.background = colour;
    l.append(el("span", "", swatch, name));
  }
  return l;
}

/** Set the readouts to `rows` of label and value. */
export function readouts(card: Card, rows: [string, string][]): void {
  card.readouts.replaceChildren(...rows.map(([k, v]) => el("div", "", el("span", "", k), el("span", "", v))));
}

/** Show (or clear, with `null`) an error under the widget. */
export function showError(card: Card, error: unknown): void {
  card.error.textContent = error === null ? "" : describe(error);
}

/** A thrown value as a sentence. */
export function describe(error: unknown): string {
  if (error instanceof Error) return error.message;
  return String(error);
}

/**
 * Coalesce bursts of calls (slider drags) into one per animation frame.
 */
export function perFrame(run: () => void): () => void {
  let queued = false;
  return () => {
    if (queued) return;
    queued = true;
    requestAnimationFrame(() => {
      queued = false;
      run();
    });
  };
}

/** Number formatting. */
export const fmt = {
  mm: (m: number, digits = 1) => `${(m * 1000).toFixed(digits)} mm`,
  m: (m: number, digits = 3) => `${m.toFixed(digits)} m`,
  px: (px: number, digits = 2) => `${px.toFixed(digits)} px`,
  deg: (rad: number, digits = 1) => `${((rad * 180) / Math.PI).toFixed(digits)}°`,
  f: (n: number) => `f/${n.toFixed(1)}`,
  num: (n: number, digits = 3) => n.toFixed(digits),
};
