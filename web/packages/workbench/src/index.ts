/**
 * `@vitavision/workbench` — the building blocks of a studio app on `@vitavision/ui`: the
 * shell and its split panes, a tree navigator, a playback transport over a sampled timeline,
 * file opening, and notifications.
 *
 * Import `@vitavision/workbench/styles.css` after `@vitavision/ui/styles.css`.
 *
 * @packageDocumentation
 */

export { AppShell, type AppShellProps, type SidePanelSize } from "./components/AppShell";
export { SplitPane, type SplitPaneProps } from "./components/SplitPane";
export {
  type PaneSize,
  type SizedPane,
  type SplitOrientation,
  clampSize,
  resolveLimits,
  resolveSize,
  type SplitLimits,
} from "./components/splitSize";

export { TreeView, type TreeViewProps } from "./components/TreeView";
export {
  type TreeKeyAction,
  type TreeNode,
  type TreeRow,
  ancestorIds,
  parentIds,
  treeKeyAction,
  visibleRows,
} from "./components/treeModel";

export {
  DEFAULT_SPEEDS,
  PlaybackBar,
  type PlaybackBarProps,
  type PlaybackMarker,
} from "./components/PlaybackBar";
export {
  type AdvanceResult,
  advance,
  clampIndex,
  createPlayhead,
  formatSeconds,
  type Playhead,
} from "./components/playhead";
export {
  type PlaybackClockOptions,
  usePlaybackClock,
  usePlayhead,
  usePlayheadTimeline,
} from "./components/usePlayback";

export { FileDrop, type FileDropProps } from "./components/FileDrop";
export { acceptsFile, collectDroppedFiles, type DropEntry } from "./components/dropFiles";

export { Toaster, type ToasterProps } from "./components/Toaster";
export {
  createToastStore,
  defaultToastStore,
  toast,
  type ToastOptions,
  type ToastRecord,
  type ToastStore,
  type ToastTone,
} from "./components/toastStore";
