import { act, renderHook } from "@testing-library/react";
import { createElement, type ReactNode } from "react";
import { renderToString } from "react-dom/server";
import { afterEach, describe, expect, it } from "vitest";

import { SceneColorsProvider, useSceneColors } from "./colors";

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

  it("renders on the server with neutral colours, reading no tokens", () => {
    document.documentElement.style.setProperty("--signal", "teal");
    function Probe() {
      return createElement("span", null, useSceneColors().signal);
    }
    expect(renderToString(createElement(Probe))).toBe("<span>gray</span>");
    // Provider overrides apply on the server too.
    const withProvider = createElement(SceneColorsProvider, { colors: { signal: "orange" } }, createElement(Probe));
    expect(renderToString(withProvider)).toBe("<span>orange</span>");
  });

  it("applies provider overrides over the tokens", () => {
    document.documentElement.style.setProperty("--signal", "teal");
    const wrapper = ({ children }: { children: ReactNode }) =>
      createElement(SceneColorsProvider, { colors: { signal: "orange" } }, children);
    const { result } = renderHook(() => useSceneColors(), { wrapper });
    expect(result.current.signal).toBe("orange");
    expect(result.current.fg).toBe(renderHook(() => useSceneColors()).result.current.fg);
  });
});
