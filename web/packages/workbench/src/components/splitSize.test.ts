import { afterEach, describe, expect, it, vi } from "vitest";

import {
  clampSize,
  readStoredSize,
  resolveLimits,
  resolveSize,
  splitKeyAction,
  writeStoredSize,
} from "./splitSize";

describe("resolveSize", () => {
  it("passes pixels through and resolves percentages against the length", () => {
    expect(resolveSize(240, 1000)).toBe(240);
    expect(resolveSize("30%", 1000)).toBe(300);
    expect(resolveSize("12.5%", 800)).toBe(100);
  });

  it("never returns a negative or non-finite size", () => {
    expect(resolveSize(-10, 1000)).toBe(0);
    expect(resolveSize("abc%" as `${number}%`, 1000)).toBe(0);
  });
});

describe("resolveLimits", () => {
  it("defaults to the whole range", () => {
    expect(resolveLimits(900)).toEqual({ min: 0, max: 900, collapsible: false });
  });

  it("caps max at the length and min at max", () => {
    expect(resolveLimits(400, 500, 2000, true)).toEqual({ min: 400, max: 400, collapsible: true });
    expect(resolveLimits(1000, "10%", "40%")).toEqual({ min: 100, max: 400, collapsible: false });
  });
});

describe("clampSize", () => {
  const limits = { min: 200, max: 600, collapsible: false };

  it("holds a size to the limits, in whole pixels", () => {
    expect(clampSize(50, limits)).toBe(200);
    expect(clampSize(900, limits)).toBe(600);
    expect(clampSize(333.4, limits)).toBe(333);
  });

  it("snaps a collapsible pane shut below half its minimum", () => {
    const collapsible = { ...limits, collapsible: true };
    expect(clampSize(99, collapsible)).toBe(0);
    expect(clampSize(100, collapsible)).toBe(200);
    expect(clampSize(150, collapsible)).toBe(200);
  });
});

describe("splitKeyAction", () => {
  const limits = { min: 100, max: 500, collapsible: false };
  const horizontalStart = { orientation: "horizontal", sizedPane: "start", step: 10, limits } as const;

  it("moves the divider the way the arrow points", () => {
    expect(splitKeyAction("ArrowRight", false, 200, horizontalStart)).toEqual({ type: "resize", size: 210 });
    expect(splitKeyAction("ArrowLeft", false, 200, horizontalStart)).toEqual({ type: "resize", size: 190 });
    // The sized pane on the other side shrinks when the divider moves towards it.
    const end = { ...horizontalStart, sizedPane: "end" } as const;
    expect(splitKeyAction("ArrowRight", false, 200, end)).toEqual({ type: "resize", size: 190 });
  });

  it("uses the vertical arrows for a vertical split and ignores the others", () => {
    const vertical = { ...horizontalStart, orientation: "vertical" } as const;
    expect(splitKeyAction("ArrowDown", false, 200, vertical)).toEqual({ type: "resize", size: 210 });
    expect(splitKeyAction("ArrowUp", false, 200, vertical)).toEqual({ type: "resize", size: 190 });
    expect(splitKeyAction("ArrowRight", false, 200, vertical)).toBeNull();
  });

  it("takes four steps with Shift, and clamps without collapsing", () => {
    expect(splitKeyAction("ArrowRight", true, 200, horizontalStart)).toEqual({ type: "resize", size: 240 });
    const collapsible = { ...horizontalStart, limits: { ...limits, collapsible: true } };
    expect(splitKeyAction("ArrowLeft", true, 110, collapsible)).toEqual({ type: "resize", size: 100 });
    expect(splitKeyAction("ArrowRight", true, 490, horizontalStart)).toEqual({ type: "resize", size: 500 });
  });

  it("goes to the limits on Home and End", () => {
    expect(splitKeyAction("Home", false, 300, horizontalStart)).toEqual({ type: "resize", size: 100 });
    expect(splitKeyAction("End", false, 300, horizontalStart)).toEqual({ type: "resize", size: 500 });
  });

  it("toggles collapse on Enter only when collapsible", () => {
    expect(splitKeyAction("Enter", false, 300, horizontalStart)).toBeNull();
    const collapsible = { ...horizontalStart, limits: { ...limits, collapsible: true } };
    expect(splitKeyAction("Enter", false, 300, collapsible)).toEqual({ type: "toggle-collapse" });
    expect(splitKeyAction("a", false, 300, collapsible)).toBeNull();
  });
});

describe("stored sizes", () => {
  afterEach(() => {
    window.localStorage.clear();
    vi.restoreAllMocks();
  });

  it("round-trips a size in whole pixels", () => {
    writeStoredSize("split", 312.6);
    expect(window.localStorage.getItem("split")).toBe("313");
    expect(readStoredSize("split")).toBe(313);
  });

  it("reads nothing for a missing or invalid value", () => {
    expect(readStoredSize("missing")).toBeNull();
    window.localStorage.setItem("bad", "wide");
    expect(readStoredSize("bad")).toBeNull();
    window.localStorage.setItem("negative", "-4");
    expect(readStoredSize("negative")).toBeNull();
  });

  it("survives storage that throws", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("denied");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("denied");
    });
    expect(readStoredSize("split")).toBeNull();
    expect(() => writeStoredSize("split", 10)).not.toThrow();
  });
});
