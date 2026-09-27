import type { Playhead } from "@vitavision/workbench";
import { useEffect, useState } from "react";

/**
 * The playhead's index, re-rendering at most every `ms` milliseconds — for views too heavy to
 * redraw on every animation frame (charts). The last change always lands.
 */
export function useThrottledPlayhead(playhead: Playhead, ms: number): number {
  const [k, setK] = useState(playhead.get);
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let last = 0;
    const flush = () => {
      timer = undefined;
      last = performance.now();
      setK(playhead.get());
    };
    const unsubscribe = playhead.subscribe(() => {
      if (timer !== undefined) return;
      timer = setTimeout(flush, Math.max(0, ms - (performance.now() - last)));
    });
    return () => {
      unsubscribe();
      if (timer !== undefined) clearTimeout(timer);
    };
  }, [playhead, ms]);
  return k;
}
