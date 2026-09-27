import { describe, expect, it, vi } from "vitest";

import { advance, clampIndex, createPlayhead, formatSeconds } from "./playhead";

describe("createPlayhead", () => {
  it("starts at 0 and exposes its timeline", () => {
    const playhead = createPlayhead(100, 0.01);
    expect(playhead.get()).toBe(0);
    expect(playhead.count).toBe(100);
    expect(playhead.dt).toBe(0.01);
  });

  it("rounds and clamps what it is set to", () => {
    const playhead = createPlayhead(10, 0.1);
    playhead.set(3.6);
    expect(playhead.get()).toBe(4);
    playhead.set(99);
    expect(playhead.get()).toBe(9);
    playhead.set(-5);
    expect(playhead.get()).toBe(0);
  });

  it("notifies only on a change, and stops after unsubscribe", () => {
    const playhead = createPlayhead(10, 0.1);
    const listener = vi.fn();
    const unsubscribe = playhead.subscribe(listener);
    playhead.set(2);
    playhead.set(2);
    playhead.set(2.2);
    expect(listener).toHaveBeenCalledTimes(1);
    unsubscribe();
    playhead.set(5);
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("works with detached members", () => {
    const { get, set } = createPlayhead(10, 0.1);
    set(7);
    expect(get()).toBe(7);
  });

  it("swaps timelines in place, clamping the index", () => {
    const playhead = createPlayhead(100, 0.01);
    const listener = vi.fn();
    playhead.subscribe(listener);
    playhead.set(80);
    listener.mockClear();

    playhead.setTimeline(50, 0.02);
    expect(playhead.get()).toBe(49);
    expect(playhead.count).toBe(50);
    expect(playhead.dt).toBe(0.02);
    expect(listener).toHaveBeenCalledTimes(1);

    playhead.setTimeline(50, 0.02);
    expect(listener).toHaveBeenCalledTimes(1);

    playhead.setTimeline(0, 0.02);
    expect(playhead.get()).toBe(0);
    playhead.set(3);
    expect(playhead.get()).toBe(0);
  });

  it("rejects an impossible timeline", () => {
    expect(() => createPlayhead(-1, 0.1)).toThrow(RangeError);
    expect(() => createPlayhead(2.5, 0.1)).toThrow(RangeError);
    expect(() => createPlayhead(10, 0)).toThrow(RangeError);
    expect(() => createPlayhead(10, Number.POSITIVE_INFINITY)).toThrow(RangeError);
    expect(() => createPlayhead(10, 0.1).setTimeline(10, -1)).toThrow(RangeError);
  });
});

describe("clampIndex", () => {
  it("makes any number a legal index", () => {
    expect(clampIndex(Number.NaN, 10)).toBe(0);
    expect(clampIndex(4, 0)).toBe(0);
    expect(clampIndex(4.5, 10)).toBe(5);
  });
});

describe("advance", () => {
  it("accumulates fractional samples", () => {
    let position = 0;
    for (let frame = 0; frame < 10; frame += 1) position = advance(position, 0.25, 100, false).position;
    expect(position).toBeCloseTo(2.5, 12);
  });

  it("stops on the last sample and reports the end when not looping", () => {
    expect(advance(8.5, 0.4, 10, false)).toEqual({ position: 8.9, ended: false });
    expect(advance(8.5, 0.5, 10, false)).toEqual({ position: 9, ended: true });
    expect(advance(8.5, 30, 10, false)).toEqual({ position: 9, ended: true });
  });

  it("wraps with period count when looping", () => {
    const { position, ended } = advance(9.5, 1, 10, true);
    expect(ended).toBe(false);
    expect(position).toBeCloseTo(0.5, 12);
  });

  it("never runs backwards, and handles degenerate timelines", () => {
    expect(advance(3, -2, 10, false)).toEqual({ position: 3, ended: false });
    expect(advance(0, 1, 1, false)).toEqual({ position: 0, ended: true });
    expect(advance(0, 1, 0, true)).toEqual({ position: 0, ended: false });
  });
});

describe("formatSeconds", () => {
  it("prints milliseconds", () => {
    expect(formatSeconds(1.2344)).toBe("1.234 s");
    expect(formatSeconds(0)).toBe("0.000 s");
  });
});
