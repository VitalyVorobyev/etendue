import { observeSceneColors, readSceneColors, type SceneColors } from "@vitavision/three";
import { useSyncExternalStore } from "react";

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

/**
 * The vitavision scene colours ({@link SceneColors}), re-read whenever the theme class on
 * the document element changes. One observer is shared by every caller.
 */
export function useSceneColors(): SceneColors {
  return useSyncExternalStore(subscribe, snapshot, snapshot);
}
