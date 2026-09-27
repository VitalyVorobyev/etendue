import { afterEach, describe, expect, it } from "vitest";

import { observeSceneColors, readSceneColors } from "./theme";

afterEach(() => {
  document.documentElement.className = "";
  document.documentElement.removeAttribute("style");
});

describe("scene colours", () => {
  it("reads the tokens, falling back to gray", () => {
    document.documentElement.style.setProperty("--signal", " rgb(1, 2, 3) ");
    const c = readSceneColors();
    expect(c.signal).toBe("rgb(1, 2, 3)");
    expect(c.defect).toBe("gray");
  });

  it("re-reads when the theme class flips", async () => {
    const seen: string[] = [];
    const stop = observeSceneColors((c) => seen.push(c.fg));
    document.documentElement.style.setProperty("--fg", "white");
    document.documentElement.classList.add("dark");
    await new Promise((r) => setTimeout(r, 0));
    stop();
    document.documentElement.classList.remove("dark");
    await new Promise((r) => setTimeout(r, 0));
    expect(seen).toEqual(["white"]);
  });
});
