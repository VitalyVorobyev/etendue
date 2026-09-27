import { observeSceneColors, readSceneColors, type SceneColors } from "@vitavision/three";
import { createContext, createElement, type ReactNode, use, useMemo, useSyncExternalStore } from "react";

let cached: SceneColors | undefined;
const listeners = new Set<() => void>();
let stop: (() => void) | undefined;

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  stop ??= observeSceneColors((colors) => {
    cached = colors;
    for (const l of listeners) l();
  });
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0) {
      stop?.();
      stop = undefined;
      cached = undefined;
    }
  };
}

function snapshot(): SceneColors {
  cached ??= readSceneColors();
  return cached;
}

/** Neutral colours for server rendering, where there is no document to read tokens from. */
const SERVER: SceneColors = {
  background: "gray",
  canvas: "gray",
  surface: "gray",
  fg: "gray",
  muted: "gray",
  line: "gray",
  lineStrong: "gray",
  signal: "gray",
  normal: "gray",
  defect: "gray",
  warn: "gray",
};

function serverSnapshot(): SceneColors {
  return typeof document === "undefined" ? SERVER : snapshot();
}

const OverrideContext = createContext<Partial<SceneColors> | null>(null);

/** Props of {@link SceneColorsProvider}. */
export interface SceneColorsProviderProps {
  /**
   * Colours to use instead of the `@vitavision/ui` tokens — for an app whose palette does not
   * define them. Any CSS colour three.js parses; keys left out still follow the tokens.
   */
  colors: Partial<SceneColors>;
  children?: ReactNode;
}

/** Override scene colours for everything inside (see {@link useSceneColors}). */
export function SceneColorsProvider({ colors, children }: SceneColorsProviderProps) {
  return createElement(OverrideContext, { value: colors }, children);
}

/**
 * The scene colours ({@link SceneColors}): the vitavision tokens, re-read whenever the theme
 * class on the document element changes (one observer shared by every caller), with any
 * {@link SceneColorsProvider} overrides applied.
 */
export function useSceneColors(): SceneColors {
  const tokens = useSyncExternalStore(subscribe, snapshot, serverSnapshot);
  const override = use(OverrideContext);
  return useMemo(() => (override ? { ...tokens, ...override } : tokens), [tokens, override]);
}
