/*
 * The transport for a sampled timeline.
 *
 * Playback state splits by how often it changes. Whether it is playing, how fast, and
 * whether it loops change when someone clicks — ordinary controlled props. Where it is
 * changes every frame — so that is a `Playhead` store the bar subscribes to itself, and a
 * playing timeline re-renders this bar and nothing else in the app.
 *
 * The bar is chrome over a viewport, like the image stage's toolbar: one row, icon buttons
 * named for what they do, the time in mono so it does not jitter as digits change.
 */

import {
  ChevronFirst,
  ChevronLast,
  Pause,
  Play,
  Repeat,
  SkipBack,
  SkipForward,
  StepBack,
  StepForward,
} from "lucide-react";
import type { ComponentPropsWithRef, KeyboardEvent, ReactNode } from "react";

import { type MeasureTone, Select, Slider, byDensity, cn, focusRing, toneColor, useDensity } from "@vitavision/ui";

import { formatSeconds, type Playhead } from "./playhead";
import { usePlayhead, usePlayheadTimeline } from "./usePlayback";

/** A point of interest on the timeline — a capture, an event — drawn as a tick on the scrubber. */
export interface PlaybackMarker {
  /** The sample the marker sits on. */
  index: number;
  /** What happens there; names the tick for assistive technology and its tooltip. */
  label?: string | undefined;
  /** The tick's colour. Defaults to `signal`. */
  tone?: MeasureTone | undefined;
}

/** The speeds offered by default. */
export const DEFAULT_SPEEDS: readonly number[] = [0.25, 0.5, 1, 2, 4];

/** Props of `PlaybackBar`. */
export interface PlaybackBarProps {
  /** Where playback is. The bar subscribes to it; seeking writes to it. */
  playhead: Playhead;
  /** Whether playback is running, controlled. */
  playing: boolean;
  /** Called with the requested playing state (the play/pause button, Space). */
  onPlayingChange: (playing: boolean) => void;
  /** The playback rate, controlled. Defaults to 1. */
  speed?: number | undefined;
  /** Called with the chosen rate. Without it the speed selector is not shown. */
  onSpeedChange?: ((speed: number) => void) | undefined;
  /** The rates the selector offers. Defaults to 0.25×, 0.5×, 1×, 2×, 4×. */
  speeds?: readonly number[] | undefined;
  /** Whether playback wraps at the end, controlled. */
  loop?: boolean | undefined;
  /** Called with the new loop state. Without it the loop toggle is not shown. */
  onLoopChange?: ((loop: boolean) => void) | undefined;
  /** Ticks on the scrubber, each a click target that seeks to it. */
  markers?: readonly PlaybackMarker[] | undefined;
  /** Called after a marker tick is clicked (the playhead has already moved to it). */
  onMarkerSelect?: ((marker: PlaybackMarker) => void) | undefined;
  /** Formats `t = k · dt` in seconds for the readout. Defaults to `"1.234 s"`. */
  formatTime?: ((seconds: number) => string) | undefined;
  /** Names the bar (a `toolbar`). Defaults to "Playback". */
  "aria-label"?: string | undefined;
  /** Merged with the bar's own classes through `cn`. */
  className?: string | undefined;
}

/**
 * The transport for a sampled timeline: jump to start, previous marker, step back,
 * play/pause, step forward, next marker, jump to end; a scrubber over the samples with the
 * markers as ticks; the time `t = k · dt`; the speed; loop.
 *
 * `playing`, `speed` and `loop` are controlled props. The position is the `playhead` store,
 * which the bar subscribes to itself — so during playback only the bar re-renders. Drive the
 * store with `usePlaybackClock`, and read `playhead.get()` in a render loop.
 *
 * A `role="toolbar"`. Keyboard, while focus is inside it: the scrubber is a slider (←/→ one
 * sample, PageUp/PageDown ten, Home/End); on the buttons, ←/→ step one sample; Space
 * toggles playback anywhere in the bar except on a button, where it presses the button. The
 * root carries `data-playing` while playing and `data-empty` when the timeline has no samples.
 */
export function PlaybackBar({
  playhead,
  playing,
  onPlayingChange,
  speed = 1,
  onSpeedChange,
  speeds = DEFAULT_SPEEDS,
  loop = false,
  onLoopChange,
  markers = [],
  onMarkerSelect,
  formatTime = formatSeconds,
  "aria-label": ariaLabel = "Playback",
  className,
}: PlaybackBarProps) {
  const density = useDensity();
  const index = usePlayhead(playhead);
  const { count, dt } = usePlayheadTimeline(playhead);
  const last = Math.max(0, count - 1);
  const empty = count === 0;

  const sorted = [...markers].filter((m) => m.index >= 0 && m.index <= last).sort((a, b) => a.index - b.index);
  const previousMarker = sorted.filter((m) => m.index < index).at(-1);
  const nextMarker = sorted.find((m) => m.index > index);

  const seek = (k: number) => playhead.set(k);

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const target = event.target as HTMLElement;
    const onButton = target.tagName === "BUTTON";
    if (event.key === " " && !onButton && target.getAttribute("role") !== "combobox") {
      event.preventDefault();
      if (!empty) onPlayingChange(!playing);
    } else if (onButton && (event.key === "ArrowLeft" || event.key === "ArrowRight")) {
      event.preventDefault();
      seek(index + (event.key === "ArrowRight" ? 1 : -1));
    }
  };

  return (
    <div
      role="toolbar"
      aria-label={ariaLabel}
      data-playing={playing ? "" : undefined}
      data-empty={empty ? "" : undefined}
      onKeyDown={onKeyDown}
      className={cn(
        "flex w-full min-w-0 items-center bg-surface",
        byDensity(density, "gap-3 px-3 py-2", "gap-2 px-2 py-1"),
        className,
      )}
    >
      <div className="flex shrink-0 items-center gap-0.5">
        <BarButton label="Jump to start" onClick={() => seek(0)} disabled={empty || index === 0}>
          <ChevronFirst className="size-4" aria-hidden />
        </BarButton>
        <BarButton
          label="Previous marker"
          onClick={() => previousMarker && seek(previousMarker.index)}
          disabled={previousMarker === undefined}
        >
          <SkipBack className="size-4" aria-hidden />
        </BarButton>
        <BarButton label="Step back" onClick={() => seek(index - 1)} disabled={empty || index === 0}>
          <StepBack className="size-4" aria-hidden />
        </BarButton>
        <BarButton
          label={playing ? "Pause" : "Play"}
          onClick={() => onPlayingChange(!playing)}
          disabled={empty}
          className="text-fg"
        >
          {playing ? <Pause className="size-4" aria-hidden /> : <Play className="size-4" aria-hidden />}
        </BarButton>
        <BarButton label="Step forward" onClick={() => seek(index + 1)} disabled={empty || index >= last}>
          <StepForward className="size-4" aria-hidden />
        </BarButton>
        <BarButton
          label="Next marker"
          onClick={() => nextMarker && seek(nextMarker.index)}
          disabled={nextMarker === undefined}
        >
          <SkipForward className="size-4" aria-hidden />
        </BarButton>
        <BarButton label="Jump to end" onClick={() => seek(last)} disabled={empty || index >= last}>
          <ChevronLast className="size-4" aria-hidden />
        </BarButton>
      </div>

      <div className="relative flex min-w-24 flex-1 flex-col justify-center">
        {sorted.length > 0 && (
          // The ticks sit on a rail above the track, positioned as Radix positions the thumb:
          // its centre travels from half a thumb (7 px) in from either end.
          <div className="relative h-2.5">
            {sorted.map((marker) => (
              <MarkerTick
                key={`${marker.index}:${marker.label ?? ""}`}
                marker={marker}
                fraction={last === 0 ? 0 : marker.index / last}
                time={formatTime(marker.index * dt)}
                active={marker.index === index}
                onClick={() => {
                  seek(marker.index);
                  onMarkerSelect?.(marker);
                }}
              />
            ))}
          </div>
        )}
        <Slider
          value={index}
          onValueChange={seek}
          min={0}
          max={last}
          step={1}
          disabled={empty}
          aria-label="Playback position"
        />
      </div>

      {/* Not an <output>: that is a live region, and this changes every frame. */}
      <span className="shrink-0 font-mono text-xs whitespace-nowrap text-fg tabular-nums">
        <span className="text-fg-subtle">t = </span>
        {formatTime(index * dt)}
        <span className="ml-2 text-fg-subtle">
          {empty ? "–" : index}/{last}
        </span>
      </span>

      {onSpeedChange && (
        <Select
          value={String(speed)}
          onValueChange={(value) => value !== "" && onSpeedChange(Number(value))}
          options={speeds.map((s) => ({ value: String(s), label: `${s}×` }))}
          aria-label="Playback speed"
          className={cn("w-20 shrink-0 font-mono", byDensity(density, "h-8", "h-7"))}
        />
      )}

      {onLoopChange && (
        <BarButton label="Loop" pressed={loop} onClick={() => onLoopChange(!loop)}>
          <Repeat className="size-4" aria-hidden />
        </BarButton>
      )}
    </div>
  );
}

function MarkerTick({
  marker,
  fraction,
  time,
  active,
  onClick,
}: {
  marker: PlaybackMarker;
  fraction: number;
  time: string;
  active: boolean;
  onClick: () => void;
}) {
  const name = marker.label ? `${marker.label} (${time})` : `Marker at ${time}`;
  return (
    <button
      type="button"
      // The previous/next-marker buttons are the keyboard's way to the markers; one tab stop
      // per tick would bury the rest of the bar behind a long capture list.
      tabIndex={-1}
      title={name}
      aria-label={name}
      data-active={active ? "" : undefined}
      onClick={onClick}
      style={{ left: `calc(7px + ${fraction} * (100% - 14px))` }}
      className="group absolute top-0 flex h-full w-2.5 -translate-x-1/2 cursor-pointer justify-center"
    >
      <span
        aria-hidden
        className="h-full w-0.5 rounded-full opacity-70 transition-opacity group-hover:opacity-100 group-data-active:w-1 group-data-active:opacity-100"
        style={{ backgroundColor: toneColor(marker.tone) }}
      />
    </button>
  );
}

/** Props of the bar's icon button. */
interface BarButtonProps
  extends Omit<ComponentPropsWithRef<"button">, "type" | "title" | "aria-label" | "aria-pressed" | "children"> {
  label: string;
  pressed?: boolean | undefined;
  children: ReactNode;
}

/** An icon button in the style of `@vitavision/stage2d`'s `StageButton`, sized by density. */
function BarButton({ label, pressed, className, children, ...props }: BarButtonProps) {
  const density = useDensity();
  return (
    <button
      {...props}
      type="button"
      title={label}
      aria-label={label}
      aria-pressed={pressed}
      data-state={pressed === undefined ? undefined : pressed ? "on" : "off"}
      className={cn(
        "grid place-items-center rounded-control text-fg-muted transition-colors",
        byDensity(density, "size-8", "size-7"),
        "hover:bg-raised hover:text-fg disabled:pointer-events-none disabled:opacity-40",
        pressed && "bg-signal/15 text-signal",
        focusRing,
        className,
      )}
    >
      {children}
    </button>
  );
}
