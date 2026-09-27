/*
 * Where playback is, as a store outside React.
 *
 * A sampled timeline plays at display rate, and the things that follow it — a robot's joint
 * angles, a camera frustum, a highlighted capture — are drawn by a render loop, not by
 * React. If the current frame lived in React state, every frame of playback would re-render
 * every component that reads it, including the whole scene graph. So it lives here: a tiny
 * store a render loop reads with `get()` at no cost, and that React components subscribe to
 * (through `usePlayhead`) only where they display it — the playback bar, a time readout.
 *
 * The store holds an integer sample index and nothing else that changes per frame. The
 * sub-sample position real-time playback needs is the clock's business (`advance`,
 * `usePlaybackClock`), not the store's, so a reader never sees a fractional frame.
 */

/**
 * A sampled timeline's current position: an external store shared by React and render loops.
 *
 * Every function member may be called detached (`const { get } = playhead`), which is how
 * `useSyncExternalStore` and a render loop hold them.
 */
export interface Playhead {
  /** The current sample index, an integer in `0 … count − 1` (0 when `count` is 0). Cheap: call it every frame. */
  readonly get: () => number;
  /**
   * Move to sample `k`. Rounded to an integer and clamped to `0 … count − 1`; listeners are
   * notified only if the index changed.
   */
  readonly set: (k: number) => void;
  /**
   * Be told when the index or the timeline changes.
   *
   * @returns The unsubscribe function.
   */
  readonly subscribe: (listener: () => void) => () => void;
  /** How many samples the timeline has. */
  readonly count: number;
  /** The time between samples, in seconds. Sample `k` is at `t = k · dt`. */
  readonly dt: number;
  /**
   * Swap in a new timeline — a newly loaded or re-baked scenario — keeping the one store
   * every subscriber already holds. The index is clamped into the new range (not reset:
   * call `set(0)` for that). Listeners are notified if anything changed.
   */
  readonly setTimeline: (count: number, dt: number) => void;
}

/**
 * A playhead over `count` samples spaced `dt` seconds apart, starting at sample 0.
 *
 * Create one per app (not per render — keep it in a `useState` initialiser or module
 * scope), hand it to `PlaybackBar` and to the render loop, and replace its timeline with
 * `setTimeline` when a new scenario loads.
 *
 * @param count - The number of samples (a non-negative integer).
 * @param dt - Seconds between samples (positive).
 * @returns The store.
 */
export function createPlayhead(count: number, dt: number): Playhead {
  let index = 0;
  let timeline = checkTimeline(count, dt);
  const listeners = new Set<() => void>();
  const notify = () => {
    for (const listener of [...listeners]) listener();
  };

  return {
    get: () => index,
    set: (k) => {
      const next = clampIndex(k, timeline.count);
      if (next === index) return;
      index = next;
      notify();
    },
    subscribe: (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    get count() {
      return timeline.count;
    },
    get dt() {
      return timeline.dt;
    },
    setTimeline: (nextCount, nextDt) => {
      const next = checkTimeline(nextCount, nextDt);
      const nextIndex = clampIndex(index, next.count);
      if (next.count === timeline.count && next.dt === timeline.dt && nextIndex === index) return;
      timeline = next;
      index = nextIndex;
      notify();
    },
  };
}

function checkTimeline(count: number, dt: number): { count: number; dt: number } {
  if (!Number.isInteger(count) || count < 0) {
    throw new RangeError(`playhead: count must be a non-negative integer, got ${count}`);
  }
  if (!(dt > 0) || !Number.isFinite(dt)) {
    throw new RangeError(`playhead: dt must be a positive number of seconds, got ${dt}`);
  }
  return { count, dt };
}

/**
 * A sample index made legal: rounded, and clamped to `0 … count − 1`.
 *
 * @param k - Any number (`NaN` reads as 0).
 * @param count - The number of samples.
 * @returns The index; 0 when `count` is 0.
 */
export function clampIndex(k: number, count: number): number {
  if (count <= 0 || Number.isNaN(k)) return 0;
  return Math.min(count - 1, Math.max(0, Math.round(k)));
}

/** The outcome of one clock step: the new fractional position, and whether playback ran off the end. */
export interface AdvanceResult {
  /** The new position in samples (fractional); the index to show is its floor. */
  position: number;
  /** True when a non-looping timeline reached its last sample on this step. */
  ended: boolean;
}

/**
 * One step of real-time playback, in samples.
 *
 * Each sample is shown for `dt`: a looping timeline has period `count · dt` and wraps from
 * the last sample to the first; a non-looping one stops on the last sample and reports
 * `ended`. The position is fractional so that playback slower than one sample per frame
 * still advances.
 *
 * @param position - The current position in samples (fractional).
 * @param delta - How many samples to advance (elapsed seconds × speed ÷ dt); not negative.
 * @param count - The number of samples.
 * @param loop - Whether to wrap at the end.
 * @returns The new position and whether playback ended.
 */
export function advance(position: number, delta: number, count: number, loop: boolean): AdvanceResult {
  if (count <= 1) return { position: 0, ended: !loop };
  const next = position + Math.max(0, delta);
  if (loop) return { position: next % count, ended: false };
  const last = count - 1;
  return next >= last ? { position: last, ended: true } : { position: next, ended: false };
}

/**
 * The default time readout: seconds with millisecond resolution, `"1.234 s"`.
 *
 * @param seconds - The time.
 * @returns The formatted time.
 */
export function formatSeconds(seconds: number): string {
  return `${seconds.toFixed(3)} s`;
}
