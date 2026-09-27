# @vitavision/workbench

The building blocks of a vitavision *studio* app — a tool whose surfaces are all permanent: a
navigator, a viewport, an inspector, a timeline. Built on `@vitavision/ui` and its tokens.

> Incubating in etendue (`web/packages/workbench`); it moves to lab-ui by PR, unchanged.

```bash
bun add @vitavision/workbench @vitavision/ui
```

```css
@import "tailwindcss";
@import "@vitavision/ui/styles.css";
@import "@vitavision/workbench/styles.css";
```

| Export | What |
|---|---|
| `AppShell` | Full-viewport frame: `header`, `left` \| `main` \| `right` (side panels resizable and optionally remembered), `bottom`. Landmarks included. |
| `SplitPane` | Two panes, horizontal or vertical, with a draggable, keyboard-operable divider (`role="separator"`, WAI-ARIA window splitter). Sizes in px or `%`, min/max, collapsible, controlled or not, `storageKey`. Nests. |
| `TreeView` | Data-driven tree (`TreeNode[]` with `icon`/`meta`), WAI-ARIA tree keyboard, controlled selection, controlled or uncontrolled expansion; reveals a selection made elsewhere. |
| `PlaybackBar` | Transport for a sampled timeline: step, play/pause, markers, scrubber, `t = k · dt`, speed, loop. |
| `createPlayhead`, `usePlayhead`, `usePlaybackClock` | The playback position as an external store, and the real-time clock that drives it. |
| `FileDrop` | Drop zone (or full-window `overlay`) plus an "Open files…" button; `accept` applied to both routes; dropped folders walked. |
| `Toaster`, `toast()` | Notifications from anywhere; one `<Toaster />` near the root. |

The pure logic behind them is exported too and tested without a DOM: `splitSize.ts`
(`resolveSize`, `resolveLimits`, `clampSize`), `treeModel.ts` (`visibleRows`, `treeKeyAction`,
`ancestorIds`, `parentIds`), `playhead.ts` (`advance`, `clampIndex`, `formatSeconds`),
`dropFiles.ts` (`acceptsFile`, `collectDroppedFiles`), `toastStore.ts` (`createToastStore`).

### Playback without re-rendering the app

The current frame changes every animation frame, so it is not React state. It is a `Playhead`:

```tsx
const [playhead] = useState(() => createPlayhead(trajectory.count, trajectory.dt));
const [playing, setPlaying] = useState(false);
const [speed, setSpeed] = useState(1);
const [loop, setLoop] = useState(false);
usePlaybackClock({ playhead, playing, speed, loop, onEnd: () => setPlaying(false) });

<PlaybackBar
  playhead={playhead}
  playing={playing} onPlayingChange={setPlaying}
  speed={speed} onSpeedChange={setSpeed}
  loop={loop} onLoopChange={setLoop}
  markers={captures.map((c) => ({ index: c.sample, label: c.name }))}
/>

// In the 3D view's own frame loop — no React involved:
renderer.setAnimationLoop(() => {
  robot.setJoints(trajectory.joints(playhead.get()));
  renderer.render(scene, camera);
});
```

The contract, kept deliberately small:

- `get()` — the current sample index, an integer in `0 … count − 1`. Cheap; call it every frame.
- `set(k)` — rounds and clamps; notifies only on a change.
- `subscribe(listener)` — returns the unsubscribe function (the `useSyncExternalStore` shape).
- `count`, `dt` — the timeline; sample `k` is at `t = k · dt`.
- `setTimeline(count, dt)` — a new scenario in the same store; the index is clamped, not reset.

Every member may be called detached. Only the bar (and anything using `usePlayhead`)
re-renders during playback. The fractional position real-time playback needs lives in the
clock, not the store, so a reader never sees a fractional frame.

### Scope

No routing, no menus, no app state beyond what each component is asked to remember. A page
that scrolls — a document, a settings form — does not want `AppShell`.

## License

Licensed under either of

- Apache License, Version 2.0
- MIT license

at your option.
