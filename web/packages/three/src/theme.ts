/**
 * Scene colours from the vitavision design tokens (`@vitavision/ui/styles.css`), so a 3D view
 * matches the UI around it in both themes. The tokens are CSS custom properties on the root
 * element; the theme switches by toggling the `dark` class there.
 */

/** Colours a scene draws with, as CSS colour strings (three's `Color.setStyle` accepts them). */
export interface SceneColors {
  /** Viewport background: the page ground (`--ground`). */
  background: string;
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

/**
 * Read the current token values from `root` (default: the document element). A token that is
 * not defined (no stylesheet loaded) reads as `gray`.
 */
export function readSceneColors(root?: Element): SceneColors {
  const el = root ?? document.documentElement;
  const style = getComputedStyle(el);
  const out = {} as SceneColors;
  for (const [key, token] of Object.entries(TOKENS) as [keyof SceneColors, string][]) {
    out[key] = style.getPropertyValue(token).trim() || "gray";
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
