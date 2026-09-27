/*
 * The React side of the playhead: subscribing to it, and driving it in real time.
 */

import { useEffect, useEffectEvent, useSyncExternalStore } from "react";

import { advance, type Playhead } from "./playhead";

/**
 * The playhead's current sample index, re-rendering the calling component when it changes.
 *
 * Use it only where the index is *displayed* (the playback bar, a readout). A render loop
 * reads `playhead.get()` in its own frame callback instead, and re-renders nothing.
 *
 * @param playhead - The store.
 * @returns The current sample index.
 */
export function usePlayhead(playhead: Playhead): number {
  return useSyncExternalStore(playhead.subscribe, playhead.get, playhead.get);
}

/**
 * The playhead's timeline — `count` and `dt` — re-rendering when `setTimeline` changes it.
 *
 * @param playhead - The store.
 * @returns The number of samples and the seconds between them.
 */
export function usePlayheadTimeline(playhead: Playhead): { count: number; dt: number } {
  const count = useSyncExternalStore(playhead.subscribe, () => playhead.count, () => playhead.count);
  const dt = useSyncExternalStore(playhead.subscribe, () => playhead.dt, () => playhead.dt);
  return { count, dt };
}

/** Options of `usePlaybackClock`. */
export interface PlaybackClockOptions {
  /** The store to advance. */
  playhead: Playhead;
  /** Whether the clock runs. */
  playing: boolean;
  /** Playback rate: 1 is real time. Defaults to 1. */
  speed?: number | undefined;
  /** Wrap from the last sample to the first instead of stopping. Defaults to false. */
  loop?: boolean | undefined;
  /** Called once when a non-looping timeline reaches its last sample; set `playing` false here. */
  onEnd?: (() => void) | undefined;
  /**
   * The longest wall-clock gap one frame may advance by, in milliseconds. A tab in the
   * background gets no animation frames, and without this cap the first frame back would
   * jump by the whole time away. Defaults to 100.
   */
  maxFrameMs?: number | undefined;
}

/**
 * Advance a playhead in real time × `speed` while `playing`, from `requestAnimationFrame`.
 *
 * Position is accumulated fractionally, so slow playback of a finely sampled timeline still
 * moves; the store only ever holds the integer index. A seek from elsewhere (the scrubber, a
 * marker) is picked up on the next frame. Starting a non-looping timeline that sits on its
 * last sample rewinds it to 0 first. `speed`, `loop` and `onEnd` may change while playing
 * without restarting the clock.
 *
 * Renders nothing and re-renders nothing: call it once, in the component that owns the
 * `playing` state.
 *
 * @param options - The playhead and the transport state.
 */
export function usePlaybackClock({
  playhead,
  playing,
  speed = 1,
  loop = false,
  onEnd,
  maxFrameMs = 100,
}: PlaybackClockOptions): void {
  const step = useEffectEvent((position: number, elapsedMs: number) => {
    const delta = (Math.min(elapsedMs, maxFrameMs) / 1000) * (speed / playhead.dt);
    return advance(position, delta, playhead.count, loop);
  });
  const end = useEffectEvent(() => onEnd?.());
  const rewindIfAtEnd = useEffectEvent(() => {
    if (!loop && playhead.count > 0 && playhead.get() >= playhead.count - 1) playhead.set(0);
  });

  useEffect(() => {
    if (!playing) return;
    rewindIfAtEnd();
    let position = playhead.get();
    let last: number | null = null;
    let frame = requestAnimationFrame(function tick(now: number) {
      // A seek since the last frame (scrubber, marker, step button) wins over the clock.
      if (Math.floor(position) !== playhead.get()) position = playhead.get();
      const result = last === null ? { position, ended: false } : step(position, now - last);
      last = now;
      position = result.position;
      playhead.set(Math.floor(position));
      if (result.ended) {
        end();
        return;
      }
      frame = requestAnimationFrame(tick);
    });
    return () => cancelAnimationFrame(frame);
  }, [playing, playhead]);
}
