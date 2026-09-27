import type { FrameTreeRuntime } from "@vitavision/three";
import { Callout, Empty, Tabs } from "@vitavision/ui";
import {
  AppShell,
  PlaybackBar,
  type PlaybackMarker,
  SplitPane,
  Toaster,
  createPlayhead,
  usePlaybackClock,
} from "@vitavision/workbench";
import { useCallback, useEffect, useMemo, useState } from "react";

import type { Baked } from "./kernel/etendue";
import { CameraView } from "./panels/CameraView";
import { Header } from "./panels/Header";
import { Inspector } from "./panels/Inspector";
import { JointChart } from "./panels/JointChart";
import { Navigator } from "./panels/Navigator";
import { Viewport } from "./panels/Viewport";
import { sceneTree } from "./scene/model";
import { Studio, initialExample, useStudio } from "./state/studio";

/** A value that belongs to one loaded scene: it resets when another scene loads. */
interface Scoped<T> {
  scene: Baked | null;
  value: T;
}

export function App() {
  const [studio] = useState(() => new Studio(createPlayhead(1, 0.01)));
  const { current, failure, loading, playing, example } = useStudio(studio);
  const { playhead } = studio;
  const [selection, setSelection] = useState<Scoped<string | null>>({ scene: null, value: null });
  const [runtimeOf, setRuntimeOf] = useState<Scoped<FrameTreeRuntime | null>>({ scene: null, value: null });
  const [tab, setTab] = useState<Scoped<string | null>>({ scene: null, value: null });
  const [speed, setSpeed] = useState(1);
  const [loop, setLoop] = useState(false);

  const selected = selection.scene === current ? selection.value : null;
  const setSelected = (id: string | null) => {
    setSelection({ scene: current, value: id });
    // Selecting a camera shows what it sees.
    if (id !== null && current?.loaded.scene.cameras?.some((c) => c.id === id)) setTab({ scene: current, value: id });
  };
  const runtime = runtimeOf.scene === current ? runtimeOf.value : null;
  const onRuntime = useCallback(
    (r: FrameTreeRuntime) =>
      setRuntimeOf((prev) => (prev.scene === current && prev.value === r ? prev : { scene: current, value: r })),
    [current],
  );

  usePlaybackClock({ playhead, playing, speed, loop, onEnd: () => studio.setPlaying(false) });

  useEffect(() => {
    studio.start(initialExample());
  }, [studio]);
  useEffect(() => () => studio.dispose(), [studio]);

  const tree = useMemo(() => (current ? sceneTree(current.loaded.scene, current.baked) : null), [current]);
  const markers = useMemo<PlaybackMarker[]>(
    () =>
      current?.baked.samples.flatMap((s, index) => (s.capture ? [{ index, label: `capture ${s.capture.id}` }] : [])) ??
      [],
    [current],
  );

  const scene = current?.loaded.scene;
  const cameras = scene?.cameras ?? [];
  const robots = current?.baked.robots ?? [];
  const selectedRobot =
    robots.find((r) => r.id === selected || (selected !== null && selected.startsWith(`${r.id}/`)))?.id ?? robots[0]?.id;
  const tabs = [
    ...cameras.map((c) => ({ id: c.id, label: c.id })),
    ...(robots.length > 0 ? [{ id: "joints", label: "Joints" }] : []),
  ];
  const bottomTab = tab.scene === current ? tab.value : null;
  const activeTab = tabs.find((t) => t.id === bottomTab)?.id ?? tabs[0]?.id ?? "joints";
  const activeCamera = cameras.find((c) => c.id === activeTab);

  const main = current ? (
    <SplitPane orientation="vertical" sizedPane="end" defaultSize="38%" minSize={160} maxSize="70%" storageKey="etendue-studio:bottom" aria-label="Resize the camera panel">
      {[
        <div key="viewport" className="relative h-full min-h-0">
          <Viewport
            current={current}
            playhead={playhead}
            selected={selected}
            onSelect={setSelected}
            onRuntime={onRuntime}
          />
          {loading && <div className="absolute top-2 left-2 text-xs text-fg-muted">Loading…</div>}
        </div>,
        <div key="bottom" className="flex h-full min-h-0 flex-col bg-surface">
          <Tabs items={tabs} active={activeTab} onSelect={(id) => setTab({ scene: current, value: id })} label="Camera and joint views" className="px-2" />
          <div className="min-h-0 flex-1 p-2" role="tabpanel">
            {activeCamera && runtime ? (
              <CameraView
                key={activeCamera.id}
                session={current.session}
                runtime={runtime}
                camera={activeCamera}
                targets={scene?.targets ?? []}
                playhead={playhead}
              />
            ) : activeTab === "joints" && selectedRobot ? (
              <JointChart baked={current.baked} robot={selectedRobot} playhead={playhead} />
            ) : null}
          </div>
        </div>,
      ]}
    </SplitPane>
  ) : (
    <div className="grid h-full place-items-center p-6">
      {failure ? (
        <Callout tone="error" title={failure.title}>
          {failure.message}
        </Callout>
      ) : (
        <Empty>{loading ? "Loading…" : "Open an example or drop a scene.json."}</Empty>
      )}
    </div>
  );

  return (
    <>
      <AppShell
        storageKey="etendue-studio"
        header={
          <Header
            label={current?.loaded.label ?? null}
            description={current?.loaded.scene.description ?? null}
            example={example}
            onExample={studio.openExample}
            onFiles={studio.openFiles}
          />
        }
        left={tree && <Navigator tree={tree} selected={selected} onSelect={setSelected} />}
        leftLabel="Scene frames"
        main={main}
        right={
          current && (
            <Inspector
              current={current}
              runtime={runtime}
              selected={selected}
              failure={failure}
              playhead={playhead}
            />
          )
        }
        bottom={
          current && (
            <PlaybackBar
              playhead={playhead}
              playing={playing}
              onPlayingChange={studio.setPlaying}
              speed={speed}
              onSpeedChange={setSpeed}
              loop={loop}
              onLoopChange={setLoop}
              markers={markers}
            />
          )
        }
      />
      <Toaster />
    </>
  );
}
