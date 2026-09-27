/*
 * The arithmetic behind `SplitPane`: resolving sizes given in pixels or percent, clamping a
 * drag or a key press to the pane's limits, and remembering a size across reloads.
 *
 * Kept out of the component so every rule — which key moves the divider which way, where a
 * collapsible pane snaps shut — is tested without a DOM.
 */

/**
 * A pane size: CSS pixels as a number, or a share of the split's length as a percentage
 * string (`"30%"`).
 */
export type PaneSize = number | `${number}%`;

/** The limits a `SplitPane`'s sized pane is held to, resolved to pixels. */
export interface SplitLimits {
  /** The smallest open size, in pixels. */
  min: number;
  /** The largest size, in pixels. */
  max: number;
  /** Whether the pane may close to 0 below `min`. */
  collapsible: boolean;
}

/**
 * A size in pixels.
 *
 * @param size - Pixels, or a percentage of `length`.
 * @param length - The split's length along its axis, in pixels.
 * @returns The size in pixels (never negative).
 */
export function resolveSize(size: PaneSize, length: number): number {
  const px = typeof size === "number" ? size : (Number.parseFloat(size) / 100) * length;
  return Number.isFinite(px) ? Math.max(0, px) : 0;
}

/**
 * The limits in pixels for a split of `length`. `max` never exceeds `length`, and `min`
 * never exceeds `max`.
 *
 * @param length - The split's length along its axis, in pixels.
 * @param minSize - The smallest open size. Defaults to 0.
 * @param maxSize - The largest size. Defaults to the whole length.
 * @param collapsible - Whether the pane may close below `minSize`.
 * @returns The resolved limits.
 */
export function resolveLimits(
  length: number,
  minSize: PaneSize = 0,
  maxSize: PaneSize = "100%",
  collapsible = false,
): SplitLimits {
  const max = Math.min(resolveSize(maxSize, length), Math.max(0, length));
  const min = Math.min(resolveSize(minSize, length), max);
  return { min, max, collapsible };
}

/**
 * Hold a proposed size to the limits. A collapsible pane dragged below half its minimum
 * snaps shut (0); between half the minimum and the minimum it stays at the minimum.
 *
 * @param size - The proposed size, in pixels.
 * @param limits - The resolved limits.
 * @returns The size to apply, in pixels, rounded to whole pixels.
 */
export function clampSize(size: number, limits: SplitLimits): number {
  if (limits.collapsible && size < limits.min / 2) return 0;
  return Math.round(Math.min(limits.max, Math.max(limits.min, size)));
}

/** Which way a `SplitPane` lays out its two panes. */
export type SplitOrientation = "horizontal" | "vertical";

/** Which of the two panes carries the explicit size; the other takes the rest. */
export type SizedPane = "start" | "end";

/** What a key press on the divider asks for. */
export type SplitKeyAction =
  | { type: "resize"; size: number }
  | { type: "toggle-collapse" };

/**
 * The divider's keyboard (the WAI-ARIA window-splitter pattern).
 *
 * The arrow keys along the split's axis move the divider the way they point — so the sized
 * pane grows or shrinks depending on which side it is on — by `step`, or four steps with
 * Shift. Home gives the sized pane its minimum, End its maximum, and Enter collapses or
 * restores a collapsible pane.
 *
 * @param key - `KeyboardEvent.key`.
 * @param shift - Whether Shift is held.
 * @param size - The sized pane's current size, in pixels.
 * @param options - The layout, the step in pixels and the resolved limits.
 * @returns The action, or `null` when the key is not the divider's.
 */
export function splitKeyAction(
  key: string,
  shift: boolean,
  size: number,
  options: { orientation: SplitOrientation; sizedPane: SizedPane; step: number; limits: SplitLimits },
): SplitKeyAction | null {
  const { orientation, sizedPane, step, limits } = options;
  const [back, forward] =
    orientation === "horizontal" ? ["ArrowLeft", "ArrowRight"] : ["ArrowUp", "ArrowDown"];
  const grow = sizedPane === "start" ? 1 : -1;
  const delta = shift ? step * 4 : step;

  switch (key) {
    case forward:
      return { type: "resize", size: clampOpen(size + grow * delta, limits) };
    case back:
      return { type: "resize", size: clampOpen(size - grow * delta, limits) };
    case "Home":
      return { type: "resize", size: Math.round(limits.min) };
    case "End":
      return { type: "resize", size: Math.round(limits.max) };
    case "Enter":
      return limits.collapsible ? { type: "toggle-collapse" } : null;
    default:
      return null;
  }
}

/** A key step never snaps a pane shut (that is Enter's job): clamp to the open range. */
function clampOpen(size: number, limits: SplitLimits): number {
  return Math.round(Math.min(limits.max, Math.max(limits.min, size)));
}

/**
 * The size stored under `key`, if any.
 *
 * @param key - The `localStorage` key.
 * @returns The stored size in pixels, or `null` when nothing valid is stored or storage
 *   cannot be read (private browsing, the server).
 */
export function readStoredSize(key: string): number | null {
  try {
    const raw = window.localStorage.getItem(key);
    if (raw === null) return null;
    const value = Number(raw);
    return Number.isFinite(value) && value >= 0 ? value : null;
  } catch {
    return null;
  }
}

/**
 * Remember a size under `key`. Failures (storage full, disabled, the server) are ignored: a
 * size that is not remembered is not worth an error.
 *
 * @param key - The `localStorage` key.
 * @param size - The size in pixels.
 */
export function writeStoredSize(key: string, size: number): void {
  try {
    window.localStorage.setItem(key, String(Math.round(size)));
  } catch {
    // Nothing to do: the size simply is not remembered.
  }
}
