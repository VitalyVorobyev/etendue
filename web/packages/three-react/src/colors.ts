import { observeSceneColors, readSceneColors, type SceneColors } from "@vitavision/three";
import { createContext, createElement, type JSX, type ReactNode, use, useMemo, useSyncExternalStore } from "react";

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

/*
 * On the server there is no document to read tokens from: every colour is `gray`, as for a
 * token no stylesheet defines. Hydration starts from the same value, then reads the real ones.
 */
const SERVER_COLORS: SceneColors = Object.freeze({
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
});
const serverSnapshot = (): SceneColors => SERVER_COLORS;

const OverrideContext = createContext<Partial<SceneColors> | null>(null);

/** Props of {@link SceneColorsProvider}. */
export interface SceneColorsProviderProps {
  /**
   * Colours to use instead of the `@vitavision/ui` tokens, for an app whose palette does not
   * define them. Any CSS colour three.js parses; keys left out still follow the tokens.
   */
  colors: Partial<SceneColors>;
  /** The subtree that sees the overrides. */
  children?: ReactNode;
}

/**
 * Override scene colours for everything inside (see {@link useSceneColors}). Overrides apply
 * on the server too, so an app palette renders the same before and after hydration.
 */
export function SceneColorsProvider({ colors, children }: SceneColorsProviderProps): JSX.Element {
  return createElement(OverrideContext, { value: colors }, children);
}

/**
 * The scene colours (`SceneColors` from `@vitavision/three`): the vitavision tokens, re-read
 * whenever the theme class on the document element changes (one observer shared by every
 * caller), with any {@link SceneColorsProvider} overrides applied. Server-safe: a server
 * render gets neutral `gray` for every colour a provider does not override.
 */
export function useSceneColors(): SceneColors {
  const tokens = useSyncExternalStore(subscribe, snapshot, serverSnapshot);
  const override = use(OverrideContext);
  return useMemo(() => (override ? { ...tokens, ...override } : tokens), [tokens, override]);
}
