/**
 * Scene colours from the vitavision design tokens (`@vitavision/ui/styles.css`), so a 3D view
 * matches the UI around it in both themes. The tokens are CSS custom properties on the root
 * element; the theme switches by toggling the `dark` class there.
 */

/** Colours a scene draws with, as CSS colour strings (three's `Color.setStyle` accepts them). */
export interface SceneColors {
  /** Viewport background: the page ground (`--ground`). */
  background: string;
  /** Image canvases: the dark field behind pixels (`--canvas`). */
  canvas: string;
  /** Raised surfaces: targets, board fills (`--surface`). */
  surface: string;
  /** Primary foreground: labels, axes of the world frame (`--fg`). */
  fg: string;
  /** Secondary foreground: inactive gizmos (`--fg-muted`). */
  muted: string;
  /** Grid lines (`--line`). */
  line: string;
  /** Strong lines (`--line-strong`). */
  lineStrong: string;
  /** The accent: selection and the active item (`--signal`). */
  signal: string;
  /** Positive / pass; also the Y axis (`--normal`). */
  normal: string;
  /** Negative / fail; also the X axis and laser light (`--defect`). */
  defect: string;
  /** Caution; also light gizmos (`--warn`). */
  warn: string;
}

const TOKENS: Record<keyof SceneColors, string> = {
  background: "--ground",
  canvas: "--canvas",
  surface: "--surface",
  fg: "--fg",
  muted: "--fg-muted",
  line: "--line",
  lineStrong: "--line-strong",
  signal: "--signal",
  normal: "--normal",
  defect: "--defect",
  warn: "--warn",
};

let probe: CanvasRenderingContext2D | null | undefined;

/**
 * `css` as a colour three.js parses (`rgb(r, g, b)`). three's `Color.setStyle` silently
 * ignores modern syntax — space-separated `hsl()`, `oklch()`, `color-mix()` — so token values
 * go through the browser itself: painted into a 1 × 1 canvas and read back as sRGB bytes.
 * A value the browser rejects is returned unchanged; an empty one is `gray`. Without a 2D
 * canvas (server rendering, some test DOMs) the value is returned unchanged.
 */
export function normalizeColor(css: string): string {
  const value = css.trim();
  if (value === "") return "gray";
  if (probe === undefined) {
    const canvas = typeof document === "undefined" ? null : document.createElement("canvas");
    if (canvas) canvas.width = canvas.height = 1;
    probe = canvas?.getContext("2d", { willReadFrequently: true }) ?? null;
  }
  if (!probe) return value;
  // An invalid value leaves fillStyle as it was: two different sentinels tell.
  probe.fillStyle = "black";
  probe.fillStyle = value;
  const a = probe.fillStyle;
  probe.fillStyle = "white";
  probe.fillStyle = value;
  if (a !== probe.fillStyle) return value;
  probe.clearRect(0, 0, 1, 1);
  probe.fillRect(0, 0, 1, 1);
  const [r, g, b] = probe.getImageData(0, 0, 1, 1).data;
  return `rgb(${r}, ${g}, ${b})`;
}

/**
 * Read the current token values from `root` (default: the document element), normalised for
 * three.js (see {@link normalizeColor}). A token that is not defined (no stylesheet loaded)
 * reads as `gray`.
 */
export function readSceneColors(root?: Element): SceneColors {
  const el = root ?? document.documentElement;
  const style = getComputedStyle(el);
  const out = {} as SceneColors;
  for (const [key, token] of Object.entries(TOKENS) as [keyof SceneColors, string][]) {
    out[key] = normalizeColor(style.getPropertyValue(token));
  }
  return out;
}

/**
 * Call `onChange` with fresh colours whenever the theme class on `root` changes. Returns the
 * unsubscribe function.
 */
export function observeSceneColors(onChange: (colors: SceneColors) => void, root?: Element): () => void {
  const el = root ?? document.documentElement;
  const observer = new MutationObserver(() => onChange(readSceneColors(el)));
  observer.observe(el, { attributes: true, attributeFilter: ["class"] });
  return () => observer.disconnect();
}
