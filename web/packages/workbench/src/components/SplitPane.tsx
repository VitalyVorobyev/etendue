/*
 * Two panes and the divider between them.
 *
 * A studio screen is a handful of permanent surfaces — a tree, a viewport, an inspector, a
 * timeline — whose relative size is the user's decision, not the layout's. The divider is
 * therefore a real control: draggable, operable from the keyboard (the WAI-ARIA
 * window-splitter pattern), announced with its position, and remembered across reloads
 * when asked to be.
 *
 * One pane carries the size and the other takes the rest, so nesting composes: the rest of
 * one split is simply another split. `AppShell` builds its side panels exactly that way.
 */

import { useCallback, useId, useRef, useState } from "react";
import type {
  CSSProperties,
  KeyboardEvent,
  PointerEvent as ReactPointerEvent,
  ReactNode,
} from "react";

import { cn, focusRing } from "@vitavision/ui";

import {
  type PaneSize,
  type SizedPane,
  type SplitOrientation,
  clampSize,
  readStoredSize,
  resolveLimits,
  resolveSize,
  splitKeyAction,
  writeStoredSize,
} from "./splitSize";

/** Props of `SplitPane`. */
export interface SplitPaneProps {
  /**
   * The two panes, in reading order (left then right, or top then bottom). Exactly two;
   * nest another `SplitPane` in either for more.
   */
  children: readonly [ReactNode, ReactNode];
  /**
   * `horizontal` (the default) lays the panes side by side with a vertical divider;
   * `vertical` stacks them with a horizontal one.
   */
  orientation?: SplitOrientation | undefined;
  /** Which pane carries the size; the other takes the rest. Defaults to `start`. */
  sizedPane?: SizedPane | undefined;
  /** The sized pane's size in pixels, controlled. Pair with `onSizeChange`. */
  size?: number | undefined;
  /** The initial size when uncontrolled: pixels or a percentage. Defaults to `"50%"`. */
  defaultSize?: PaneSize | undefined;
  /** Called with the new size in pixels on every drag move and key press (0 when collapsed). */
  onSizeChange?: ((size: number) => void) | undefined;
  /** The sized pane's smallest open size: pixels or a percentage. Defaults to 0. */
  minSize?: PaneSize | undefined;
  /** The sized pane's largest size: pixels or a percentage. Defaults to `"100%"`. */
  maxSize?: PaneSize | undefined;
  /**
   * Lets the sized pane close: dragging it below half its minimum snaps it shut, and Enter
   * on the divider collapses it or restores the last open size.
   */
  collapsible?: boolean | undefined;
  /** How far one arrow-key press moves the divider, in pixels (four times with Shift). Defaults to 16. */
  step?: number | undefined;
  /**
   * Remember the size under this `localStorage` key, and start from it. Uncontrolled only:
   * a controlled split's size is the caller's to keep.
   */
  storageKey?: string | undefined;
  /** Names the divider for assistive technology. Defaults to "Resize panes". */
  "aria-label"?: string | undefined;
  /** Merged with the split's own classes through `cn`. */
  className?: string | undefined;
}

/**
 * Two panes with a draggable, keyboard-operable divider between them.
 *
 * The divider is a `role="separator"` focusable control carrying `aria-valuenow` (the sized
 * pane's size in pixels), `aria-valuemin`/`aria-valuemax` and `aria-controls` (the sized
 * pane). Arrow keys along the axis move it by `step` (Shift: four steps), Home/End give the
 * sized pane its minimum/maximum, Enter collapses or restores a `collapsible` pane. Dragging
 * uses pointer capture, so the drag survives the pointer leaving the divider.
 *
 * Fills its container (`h-full w-full`), so it nests. State is exposed as `data-orientation`
 * on the root, `data-dragging` on the root and the divider while a drag is in progress, and
 * `data-collapsed` on the root and the sized pane while it is closed.
 */
export function SplitPane({
  children,
  orientation = "horizontal",
  sizedPane = "start",
  size: controlledSize,
  defaultSize = "50%",
  onSizeChange,
  minSize = 0,
  maxSize = "100%",
  collapsible = false,
  step = 16,
  storageKey,
  "aria-label": ariaLabel = "Resize panes",
  className,
}: SplitPaneProps) {
  const paneId = useId();
  const horizontal = orientation === "horizontal";
  const [length, setLength] = useState<number | null>(null);
  const [internalSize, setInternalSize] = useState<number | null>(null);
  const [dragging, setDragging] = useState(false);
  const rootRef = useRef<HTMLDivElement | null>(null);
  // The drag in progress: where it started, and the size it has reached. The reached size is
  // kept here, not read back from state, because a pointer move's update is rendered at
  // continuous priority and may not have landed when the pointer is released.
  const dragRef = useRef<{ origin: number; start: number; length: number; reached: number } | null>(null);
  const lastOpenRef = useRef<number | null>(null);

  const limits = length === null ? null : resolveLimits(length, minSize, maxSize, collapsible);
  const size =
    controlledSize ??
    internalSize ??
    (length === null || limits === null ? null : clampSize(resolveSize(defaultSize, length), limits));
  const collapsed = size === 0;
  const controlled = controlledSize !== undefined;

  /*
   * Measure the split along its axis — synchronously on attach, so the first paint has the
   * limits, then on every resize — and pick up a remembered size. A callback ref rather than
   * an effect: the subscription lives exactly as long as the element.
   */
  const attach = useCallback(
    (element: HTMLDivElement | null) => {
      rootRef.current = element;
      if (!element) return;
      const measure = () => {
        const rect = element.getBoundingClientRect();
        setLength(horizontal ? rect.width : rect.height);
      };
      measure();
      if (storageKey !== undefined && !controlled) {
        const stored = readStoredSize(storageKey);
        if (stored !== null) setInternalSize(stored);
      }
      if (typeof ResizeObserver === "undefined") return;
      const observer = new ResizeObserver(measure);
      observer.observe(element);
      return () => observer.disconnect();
    },
    [horizontal, storageKey, controlled],
  );

  /** The live length and limits, measured now (a drag or a key press must not use stale ones). */
  const measureNow = () => {
    const element = rootRef.current;
    const rect = element?.getBoundingClientRect();
    const now = rect ? (horizontal ? rect.width : rect.height) : (length ?? 0);
    return { length: now, limits: resolveLimits(now, minSize, maxSize, collapsible) };
  };

  /** Take a new size: state, the caller, and (on `commit`) storage. `changed` defaults to a comparison with the rendered size. */
  const apply = (next: number, commit: boolean, changed = next !== size) => {
    if (next > 0) lastOpenRef.current = next;
    if (!controlled) setInternalSize(next);
    if (changed) onSizeChange?.(next);
    if (commit && storageKey !== undefined && !controlled) {
      writeStoredSize(storageKey, next);
    }
  };

  const coordinate = (event: ReactPointerEvent) => (horizontal ? event.clientX : event.clientY);

  const onPointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    const { length: now } = measureNow();
    const current = size ?? 0;
    dragRef.current = { origin: coordinate(event), start: current, length: now, reached: current };
    event.currentTarget.setPointerCapture(event.pointerId);
    event.currentTarget.focus();
    setDragging(true);
  };

  const onPointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (drag === null) return;
    const delta = (coordinate(event) - drag.origin) * (sizedPane === "start" ? 1 : -1);
    const next = clampSize(drag.start + delta, resolveLimits(drag.length, minSize, maxSize, collapsible));
    if (next === drag.reached) return;
    drag.reached = next;
    // Compared with the drag's own last size above: the rendered one may lag behind.
    apply(next, false, true);
  };

  const endDrag = (event: ReactPointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (drag === null) return;
    dragRef.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    setDragging(false);
    if (drag.reached > 0) lastOpenRef.current = drag.reached;
    if (storageKey !== undefined && !controlled) writeStoredSize(storageKey, drag.reached);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const { limits: now } = measureNow();
    const current = size ?? 0;
    const action = splitKeyAction(event.key, event.shiftKey, current, {
      orientation,
      sizedPane,
      step,
      limits: now,
    });
    if (action === null) return;
    event.preventDefault();
    if (action.type === "resize") {
      apply(action.size, true);
    } else if (current > 0) {
      lastOpenRef.current = current;
      apply(0, true);
    } else {
      apply(clampSize(lastOpenRef.current ?? now.min, { ...now, collapsible: false }), true);
    }
  };

  // Before the first measurement (and on the server) the pane is sized by CSS alone, from
  // the default; afterwards by the measured size. The limits stay in CSS too, so a window
  // resize never pushes the pane outside them between two measurements.
  const basis =
    size !== null ? `${size}px` : typeof defaultSize === "number" ? `${defaultSize}px` : defaultSize;
  const cssSize = (value: PaneSize) => (typeof value === "number" ? `${value}px` : value);
  const sizedStyle: CSSProperties = {
    flex: `0 0 ${basis}`,
    ...(collapsed
      ? {}
      : horizontal
        ? { minWidth: cssSize(minSize), maxWidth: cssSize(maxSize) }
        : { minHeight: cssSize(minSize), maxHeight: cssSize(maxSize) }),
  };

  const [first, second] = children;
  const paneClasses = "relative min-h-0 min-w-0 overflow-hidden";
  const sized = (
    <div
      id={paneId}
      style={sizedStyle}
      className={paneClasses}
      data-collapsed={collapsed ? "" : undefined}
      // A closed pane keeps its content mounted (a tree keeps its expansion), but nothing in
      // it may take focus or be read out.
      inert={collapsed}
    >
      {sizedPane === "start" ? first : second}
    </div>
  );
  const rest = (
    <div className={cn(paneClasses, "flex-1 basis-0")}>{sizedPane === "start" ? second : first}</div>
  );

  return (
    <div
      ref={attach}
      data-orientation={orientation}
      data-dragging={dragging ? "" : undefined}
      data-collapsed={collapsed ? "" : undefined}
      className={cn(
        "flex h-full min-h-0 w-full min-w-0",
        horizontal ? "flex-row" : "flex-col",
        dragging && (horizontal ? "cursor-col-resize select-none" : "cursor-row-resize select-none"),
        className,
      )}
    >
      {sizedPane === "start" ? sized : rest}
      <div
        role="separator"
        tabIndex={0}
        aria-label={ariaLabel}
        // The separator's own orientation is across the split: side-by-side panes are divided
        // by a vertical line.
        aria-orientation={horizontal ? "vertical" : "horizontal"}
        aria-controls={paneId}
        aria-valuenow={size === null ? undefined : Math.round(size)}
        aria-valuemin={limits === null ? undefined : collapsible ? 0 : Math.round(limits.min)}
        aria-valuemax={limits === null ? undefined : Math.round(limits.max)}
        data-dragging={dragging ? "" : undefined}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onKeyDown={onKeyDown}
        className={cn(
          // A 1px rule with a wider invisible hit area on either side of it.
          "relative z-10 shrink-0 touch-none bg-line transition-colors",
          "before:absolute before:content-['']",
          horizontal
            ? "w-px cursor-col-resize before:inset-y-0 before:-right-1 before:-left-1"
            : "h-px cursor-row-resize before:inset-x-0 before:-top-1 before:-bottom-1",
          "hover:bg-signal data-dragging:bg-signal",
          focusRing,
        )}
      />
      {sizedPane === "start" ? rest : sized}
    </div>
  );
}
