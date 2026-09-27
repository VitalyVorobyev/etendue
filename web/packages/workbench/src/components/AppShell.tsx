/*
 * The frame of a studio app: a header across the top, a main surface, optional side panels
 * either side of it, and an optional strip along the bottom.
 *
 * `@vitavision/ui` deliberately has no opinion on how an app fills the viewport — a page
 * scrolls. A studio is the other kind of screen: every surface is permanent, nothing scrolls
 * but the insides of panels, and the side panels are resized by hand. This is that frame,
 * built from `SplitPane`, and nothing more: no routing, no menus, no state of its own beyond
 * the panel sizes it is asked to remember.
 */

import type { ReactNode } from "react";

import { cn } from "@vitavision/ui";

import { SplitPane } from "./SplitPane";
import type { PaneSize } from "./splitSize";

/** How one side panel of an `AppShell` is sized. */
export interface SidePanelSize {
  /** The initial width: pixels or a percentage. */
  defaultSize?: PaneSize | undefined;
  /** The narrowest the panel may be dragged. */
  minSize?: PaneSize | undefined;
  /** The widest the panel may be dragged. */
  maxSize?: PaneSize | undefined;
  /** Lets the panel close (drag it under half its minimum, or Enter on its divider). */
  collapsible?: boolean | undefined;
}

/** Props of `AppShell`. */
export interface AppShellProps {
  /** The top bar: the app's name, the file actions, the theme toggle. */
  header?: ReactNode;
  /** The panel left of the main surface — a navigator such as a `TreeView`. */
  left?: ReactNode;
  /** The main surface — the viewport. Always present. */
  main: ReactNode;
  /** The panel right of the main surface — an inspector. */
  right?: ReactNode;
  /** The strip along the bottom, across the whole width — a `PlaybackBar`, a status line. */
  bottom?: ReactNode;
  /** Names the left panel's landmark (`<aside>`). Defaults to "Navigator". */
  leftLabel?: string | undefined;
  /** Names the right panel's landmark (`<aside>`). Defaults to "Inspector". */
  rightLabel?: string | undefined;
  /** The left panel's sizing. Defaults to 280 px, 180 px to 50%. */
  leftSize?: SidePanelSize | undefined;
  /** The right panel's sizing. Defaults to 320 px, 220 px to 50%. */
  rightSize?: SidePanelSize | undefined;
  /**
   * Remember the side panels' widths in `localStorage`, under `<storageKey>:left` and
   * `<storageKey>:right`.
   */
  storageKey?: string | undefined;
  /** Merged with the shell's own classes through `cn` — override `h-dvh` to embed it. */
  className?: string | undefined;
}

const LEFT_DEFAULTS = { defaultSize: 280, minSize: 180, maxSize: "50%" } as const;
const RIGHT_DEFAULTS = { defaultSize: 320, minSize: 220, maxSize: "50%" } as const;

/**
 * The full-viewport frame of a studio app: `header` across the top, `left` | `main` |
 * `right` in the middle with resizable side panels, `bottom` across the foot. Every slot but
 * `main` is optional, and an absent slot takes no space.
 *
 * Landmarks: the header is a `<header>`, the main surface a `<main>`, the side panels
 * `<aside>`s named by `leftLabel` and `rightLabel`. Each side panel is divided from the main
 * surface by a `SplitPane` divider (drag, or focus it and use the arrow keys). Only the
 * insides of the panels scroll; the shell itself is fixed to the viewport (`h-dvh`).
 */
export function AppShell({
  header,
  left,
  main,
  right,
  bottom,
  leftLabel = "Navigator",
  rightLabel = "Inspector",
  leftSize,
  rightSize,
  storageKey,
  className,
}: AppShellProps) {
  const leftSizing = { ...LEFT_DEFAULTS, ...leftSize };
  const rightSizing = { ...RIGHT_DEFAULTS, ...rightSize };
  const hasLeft = left !== undefined && left !== null && left !== false;
  const hasRight = right !== undefined && right !== null && right !== false;

  const mainSurface = <main className="relative h-full min-h-0 min-w-0 overflow-hidden">{main}</main>;

  const withRight = hasRight ? (
    <SplitPane
      sizedPane="end"
      {...rightSizing}
      storageKey={storageKey === undefined ? undefined : `${storageKey}:right`}
      aria-label={`Resize ${rightLabel.toLowerCase()}`}
    >
      {mainSurface}
      <aside aria-label={rightLabel} className="h-full overflow-auto bg-surface">
        {right}
      </aside>
    </SplitPane>
  ) : (
    mainSurface
  );

  const middle = hasLeft ? (
    <SplitPane
      sizedPane="start"
      {...leftSizing}
      storageKey={storageKey === undefined ? undefined : `${storageKey}:left`}
      aria-label={`Resize ${leftLabel.toLowerCase()}`}
    >
      <aside aria-label={leftLabel} className="h-full overflow-auto bg-surface">
        {left}
      </aside>
      {withRight}
    </SplitPane>
  ) : (
    withRight
  );

  return (
    <div
      className={cn(
        "grid h-dvh w-full grid-rows-[auto_minmax(0,1fr)_auto] overflow-hidden bg-ground text-fg",
        className,
      )}
    >
      {header ? (
        <header className="min-w-0 border-b border-line bg-surface">{header}</header>
      ) : (
        <div aria-hidden />
      )}
      <div className="min-h-0 min-w-0">{middle}</div>
      {bottom ? <div className="min-w-0 border-t border-line bg-surface">{bottom}</div> : <div aria-hidden />}
    </div>
  );
}
