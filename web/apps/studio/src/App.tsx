import { type FrameTreeRuntime, type Iso3, invertIso3 } from "@vitavision/three";
import { Button, Callout, Empty, Panel, SegmentedControl, Select } from "@vitavision/ui";
import {
  AppShell,
  PlaybackBar,
  type PlaybackMarker,
  Toaster,
  createPlayhead,
  toast,
  usePlaybackClock,
} from "@vitavision/workbench";
import { Box, ChevronDown, ChevronUp, Minimize2, Pin } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import library, { type LibraryRobot } from "virtual:etendue-robots";

import { exportFile, jsonText } from "./io/export";
import { type Cell, defaultOf } from "./io/project";
import { fileUrl, isTauri, parentDir } from "./io/tauri";
import { describeFailure } from "./kernel/etendue";
import { type Bodies, type PoseOverride, bodyPoses, posed } from "./kernel/posed";
import type { GeneratedPoses } from "@etendue/wasm";
import { scenarioFromPoses } from "./kernel/native";
import { CameraGrid } from "./panels/CameraGrid";
import { DatasetPanel } from "./panels/DatasetPanel";
import { GeneratorPanel } from "./panels/GeneratorPanel";
import { type ExportItem, Header, type Mode } from "./panels/Header";
import { Inspector } from "./panels/Inspector";
import { JointChart, type JointQuantity } from "./panels/JointChart";
import { Navigator } from "./panels/Navigator";
import { StepList } from "./panels/StepList";
import { type GizmoTarget, Viewport } from "./panels/Viewport";
import {
  type EntityList,
  IDENTITY,
  addEntity,
  entityOf,
  newEntity,
  removeEntity,
  removeRobot,
  uniqueId,
  updateEntity,
} from "./scene/edit";
import { sceneTree } from "./scene/model";
import { DatasetRunner } from "./state/datasetRun";
import { Studio, initialExample, useStudio } from "./state/studio";

/** A value that belongs to one opened document: it resets when another one opens. */
interface Scoped<T> {
  opened: number;
  value: T;
}

const MODE_KEY = "etendue-studio:mode";
const NO_OVERRIDES: readonly PoseOverride[] = [];
const NO_CELLS: readonly Cell[] = [];

function storedMode(modes: Mode[]): Mode {
  try {
    const m = localStorage.getItem(MODE_KEY) as Mode | null;
    if (m !== null && modes.includes(m)) return m;
  } catch {
    // Storage may be unavailable (private window); the default applies.
  }
  return "setup";
}

/** Whether a key event belongs to a text field (which has its own undo). */
function inField(e: KeyboardEvent): boolean {
  const t = e.target as HTMLElement | null;
  return t !== null && (t.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(t.tagName));
}

export function App() {
  const [studio] = useState(() => new Studio(createPlayhead(1, 0.01)));
  const [runner] = useState(() => new DatasetRunner(studio));
  const {
    current,
    failure,
    loading,
    playing,
    example,
    dataset,
    undo,
    redo,
    dirty,
    opened,
    preview: editing,
    project,
  } = useStudio(studio);
  const { playhead } = studio;
  const modes = useMemo<Mode[]>(() => ["setup", "motion", "images", ...(isTauri() ? ["dataset" as const] : [])], []);
  const [mode, setModeState] = useState<Mode>(() => storedMode(modes));
  const [selection, setSelection] = useState<Scoped<string | null>>({ opened: 0, value: null });
  const [runtime, setRuntime] = useState<FrameTreeRuntime | null>(null);
  const [focusCamera, setFocusCamera] = useState<string | null>(null);
  const [chartOpen, setChartOpen] = useState(true);
  const [quantity, setQuantity] = useState<JointQuantity>("position");
  const [insetOpen, setInsetOpen] = useState(true);
  const [motionPanel, setMotionPanel] = useState<"program" | "generate">("program");
  const [preview, setPreview] = useState<GeneratedPoses | null>(null);
  const [chartRobot, setChartRobot] = useState<string | null>(null);
  const [speed, setSpeed] = useState(1);
  const [loop, setLoop] = useState(false);

  const setMode = (m: Mode) => {
    setModeState(m);
    try {
      localStorage.setItem(MODE_KEY, m);
    } catch {
      // Remembering the workspace is a convenience.
    }
  };

  // The selection survives edits (unless they remove it) but not another document.
  const selected =
    selection.opened === opened && selection.value !== null && current?.baked.frames.includes(selection.value)
      ? selection.value
      : selection.opened === opened &&
          selection.value !== null &&
          current?.loaded.scene.robots?.some((r) => r.id === selection.value)
        ? selection.value
        : null;
  const setSelected = (id: string | null) => setSelection({ opened, value: id });
  // The frame tree's runtime: a new one for every bake.
  const onRuntime = useCallback((r: FrameTreeRuntime) => setRuntime(r), []);
  // World poses with the edit in progress drawn over the bake.
  const overrides = editing?.overrides ?? NO_OVERRIDES;
  // Cameras with physical optics are baked at their entrance pupil; the document places them by
  // their bodies (ADR 0008).
  const bodies = useMemo<Bodies>(() => {
    const out = new Map<string, Iso3>();
    for (const c of current?.loaded.scene.cameras ?? []) {
      const m = c.optics ? current?.session.mechSe3Projection(c.id) : undefined;
      if (m) out.set(c.id, invertIso3(m));
    }
    return out;
  }, [current]);
  const poses = useMemo(() => (runtime ? posed(runtime, overrides, bodies) : null), [runtime, overrides, bodies]);
  // What an editor shows and moves: bodies, not projection frames.
  const bodyPose = useMemo(() => (poses ? bodyPoses(poses, bodies) : null), [poses, bodies]);
  const live = useMemo(() => ({ preview: studio.preview, commit: studio.commit, cancel: studio.cancel }), [studio]);

  usePlaybackClock({ playhead, playing, speed, loop, onEnd: () => studio.setPlaying(false) });

  useEffect(() => {
    studio.start(initialExample());
  }, [studio]);
  useEffect(() => () => studio.dispose(), [studio]);

  // Undo, redo and save from the keyboard, except inside a text field.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || e.altKey) return;
      const key = e.key.toLowerCase();
      if (key === "s") {
        e.preventDefault();
        void studio.save(e.shiftKey);
      } else if ((key === "z" || key === "y") && !inField(e)) {
        e.preventDefault();
        if (key === "y" || e.shiftKey) studio.redo();
        else studio.undo();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [studio]);

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
  const selectedRobot = robots.find((r) => r.id === selected || (selected !== null && selected.startsWith(`${r.id}/`)));
  const shownRobot = robots.find((r) => r.id === chartRobot)?.id ?? selectedRobot?.id ?? robots[0]?.id ?? null;
  // The generated image of a camera at sample `k`, if `k` is a capture it rendered.
  const renderedAt = useCallback(
    (camera: string, k: number): string | null => {
      const capture = current?.baked.samples[k]?.capture?.id;
      if (!dataset?.render?.cameras.includes(camera) || capture === undefined) return null;
      // Per run: a regenerated image has the same path, and the webview would show the cached one.
      return `${fileUrl(`${dataset.output}/images/${camera}/${capture}.png`)}?run=${dataset.run}`;
    },
    [current, dataset],
  );
  const importPoses = (path: string) => {
    const ids = scene?.robots?.map((r) => r.id) ?? [];
    const robot = ids.length === 1 ? ids[0] : selectedRobot?.id;
    if (!robot) {
      toast({
        title: ids.length === 0 ? "The scene has no robot to move" : "Select the robot that moves",
        tone: "error",
      });
      return;
    }
    void scenarioFromPoses(path, robot)
      .then((scenario) => studio.setScenario(scenario, `import poses ${path.split(/[\\/]/).at(-1) ?? path}`))
      .catch((e: unknown) =>
        toast({
          title: "Could not import the poses",
          description: e instanceof Error ? e.message : String(e),
          tone: "error",
        }),
      );
  };

  const exportDir = current?.loaded.origin ? parentDir(current.loaded.origin) : null;
  const scenario = current?.loaded.scenario ?? null;
  // An export computed by the kernel; a failure (a scenario that does not compile) is shown.
  const exporting = (title: string, name: string, type: string, text: () => string) => () => {
    let body: string;
    try {
      body = text();
    } catch (e) {
      const failure = describeFailure(e);
      toast({ title: `Could not export: ${failure.title}`, description: failure.message, tone: "error" });
      return;
    }
    void exportFile(title, { name, text: body, type }, exportDir);
  };
  const exports: ExportItem[] =
    current && scenario
      ? [
          ...(shownRobot
            ? [
                {
                  label: `Captures of ${shownRobot} (CSV)…`,
                  run: exporting("Export the capture poses", "captures.csv", "text/csv", () =>
                    current.session.exportCaptures(scenario, shownRobot, "csv"),
                  ),
                },
                {
                  label: `Captures of ${shownRobot} (JSON)…`,
                  run: exporting("Export the capture poses", "captures.json", "application/json", () =>
                    current.session.exportCaptures(scenario, shownRobot, "json"),
                  ),
                },
                {
                  label: `Trajectory of ${shownRobot} (CSV)…`,
                  run: exporting("Export the joint trajectory", "trajectory.csv", "text/csv", () =>
                    current.session.exportTrajectory(scenario, shownRobot),
                  ),
                },
                {
                  label: "Motion report (JSON)…",
                  run: exporting("Export the motion report", "motion_report.json", "application/json", () =>
                    jsonText(current.session.motionReport(scenario)),
                  ),
                },
              ]
            : []),
          {
            label: "Scenario (JSON)…",
            run: exporting("Export the scenario", "scenario.json", "application/json", () => jsonText(scenario)),
          },
          {
            label: "Baked motion (JSON)…",
            run: exporting("Export the baked motion", "baked.json", "application/json", () => jsonText(current.baked)),
          },
        ]
      : [];
  // The motion report, for the Motion workspace (computed when it shows).
  const report = useMemo(() => {
    if (mode !== "motion" || !current?.loaded.scenario) return null;
    try {
      return current.session.motionReport(current.loaded.scenario);
    } catch {
      return null;
    }
  }, [mode, current]);
  const unresolved = scenario?.steps.some((s) => s.type === "ptp_pose" && s.q === undefined) ?? false;
  const resolveIk = () => {
    if (!current || !scenario) return;
    try {
      studio.setScenario(current.session.resolveScenario(scenario), "resolve IK");
    } catch (e) {
      const failure = describeFailure(e);
      toast({ title: failure.title, description: failure.message, tone: "error" });
    }
  };

  const add = (list: EntityList) => {
    if (!current) return;
    // Attach to the selected frame (a link, an entity), else to the world.
    const parent = selected !== null && current.baked.frames.includes(selected) ? selected : "world";
    const entity = newEntity(current.loaded.scene, list, parent, {
      camera: defaultOf(project, "camera"),
      laser: defaultOf(project, "laser"),
      target: defaultOf(project, "target"),
    });
    if (studio.edit(`add ${entity.id}`, (d) => ({ ...d, scene: addEntity(d.scene, list, entity) }))) {
      setSelected(entity.id);
    }
  };
  const addRobot = (lib: LibraryRobot) => {
    if (!current) return;
    const parent = selected !== null && current.baked.frames.includes(selected) ? selected : "world";
    const id = uniqueId(current.loaded.scene, lib.id);
    void studio
      .addRobot(lib.manifest, id, (d, robot) => ({
        ...d,
        scene: {
          ...d.scene,
          robots: [
            ...(d.scene.robots ?? []),
            {
              id,
              parent,
              parent_se3_self: IDENTITY,
              // Repository-relative: the loader finds it in the robot library, and Save rebases it.
              manifest: lib.manifest,
              // Every joint at 0, or at the nearest limit.
              initial_q: robot.manifest.joints.map((j) => Math.min(Math.max(0, j.lower), j.upper)),
            },
          ],
        },
      }))
      .then((ok) => ok && setSelected(id));
  };
  const isRobot = (id: string | null) => id !== null && (current?.loaded.scene.robots ?? []).some((r) => r.id === id);
  const removable =
    current !== null && selected !== null && (entityOf(current.loaded.scene, selected) !== null || isRobot(selected));
  const remove = () => {
    if (!current || selected === null) return;
    const ok = isRobot(selected)
      ? studio.edit(`remove ${selected}`, (d) => ({
          ...d,
          ...removeRobot(d.scene, d.scenario, selected),
          robots: d.robots.filter((r) => r.id !== selected),
        }))
      : studio.edit(`remove ${selected}`, (d) => ({ ...d, scene: removeEntity(d.scene, selected) }));
    if (ok) setSelected(null);
  };

  // The selection's handle in the Setup workspace: an entity on its frame, a robot on its base.
  const shownScene = editing?.doc.scene ?? current?.loaded.scene;
  const handled =
    mode === "setup" && shownScene && selected !== null
      ? (entityOf(shownScene, selected) ?? shownScene.robots?.find((r) => r.id === selected) ?? null)
      : null;
  const handleFrame =
    handled && isRobot(selected)
      ? `${selected}/${current?.loaded.robots.find((r) => r.id === selected)?.manifest.base_link}`
      : selected;
  const gizmo: GizmoTarget | null =
    handled && handleFrame !== null && selected !== null
      ? {
          frame: handleFrame,
          parent: handled.parent,
          placement: handled.parent_se3_self,
          onPreview: (pose) =>
            studio.preview(`move ${selected}`, (d) => ({
              ...d,
              scene: isRobot(selected)
                ? {
                    ...d.scene,
                    robots: (d.scene.robots ?? []).map((r) =>
                      r.id === selected ? { ...r, parent_se3_self: pose } : r,
                    ),
                  }
                : updateEntity(d.scene, selected, (e) => ({ ...e, parent_se3_self: pose })),
            })),
          onCommit: studio.commit,
          onCancel: studio.cancel,
        }
      : null;

  const viewport = current ? (
    <Viewport
      current={current}
      playhead={playhead}
      selected={selected}
      onSelect={setSelected}
      onRuntime={onRuntime}
      candidates={mode === "motion" && motionPanel === "generate" ? preview?.candidates : null}
      overrides={overrides}
      bodies={bodies}
      poses={bodyPose}
      gizmo={gizmo}
    />
  ) : null;

  const main = current ? (
    <div className="relative flex h-full min-h-0 flex-col">
      {mode === "images" && (
        <div className="min-h-0 flex-1">
          <CameraGrid
            session={current.session}
            runtime={runtime}
            poses={poses}
            cameras={cameras}
            targets={scene?.targets ?? []}
            lasers={scene?.lasers ?? []}
            playhead={playhead}
            renderedAt={renderedAt}
            focus={focusCamera}
            onFocus={setFocusCamera}
          />
        </div>
      )}
      {mode === "dataset" && (
        <div className="min-h-0 flex-1 overflow-auto p-4">
          <DatasetPanel current={current} dataset={dataset} runner={runner} />
        </div>
      )}
      {/* The 3D view stays mounted in every workspace: the camera images render its scene. */}
      <div
        className={
          mode === "images"
            ? insetOpen
              ? "absolute right-3 bottom-3 z-10 h-52 w-72 overflow-hidden rounded-md border border-line-strong bg-canvas shadow-lg"
              : "hidden"
            : mode === "dataset"
              ? "hidden"
              : "relative min-h-0 flex-1"
        }
        data-testid="viewport"
      >
        {viewport}
        {loading && <div className="absolute top-2 left-2 text-xs text-fg-muted">Loading…</div>}
        {mode === "images" && (
          <Button
            variant="ghost"
            size="sm"
            className="absolute top-1 right-1"
            icon={<Minimize2 className="size-3.5" />}
            aria-label="Hide the 3D view"
            onClick={() => setInsetOpen(false)}
          />
        )}
      </div>
      {mode === "images" && !insetOpen && (
        <Button
          size="sm"
          className="absolute right-3 bottom-3 z-10 shadow-lg"
          icon={<Box className="size-4" />}
          onClick={() => setInsetOpen(true)}
        >
          3D view
        </Button>
      )}
      {mode === "motion" && robots.length > 0 && (
        <div className={`flex shrink-0 flex-col border-t border-line bg-surface ${chartOpen ? "h-60" : ""}`}>
          <div className="flex h-8 items-center gap-2 px-2">
            <Button
              variant="ghost"
              size="sm"
              icon={chartOpen ? <ChevronDown className="size-4" /> : <ChevronUp className="size-4" />}
              onClick={() => setChartOpen(!chartOpen)}
              aria-expanded={chartOpen}
            >
              Joints
            </Button>
            {robots.length > 1 && shownRobot && (
              <Select
                aria-label="Robot"
                value={shownRobot}
                options={robots.map((r) => ({ value: r.id, label: r.id }))}
                onValueChange={(v) => v !== "" && setChartRobot(v)}
                className="w-40"
              />
            )}
            {chartOpen && (
              <SegmentedControl
                value={quantity}
                options={[
                  { value: "position", label: "Position" },
                  { value: "velocity", label: "Velocity" },
                  { value: "acceleration", label: "Acceleration" },
                ]}
                onValueChange={(v) => v !== "" && setQuantity(v as JointQuantity)}
                aria-label="Joint quantity"
              />
            )}
          </div>
          {chartOpen && shownRobot && (
            <div className="min-h-0 flex-1 px-2 pb-2">
              <JointChart
                baked={current.baked}
                robot={shownRobot}
                joints={current.loaded.robots.find((r) => r.id === shownRobot)?.manifest.joints ?? []}
                quantity={quantity}
                playhead={playhead}
                height={150}
              />
            </div>
          )}
        </div>
      )}
    </div>
  ) : (
    <div className="grid h-full place-items-center p-6">
      {failure ? (
        <Callout tone="error" title={failure.title}>
          {failure.message}
        </Callout>
      ) : (
        <Empty>{loading ? "Loading…" : "Open an example from the File menu, or drop a scene.json."}</Empty>
      )}
    </div>
  );

  const left =
    current && tree && mode === "setup" ? (
      <Navigator
        tree={tree}
        selected={selected}
        onSelect={setSelected}
        onAdd={add}
        robots={library}
        onAddRobot={addRobot}
        onRemove={removable ? remove : null}
      />
    ) : null;
  const right =
    current && mode === "setup" ? (
      <Inspector
        current={current}
        preview={editing}
        poses={bodyPose}
        selected={selected}
        failure={failure}
        playhead={playhead}
        live={live}
        onEdit={studio.edit}
        onRenamed={setSelected}
      />
    ) : current && mode === "motion" ? (
      <div className="flex h-full min-h-0 flex-col">
        <div className="border-b border-line p-2">
          <SegmentedControl
            value={motionPanel}
            options={[
              { value: "program", label: "Program" },
              { value: "generate", label: "Generate poses" },
            ]}
            onValueChange={(v) => (v === "program" || v === "generate") && setMotionPanel(v)}
            aria-label="Motion panel"
          />
        </div>
        <div className="min-h-0 flex-1 overflow-auto">
          {motionPanel === "generate" ? (
            <GeneratorPanel
              key={opened}
              session={current.session}
              scene={current.loaded.scene}
              initial={current.loaded.scenario?.generator ?? null}
              onPreview={setPreview}
              onApply={(generated) => {
                if (studio.setScenario(generated.scenario, "generate poses")) setMotionPanel("program");
              }}
            />
          ) : (
            <Panel
              title="Program"
              bodyClassName="p-0"
              actions={
                <Button
                  size="sm"
                  variant="ghost"
                  icon={<Pin className="size-3.5" />}
                  disabled={!unresolved}
                  title={
                    unresolved
                      ? "Store every PTP pose's joint configuration, so the IK branch can never change"
                      : "Every PTP pose's configuration is stored"
                  }
                  onClick={resolveIk}
                >
                  Resolve IK
                </Button>
              }
            >
              {report && (
                <div
                  className="border-b border-line px-3 py-2 font-mono text-xs text-fg-muted"
                  data-testid="motion-summary"
                >
                  {report.duration_s.toFixed(2)} s · {report.captures} captures ·{" "}
                  {report.steps.reduce((n, s) => n + (s.warnings?.length ?? 0), 0)} warnings
                </div>
              )}
              <StepList scenario={current.loaded.scenario} baked={current.baked} report={report} playhead={playhead} />
            </Panel>
          )}
        </div>
      </div>
    ) : null;

  return (
    <>
      <AppShell
        storageKey="etendue-studio"
        header={
          <Header
            label={current?.loaded.label ?? null}
            description={current?.loaded.scene.description ?? null}
            dirty={dirty}
            mode={mode}
            modes={modes}
            onMode={setMode}
            undo={undo}
            redo={redo}
            onUndo={studio.undo}
            onRedo={studio.redo}
            example={example}
            cells={project?.cells ?? NO_CELLS}
            onExample={studio.openExample}
            onFiles={studio.openFiles}
            onPaths={studio.openPaths}
            onPoses={current ? importPoses : null}
            onSave={current ? (as) => void studio.save(as) : null}
            exports={exports}
          />
        }
        left={left}
        leftLabel="Scene frames"
        main={main}
        right={right}
        rightLabel={mode === "motion" ? "Program" : "Inspector"}
        rightSize={{ defaultSize: 380, minSize: 260, maxSize: "50%" }}
        bottom={
          current &&
          mode !== "dataset" && (
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
