import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { useSceneColors } from "./colors";

afterEach(() => {
  document.documentElement.className = "";
  document.documentElement.removeAttribute("style");
});

describe("useSceneColors", () => {
  it("re-reads the tokens when the theme class flips, with one shared observer", async () => {
    document.documentElement.style.setProperty("--signal", "teal");
    const a = renderHook(() => useSceneColors());
    const b = renderHook(() => useSceneColors());
    expect(a.result.current.signal).toBe("teal");
    await act(async () => {
      document.documentElement.style.setProperty("--signal", "cyan");
      document.documentElement.classList.add("dark");
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(a.result.current.signal).toBe("cyan");
    expect(b.result.current).toBe(a.result.current);
    a.unmount();
    b.unmount();
    // With no subscriber left the cache is dropped: a new reader sees current values.
    document.documentElement.style.setProperty("--signal", "navy");
    expect(renderHook(() => useSceneColors()).result.current.signal).toBe("navy");
  });
});
