import { act, render, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { createPlayhead } from "./playhead";
import { usePlaybackClock, usePlayhead, usePlayheadTimeline } from "./usePlayback";

/** A hand-cranked `requestAnimationFrame`: `frame(ms)` runs the pending callback at time `ms`. */
function manualFrames() {
  let pending: FrameRequestCallback | null = null;
  let id = 0;
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    pending = callback;
    id += 1;
    return id;
  });
  vi.stubGlobal("cancelAnimationFrame", () => {
    pending = null;
  });
  return {
    frame(now: number) {
      const callback = pending;
      pending = null;
      act(() => callback?.(now));
    },
    get pending() {
      return pending !== null;
    },
  };
}

describe("usePlayhead", () => {
  it("re-renders with the index, and with the timeline", () => {
    const playhead = createPlayhead(10, 0.5);
    const { result } = renderHook(() => ({ index: usePlayhead(playhead), timeline: usePlayheadTimeline(playhead) }));
    expect(result.current.index).toBe(0);
    act(() => playhead.set(4));
    expect(result.current.index).toBe(4);
    act(() => playhead.setTimeline(20, 0.25));
    expect(result.current.timeline).toEqual({ count: 20, dt: 0.25 });
  });

  it("does not re-render a component that only reads get()", () => {
    const playhead = createPlayhead(10, 0.5);
    const renders = vi.fn();
    function Reader() {
      renders();
      return null;
    }
    render(<Reader />);
    act(() => playhead.set(5));
    expect(renders).toHaveBeenCalledTimes(1);
  });
});

describe("usePlaybackClock", () => {
  let frames: ReturnType<typeof manualFrames>;
  beforeEach(() => {
    frames = manualFrames();
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("advances in real time × speed", () => {
    const playhead = createPlayhead(1000, 0.01); // 100 samples per second
    renderHook(() => usePlaybackClock({ playhead, playing: true, speed: 2 }));
    frames.frame(1000); // the first frame only sets the clock's origin
    expect(playhead.get()).toBe(0);
    frames.frame(1050); // 50 ms × 2 = 10 samples
    expect(playhead.get()).toBe(10);
    frames.frame(1100);
    expect(playhead.get()).toBe(20);
  });

  it("accumulates fractions of a sample across frames", () => {
    const playhead = createPlayhead(100, 1); // one sample per second
    renderHook(() => usePlaybackClock({ playhead, playing: true, speed: 0.25 }));
    frames.frame(0);
    for (let t = 100; t <= 4000; t += 100) frames.frame(t);
    expect(playhead.get()).toBe(1);
  });

  it("caps a long gap between frames", () => {
    const playhead = createPlayhead(1000, 0.01);
    renderHook(() => usePlaybackClock({ playhead, playing: true, maxFrameMs: 50 }));
    frames.frame(0);
    frames.frame(10_000);
    expect(playhead.get()).toBe(5);
  });

  it("picks up a seek made between frames", () => {
    const playhead = createPlayhead(1000, 0.01);
    renderHook(() => usePlaybackClock({ playhead, playing: true }));
    frames.frame(0);
    frames.frame(50);
    expect(playhead.get()).toBe(5);
    act(() => playhead.set(500));
    frames.frame(100);
    expect(playhead.get()).toBe(505);
  });

  it("stops at the end and calls onEnd once, when not looping", () => {
    const playhead = createPlayhead(10, 0.01);
    const onEnd = vi.fn();
    renderHook(() => usePlaybackClock({ playhead, playing: true, onEnd }));
    frames.frame(0);
    frames.frame(90);
    expect(playhead.get()).toBe(9);
    expect(onEnd).toHaveBeenCalledTimes(1);
    expect(frames.pending).toBe(false);
  });

  it("wraps when looping", () => {
    const playhead = createPlayhead(10, 0.01);
    renderHook(() => usePlaybackClock({ playhead, playing: true, loop: true }));
    frames.frame(0);
    frames.frame(95); // 9.5 samples
    frames.frame(125); // 12.5 → wraps to 2.5
    expect(playhead.get()).toBe(2);
    expect(frames.pending).toBe(true);
  });

  it("rewinds a finished timeline when started again", () => {
    const playhead = createPlayhead(10, 0.01);
    playhead.set(9);
    renderHook(() => usePlaybackClock({ playhead, playing: true }));
    expect(playhead.get()).toBe(0);
  });

  it("does nothing while paused, and cancels its frame on pause", () => {
    const playhead = createPlayhead(10, 0.01);
    const { rerender } = renderHook(({ playing }) => usePlaybackClock({ playhead, playing }), {
      initialProps: { playing: false },
    });
    expect(frames.pending).toBe(false);
    rerender({ playing: true });
    expect(frames.pending).toBe(true);
    rerender({ playing: false });
    expect(frames.pending).toBe(false);
  });

  it("takes a new speed without restarting", () => {
    const playhead = createPlayhead(1000, 0.01);
    const { rerender } = renderHook(({ speed }) => usePlaybackClock({ playhead, playing: true, speed }), {
      initialProps: { speed: 1 },
    });
    frames.frame(0);
    frames.frame(50);
    expect(playhead.get()).toBe(5);
    rerender({ speed: 4 });
    frames.frame(100);
    expect(playhead.get()).toBe(25);
  });
});
