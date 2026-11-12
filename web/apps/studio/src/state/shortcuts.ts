/**
 * Undo, redo and save from the keyboard, except inside a text field (which has its own undo), and
 * only where the open cell is shown: elsewhere they would change or save something out of sight.
 */

import { useEffect } from "react";

import type { Studio } from "./studio";

/** Whether a key event belongs to a text field. */
function inField(e: KeyboardEvent): boolean {
  const t = e.target as HTMLElement | null;
  return t !== null && (t.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(t.tagName));
}

export function useEditShortcuts(studio: Studio, sceneShown: boolean): void {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || e.altKey) return;
      const key = e.key.toLowerCase();
      if (key === "s") {
        // The browser's own Save is never what the user means here.
        e.preventDefault();
        if (sceneShown) void studio.save(e.shiftKey);
      } else if ((key === "z" || key === "y") && !inField(e) && sceneShown) {
        e.preventDefault();
        if (key === "y" || e.shiftKey) studio.redo();
        else studio.undo();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [studio, sceneShown]);
}
