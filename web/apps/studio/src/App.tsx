import { type FrameTreeRuntime, type Iso3, invertIso3 } from "@vitavision/three";
import { Button, Callout, ConfirmDialog, Empty, Panel, SegmentedControl, Select } from "@vitavision/ui";
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
import { type ReactNode, useCallback, useEffect, useMemo, useState } from "react";
import library, { type LibraryRobot } from "virtual:etendue-robots";

import { type Cell, defaultOf } from "./io/project";
import { fileUrl, isTauri } from "./io/tauri";
import { describeFailure } from "./kernel/etendue";
import { type Bodies, type PoseOverride, bodyPoses, posed } from "./kernel/posed";
import type { GeneratedPoses } from "@etendue/wasm";
import { scenarioFromPoses } from "./kernel/native";
import { useThrottledPlayhead } from "./kernel/useThrottled";
import { type Jog, bakedQ, jogOverrides, motionSteps, reteach, robotFrames, teach } from "./robot/jog";
import { JogPanel } from "./robot/JogPanel";
import { stepAt, tcpPath } from "./scene/TcpPath";
import { useLibraryWorkspace } from "./library/Workspace";
import { useOpticsWorkspace } from "./optics/Workspace";
import { useRigsWorkspace } from "./rigs/Workspace";
import { useCellBuilder } from "./cell/CellBuilder";
import { CameraGrid } from "./panels/CameraGrid";
import { CellDialog } from "./panels/CellDialog";
import { ErrorBoundary } from "./panels/ErrorBoundary";
import { RobotLibrary } from "./robots/RobotLibrary";
import { GeneratorPanel } from "./panels/GeneratorPanel";
import { exportsOf } from "./panels/exports";
import { Header, type Mode } from "./panels/Header";
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
import { CalibrationRunner } from "./state/calibrationRun";
import { useCalibrateWorkspace } from "./calibrate/Workspace";
import { DatasetRunner } from "./state/datasetRun";
import { useUnsavedGuard } from "./state/guard";
import { useEditShortcuts } from "./state/shortcuts";
import { Studio, initialExample, useStudio } from "./state/studio";

/** A value that belongs to one opened document: it resets when another one opens. */
interface Scoped<T> {
  opened: number;
  value: T;
}

const MODE_KEY = "etendue-studio:mode";
const NO_OVERRIDES: readonly PoseOverride[] = [];
const NO_CELLS: readonly Cell[] = [];

/** The workspaces. Calibrate is in a browser too, as a preview of what a calibration observes. */
const MODES: Mode[] = ["library", "optics", "rigs", "setup", "motion", "images", "dataset"];

function storedMode(modes: Mode[]): Mode {
  try {
    const m = localStorage.getItem(MODE_KEY) as Mode | null;
    if (m !== null && modes.includes(m)) return m;
  } catch {
    // Storage may be unavailable (private window); the default applies.
  }
  return "setup";
}

/** The workspaces that show the open cell, where Undo, Redo and Save act on it. */
const SCENE_MODES: readonly Mode[] = ["setup", "motion", "images", "dataset"];

export function App() {
  const [studio] = useState(() => new Studio(createPlayhead(1, 0.01)));
  const [runner] = useState(() => new DatasetRunner(studio));
  const [calibration] = useState(() => new CalibrationRunner());
  const studioState = useStudio(studio);
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
    reopenable,
    bank,
    question,
  } = studioState;
  const bankName = bank?.manifest.name ?? null;
  // The Library workspace: every item the project sees, to browse and edit.
  const libraryPanels = useLibraryWorkspace(studio, studioState);
  const [cellDialog, setCellDialog] = useState(false);
  const [robotLibrary, setRobotLibrary] = useState(false);
  const { playhead } = studio;
  const [mode, setModeState] = useState<Mode>(() => storedMode(MODES));
  const [selection, setSelection] = useState<Scoped<string | null>>({ opened: 0, value: null });
  const [runtime, setRuntime] = useState<FrameTreeRuntime | null>(null);
  const [focusCamera, setFocusCamera] = useState<string | null>(null);
  const [chartOpen, setChartOpen] = useState(true);
  const [quantity, setQuantity] = useState<JointQuantity>("position");
  const [insetOpen, setInsetOpen] = useState(true);
  const [motionPanel, setMotionPanel] = useState<"program" | "generate" | "jog">("program");
  // A robot jogged at the playhead (P12); it lapses when the playhead moves or the program bakes.
  const [jogState, setJog] = useState<(Jog & { baked: unknown }) | null>(null);
  const [jogRobot, setJogRobot] = useState<string | null>(null);
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
  // The cell builder (Setup): while it is open its steps replace the Navigator and the Inspector.
  const cellBuilder = useCellBuilder(studio, studioState, library, () => setMode("optics"));

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
  const jogK = useThrottledPlayhead(playhead, 50);
  const jogging = current !== null && mode === "motion" && motionPanel === "jog" && !playing;
  const jogFrames = useMemo(() => {
    const robot = jogRobot ?? current?.loaded.scene.robots?.[0]?.id;
    return robot && current ? { robot, frames: robotFrames(current.loaded.robots, robot) } : null;
  }, [current, jogRobot]);
  const jog = useMemo<Jog | null>(() => {
    if (!jogging || !current || !jogFrames) return null;
    if (jogState && jogState.baked === current.baked && jogState.k === jogK && jogState.robot === jogFrames.robot) {
      return jogState;
    }
    const q = bakedQ(current.baked, jogFrames.robot, jogK);
    return q ? { robot: jogFrames.robot, q, k: jogK } : null;
  }, [jogging, current, jogFrames, jogState, jogK]);
  const updateJog = (j: Jog) => current && setJog({ ...j, baked: current.baked });
  const jogOverridesNow = useMemo(
    () =>
      jog && runtime && current && jogFrames?.frames
        ? jogOverrides(current.session, runtime, current.loaded.scene, current.baked.frames, jogFrames.frames.base, jog)
        : NO_OVERRIDES,
    [jog, runtime, current, jogFrames],
  );
  // World poses with the edit in progress (or the jogged robot) drawn over the bake.
  const editOverrides = editing?.overrides ?? NO_OVERRIDES;
  const overrides = useMemo(
    () => (jogOverridesNow.length === 0 ? editOverrides : [...editOverrides, ...jogOverridesNow]),
    [editOverrides, jogOverridesNow],
  );
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

  useUnsavedGuard(studio);

  // Undo, redo and save act only where the cell is shown.
  const sceneShown = SCENE_MODES.includes(mode);
  useEditShortcuts(studio, sceneShown);

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
  // The Calibrate workspace (its dataset, calibration and observations around the 3-D view).
  const calibrate = useCalibrateWorkspace({
    active: mode === "dataset",
    studio,
    current,
    dataset,
    project,
    bank,
    example,
    runner,
    calibration,
    runtime,
    poses,
    playhead,
    renderedAt,
  });
  const { estimate } = calibrate;
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

  const scenario = current?.loaded.scenario ?? null;
  const exports = useMemo(() => exportsOf(current, shownRobot), [current, shownRobot]);
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
  const replaceRobot = (lib: LibraryRobot) => {
    if (!current || selected === null || !isRobot(selected)) return;
    const id = selected;
    void studio.replaceRobot(lib.manifest, id, (d, robot) => ({
      ...d,
      scene: {
        ...d.scene,
        robots: (d.scene.robots ?? []).map((r) =>
          r.id === id
            ? {
                ...r,
                manifest: lib.manifest,
                initial_q: robot.manifest.joints.map((j) => Math.min(Math.max(0, j.lower), j.upper)),
              }
            : r,
        ),
      },
    }));
  };
  const loadLibraryRobot = useCallback((r: LibraryRobot) => studio.loadLibraryRobot(r.manifest, r.id), [studio]);
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
  const jogGizmo: GizmoTarget | null =
    jog && current && jogFrames?.frames
      ? {
          frame: jogFrames.frames.tcp,
          parent: jogFrames.frames.base,
          placement: current.session.robotTcp(jog.robot, jog.q).base_se3_tcp,
          onPreview: (pose) => {
            const answer = current.session.robotIk(jog.robot, pose, jog.q, { restarts: 0 });
            updateJog({ ...jog, q: answer.converged ? answer.q : jog.q, unreachable: !answer.converged });
          },
          onCommit: () => {},
          onCancel: () => {},
        }
      : null;
  const gizmo: GizmoTarget | null =
    jogGizmo ??
    (handled && handleFrame !== null && selected !== null
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
      : null);

  // Each robot's TCP over the program (the Motion workspace).
  const tcpPaths = useMemo(
    () =>
      current
        ? (current.loaded.scene.robots ?? []).flatMap((r) => {
            const frames = robotFrames(current.loaded.robots, r.id);
            const path = frames && tcpPath(current.baked, frames.tcp);
            return path ? [path] : [];
          })
        : [],
    [current],
  );

  // The robot motion at (or that led to) the playhead, which the TCP path highlights.
  const pathStep = mode === "motion" && current ? stepAt(report, current.baked.samples[jogK]?.t ?? 0, true) : -1;
  const paths = useMemo(
    () => ({ data: tcpPaths, report, current: pathStep, onSeek: (k: number) => playhead.set(k) }),
    [tcpPaths, report, pathStep, playhead],
  );

  // The jogged robot's steps, any of which can be re-taught at the jogged pose; the last that ran
  // by the playhead is offered first.
  const jogSteps = useMemo(() => (jog ? motionSteps(current?.loaded.scenario ?? null, jog.robot) : []), [jog, current]);
  const jogStep = jog
    ? (jogSteps.filter((s) => s.index <= stepAt(report, current?.baked.samples[jog.k]?.t ?? 0)).at(-1)?.index ?? null)
    : null;
  const reteachJog = (index: number) => {
    const scenario = current?.loaded.scenario;
    const step = scenario?.steps[index];
    if (!jog || !current || !scenario || !step) return;
    const next = reteach(step, jog.q, current.session.robotTcp(jog.robot, jog.q).base_se3_tcp);
    if (next) {
      studio.setScenario(
        { ...scenario, steps: scenario.steps.map((s, i) => (i === index ? next : s)) },
        `re-teach step ${index + 1}`,
      );
    }
  };

  // The jogged pose as a program step (and a capture), the playhead at its end.
  const teachJog = (capture: boolean) => {
    if (!jog || !current) return;
    const tcp = current.session.robotTcp(jog.robot, jog.q).base_se3_tcp;
    const scenario = teach(current.loaded.scenario, jog.robot, jog.q, tcp, capture);
    if (studio.setScenario(scenario, capture ? "teach a capture" : "teach a pose")) {
      const baked = studio.get().current?.baked;
      if (baked) playhead.set(baked.samples.length - 1);
    }
  };

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
      paths={mode === "motion" ? paths : undefined}
      estimate={estimate}
    />
  ) : null;

  const main = current ? (
    <div
      className={
        mode === "dataset"
          ? "grid h-full min-h-0 grid-cols-[minmax(0,2fr)_minmax(0,3fr)] grid-rows-[minmax(0,3fr)_minmax(0,2fr)]"
          : "relative flex h-full min-h-0 flex-col"
      }
    >
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
      {mode === "dataset" && calibrate.panel}
      {/* The 3D view stays mounted in every workspace: the camera images render its scene. */}
      <div
        className={
          mode === "images"
            ? insetOpen
              ? "absolute right-3 bottom-3 z-10 h-52 w-72 overflow-hidden rounded-md border border-line-strong bg-canvas shadow-lg"
              : "hidden"
            : mode === "dataset"
              ? "relative min-h-0 min-w-0"
              : "relative min-h-0 flex-1"
        }
        data-testid="viewport"
        data-estimate={estimate ? estimate.cameras.length : undefined}
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
      {mode === "dataset" && calibrate.observations}
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

  // Optics and Rigs: designs placed into the open cell under the selected frame; then the Setup
  // workspace, with the placed design selected.
  const placeParent = selected !== null && current?.baked.frames.includes(selected) ? selected : "world";
  const onPlaced = (id: string) => {
    setMode("setup");
    setSelected(id);
  };
  const optics = useOpticsWorkspace({ studio, project, current, loading, parent: placeParent, onPlaced });
  const rigs = useRigsWorkspace({
    studio,
    project,
    head: optics.edited,
    shownHead: optics.params,
    current,
    loading,
    parent: placeParent,
    onPlaced,
  });

  const left =
    current && mode === "setup" && cellBuilder.active ? (
      cellBuilder.left
    ) : current && tree && mode === "setup" ? (
      <Navigator
        tree={tree}
        selected={selected}
        onSelect={setSelected}
        onAdd={add}
        robots={library}
        onAddRobot={addRobot}
        onBrowseRobots={() => setRobotLibrary(true)}
        onRemove={removable ? remove : null}
        onNewCell={() => {
          setSelected(null);
          cellBuilder.start();
        }}
      />
    ) : null;
  const right =
    current && mode === "setup" && cellBuilder.active ? (
      cellBuilder.right
    ) : current && mode === "setup" ? (
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
              { value: "jog", label: "Jog" },
              { value: "generate", label: "Generate poses" },
            ]}
            onValueChange={(v) => {
              if (v === "program" || v === "generate" || v === "jog") setMotionPanel(v);
              if (v === "jog") studio.setPlaying(false);
            }}
            aria-label="Motion panel"
          />
        </div>
        <div className="min-h-0 flex-1 overflow-auto">
          {motionPanel === "jog" ? (
            jog ? (
              <JogPanel
                session={current.session}
                robots={current.loaded.robots}
                jog={jog}
                onJog={updateJog}
                onReset={(robot) => {
                  setJog(null);
                  setJogRobot(robot);
                }}
                onTeach={teachJog}
                steps={jogSteps}
                defaultStep={jogStep}
                onReteach={reteachJog}
              />
            ) : (
              <Empty>{playing ? "Pause to jog." : "The cell has no robot to jog."}</Empty>
            )
          ) : motionPanel === "generate" ? (
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
              <StepList
                scenario={current.loaded.scenario}
                baked={current.baked}
                report={report}
                playhead={playhead}
                onSteps={(steps, label) =>
                  current.loaded.scenario && studio.setScenario({ ...current.loaded.scenario, steps }, label)
                }
              />
            </Panel>
          )}
        </div>
      </div>
    ) : null;

  // A pane that fails to render shows why, and starts again in another workspace or document.
  const guarded = (pane: ReactNode) => (
    <ErrorBoundary area="This pane" resetKey={`${mode}/${opened}`}>
      {pane}
    </ErrorBoundary>
  );

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
            modes={MODES}
            onMode={setMode}
            undo={sceneShown ? undo : null}
            redo={sceneShown ? redo : null}
            onUndo={studio.undo}
            onRedo={studio.redo}
            example={example}
            project={project ? { name: project.manifest.name, writable: project.write !== null } : null}
            cells={project?.cells ?? NO_CELLS}
            onExample={studio.openExample}
            onProject={studio.canOpenProjects() ? (create) => void studio.openProject(create) : null}
            reopenable={reopenable}
            onReopen={() => void studio.reopenProject()}
            bank={bankName}
            onBank={!isTauri() && studio.canOpenProjects() ? () => void studio.chooseBank() : null}
            onSaveAsCell={current && project?.write ? () => setCellDialog(true) : null}
            onFiles={studio.openFiles}
            onPaths={studio.openPaths}
            onPoses={current ? importPoses : null}
            onSave={current ? (as) => void studio.save(as) : null}
            exports={exports}
          />
        }
        left={guarded(
          mode === "library"
            ? libraryPanels.left
            : mode === "optics"
              ? optics.left
              : mode === "rigs"
                ? rigs.left
                : left,
        )}
        leftLabel={
          mode === "library" ? "Library items" : mode === "optics" || mode === "rigs" ? "Design" : "Scene frames"
        }
        main={guarded(
          mode === "library"
            ? libraryPanels.main
            : mode === "optics"
              ? optics.main
              : mode === "rigs"
                ? rigs.main
                : main,
        )}
        right={guarded(
          mode === "library"
            ? libraryPanels.right
            : mode === "optics"
              ? optics.right
              : mode === "rigs"
                ? rigs.right
                : right,
        )}
        rightLabel={
          mode === "library"
            ? "Layers"
            : mode === "optics" || mode === "rigs"
              ? "Figures"
              : mode === "motion"
                ? "Program"
                : "Inspector"
        }
        rightSize={{ defaultSize: 380, minSize: 260, maxSize: "50%" }}
        bottom={
          current &&
          mode !== "library" &&
          mode !== "optics" &&
          mode !== "rigs" && (
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
      {project && (
        <CellDialog
          open={cellDialog}
          onOpenChange={setCellDialog}
          project={project.manifest.name}
          taken={project.manifest.cells ?? []}
          onSave={studio.saveAsCell}
        />
      )}
      <RobotLibrary
        open={robotLibrary}
        onOpenChange={setRobotLibrary}
        robots={library}
        load={loadLibraryRobot}
        attachTo={selected !== null && current?.baked.frames.includes(selected) ? selected : "world"}
        replace={isRobot(selected) ? selected : null}
        onAdd={addRobot}
        onReplace={replaceRobot}
      />
      <ConfirmDialog
        open={question !== null}
        onOpenChange={(open) => !open && studio.answer(false)}
        title={question?.title ?? ""}
        description={question?.description ?? ""}
        confirmLabel={question?.confirmLabel ?? "OK"}
        destructive
        onConfirm={() => studio.answer(true)}
      />
      <Toaster />
    </>
  );
}
