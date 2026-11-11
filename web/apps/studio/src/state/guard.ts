/**
 * Unsaved work is not lost by closing the studio. In the Tauri shell the window's close asks
 * first (the studio's own dialog); in a browser, closing or reloading the tab with unsaved
 * changes gets the browser's leave-page prompt.
 */

import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect } from "react";

import { isTauri } from "../io/tauri";
import type { Studio } from "./studio";

/** Ask before the window or tab closes while `studio` has unsaved changes. */
export function useUnsavedGuard(studio: Studio): void {
  useEffect(() => {
    if (isTauri()) {
      let unlisten: (() => void) | null = null;
      let gone = false;
      getCurrentWindow()
        .onCloseRequested(async (event) => {
          if (!(await studio.mayDiscard("Closing the window"))) event.preventDefault();
        })
        .then((stop) => {
          if (gone) stop();
          else unlisten = stop;
        })
        .catch((e: unknown) => console.warn(`close guard: ${e instanceof Error ? e.message : String(e)}`));
      return () => {
        gone = true;
        unlisten?.();
      };
    }
    const onBeforeUnload = (e: BeforeUnloadEvent) => {
      if (studio.get().dirty) e.preventDefault();
    };
    window.addEventListener("beforeunload", onBeforeUnload);
    return () => window.removeEventListener("beforeunload", onBeforeUnload);
  }, [studio]);
}
