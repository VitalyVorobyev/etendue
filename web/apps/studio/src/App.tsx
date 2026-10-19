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
import { describeFailure, expandRig } from "./kernel/etendue";
import { type Bodies, type PoseOverride, bodyPoses, posed } from "./kernel/posed";
import type { GeneratedPoses } from "@etendue/wasm";
import { scenarioFromPoses } from "./kernel/native";
import { useThrottledPlayhead } from "./kernel/useThrottled";
import { analyse } from "./optics/analysis";
import { type HeadParams, initialHead, placement, toItem } from "./optics/design";
import { OpticsForm, OpticsReadouts } from "./optics/OpticsPanels";
import { PlaneMap } from "./optics/PlaneMap";
import { SectionView } from "./optics/SectionView";
import { RangeViews } from "./optics/RangeViews";
import { analyse as analyseRig } from "./rigs/analysis";
import { type RigParams, initialRig, toItem as toRigItem, toLayout } from "./rigs/design";
import { RigForm, RigReadouts, TiltChart } from "./rigs/RigPanels";
import { RigView, type VoxelShow } from "./rigs/RigView";
import { type Jog, bakedQ, jogOverrides, motionSteps, reteach, robotFrames, teach } from "./robot/jog";
import { JogPanel } from "./robot/JogPanel";
import { stepAt, tcpPath } from "./scene/TcpPath";
import { CoverageLegend, SliceMaps } from "./rigs/SliceMaps";
import { CameraGrid } from "./panels/CameraGrid";
import { CellDialog } from "./panels/CellDialog";
import { RobotLibrary } from "./robots/RobotLibrary";
import { CALIBRATE_FORM, type CalibrateForm, CalibratePanel, noiseOf } from "./panels/CalibratePanel";
import { CameraView } from "./panels/CameraView";
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
  applyLayout,
  entityOf,
  newEntity,
  removeEntity,
  removeRobot,
  uniqueId,
  updateEntity,
} from "./scene/edit";
import { sceneTree } from "./scene/model";
import { CalibrationRunner, useCalibrationRun } from "./state/calibrationRun";
import { estimateDrawing, observedAt as observedIn } from "./calibrate/compare";
import { ObservationPreview } from "./calibrate/ObservationPreview";
import { useObservations } from "./calibrate/observations";
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

/** The workspaces. Calibrate is in a browser too, as a preview of what a calibration observes. */
const MODES: Mode[] = ["optics", "rigs", "setup", "motion", "images", "dataset"];

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
  const [calibration] = useState(() => new CalibrationRunner());
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
  } = useStudio(studio);
  const [cellDialog, setCellDialog] = useState(false);
  const [robotLibrary, setRobotLibrary] = useState(false);
  // The Optics workspace's head, kept across workspaces; it starts from the project's parts.
  const [head, setHead] = useState<HeadParams | null>(null);
  const startHead = useMemo(() => (project ? initialHead(project) : null), [project]);
  const headParams = head ?? startHead;
  const headAnalysis = useMemo(() => (headParams ? analyse(headParams) : null), [headParams]);
  // The Rigs workspace's rig: a stereo pair of the head (or the project's default camera) at first.
  const [rig, setRig] = useState<RigParams | null>(null);
  const [voxels, setVoxels] = useState<VoxelShow>({ kind: "views", min: 2 });
  const [rigSelected, setRigSelected] = useState<string | null>(null);
  const startRig = useMemo(
    () => (project ? initialRig(project, head) : null),
    // A head edited later does not replace the rig's unit; only the first one seeds it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [project],
  );
  const rigParams = rig ?? startRig;
  const rigAnalysis = useMemo(() => (rigParams ? analyseRig(rigParams) : null), [rigParams]);
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
  // The Calibrate workspace: what it calibrates from, how much the 3-D view exaggerates the
  // estimate, and the camera whose observations it shows.
  const [calibrateForm, setCalibrateForm] = useState<CalibrateForm>(CALIBRATE_FORM);
  const [estimateScale, setEstimateScale] = useState(100);
  const [observedCamera, setObservedCamera] = useState<string | null>(null);
  const calibrationRun = useCalibrationRun(calibration);

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
  // What a calibration of the open scene observes (the Calibrate workspace): exact, or with the
  // form's noise; a detector's observations are not previewed.
  const observeNoise = useMemo(
    () =>
      mode !== "dataset"
        ? null
        : calibrateForm.source === "noisy"
          ? noiseOf(calibrateForm.noise)
          : calibrateForm.source === "exact"
            ? {}
            : null,
    [mode, calibrateForm],
  );
  const { observations, error: observeError } = useObservations(current, observeNoise);
  const observedAt = useCallback(
    (camera: string, k: number) => (current ? observedIn(observations, current.baked, camera, k) : null),
    [current, observations],
  );
  const shownCamera = cameras.find((c) => c.id === observedCamera) ?? cameras[0] ?? null;
  // A project opened for writing keeps its datasets and calibration runs (ADR 0009); otherwise a
  // dataset keeps its own runs.
  const projectFolder =
    project?.write && project.source.absolute ? project.source.absolute(project.root).replace(/[\\/]+$/, "") : null;
  const cellName = project?.cells.some((c) => c.id === example) ? example.split("/").at(-1) : undefined;
  const projectDatasets = projectFolder && cellName ? `${projectFolder}/datasets/${cellName}` : null;
  // The last calibration of this dataset (its first trial), drawn over the truth in 3-D.
  const calibrated =
    dataset && calibrationRun.result?.dir === dataset.output ? (calibrationRun.result.run.reports[0] ?? null) : null;
  const estimate = useMemo(() => {
    if (mode !== "dataset" || !calibrated || !current) return null;
    const extent = (id: string) => {
      try {
        return current.session.targetExtent(id) ?? null;
      } catch {
        return null;
      }
    };
    return estimateDrawing(calibrated, estimateScale, current.loaded.scene, current.baked, extent);
  }, [mode, calibrated, current, estimateScale]);
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
      {mode === "dataset" && (
        <div className="row-span-2 min-h-0 overflow-auto border-r border-line p-4">
          {isTauri() ? (
            <DatasetPanel current={current} dataset={dataset} runner={runner} projectFolder={projectDatasets} />
          ) : (
            <ObservationPreview
              form={calibrateForm}
              onForm={setCalibrateForm}
              lasers={(scene?.lasers ?? []).length > 0}
              observations={observations}
            />
          )}
          {dataset && (
            <section className="mt-6 border-t border-line pt-4" aria-label="Calibrate">
              <h2 className="mb-3 text-sm font-medium">Calibrate and compare</h2>
              <CalibratePanel
                dataset={dataset}
                cameras={(current.loaded.scene.cameras ?? []).map((c) => c.id)}
                lasers={(current.loaded.scene.lasers ?? []).length > 0}
                runner={calibration}
                form={calibrateForm}
                onForm={setCalibrateForm}
                scale={estimateScale}
                onScale={setEstimateScale}
                runsFolder={projectFolder ? `${projectFolder}/runs` : `${dataset.output}/runs`}
              />
            </section>
          )}
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
      {mode === "dataset" && (
        <section
          className="flex min-h-0 min-w-0 flex-col border-t border-line"
          aria-label="Observations"
          data-testid="observations"
        >
          <div className="flex shrink-0 flex-wrap items-center gap-2 px-2 py-1 text-xs">
            <span className="font-medium">Observations</span>
            {cameras.length > 1 && shownCamera && (
              <Select
                aria-label="Observed camera"
                value={shownCamera.id}
                options={cameras.map((c) => ({ value: c.id, label: c.id }))}
                onValueChange={setObservedCamera}
              />
            )}
            <span className="text-fg-muted">
              {calibrateForm.source === "detected"
                ? "The detector's observations are in the dataset's images."
                : (observeError ??
                  "At a capture, a dot is what the calibration observes (an outlier larger, in red) and a cross the truth; a cross without a dot was dropped.")}
            </span>
          </div>
          {shownCamera && runtime && (
            <div className="min-h-0 flex-1">
              <CameraView
                session={current.session}
                runtime={runtime}
                poses={poses ?? undefined}
                camera={shownCamera}
                targets={scene?.targets ?? []}
                lasers={scene?.lasers ?? []}
                playhead={playhead}
                renderedAt={renderedAt}
                observedAt={observedAt}
              />
            </div>
          )}
        </section>
      )}
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

  // A head from the Optics workspace into the open cell: under the selected frame, or above the
  // world origin looking down; then the Setup workspace, with its camera selected.
  const placeHead = (p: HeadParams) => {
    if (!current) return;
    const scene = current.loaded.scene;
    const parent = selected !== null && current.baked.frames.includes(selected) ? selected : "world";
    const pose =
      parent === "world"
        ? {
            rotation: [1, 0, 0, 0] as [number, number, number, number],
            translation: [0, 0, 0.8] as [number, number, number],
          }
        : IDENTITY;
    const cameraId = uniqueId(scene, "head_cam");
    const laserId = uniqueId(
      { ...scene, cameras: [...(scene.cameras ?? []), { id: cameraId } as never] },
      "head_laser",
    );
    const { camera, laser } = placement(p, parent, pose, cameraId, laserId);
    const ok = studio.edit(`place ${p.name}`, (d) => ({
      ...d,
      scene: addEntity(addEntity(d.scene, "cameras", camera), "lasers", laser),
    }));
    if (ok) {
      setMode("setup");
      setSelected(cameraId);
    }
  };

  // A rig from the Rigs workspace into the open cell: a rig frame under the selected frame (or 0.8 m
  // above the world origin, looking down), its cameras and lasers expanded from the layout.
  const placeRig = (p: RigParams) => {
    if (!current) return;
    const scene = current.loaded.scene;
    const parent = selected !== null && current.baked.frames.includes(selected) ? selected : "world";
    const pose =
      parent === "world"
        ? {
            rotation: [1, 0, 0, 0] as [number, number, number, number],
            translation: [0, 0, 0.8] as [number, number, number],
          }
        : IDENTITY;
    const id = uniqueId(scene, p.id || "rig");
    const layout = toLayout(p);
    const ok = studio.edit(`place ${p.name}`, (d) => {
      const withRig = { ...d.scene, rigs: [...(d.scene.rigs ?? []), { id, parent, parent_se3_self: pose, layout }] };
      return { ...d, scene: applyLayout(withRig, id, layout, expandRig(layout, id)) };
    });
    if (ok) {
      setMode("setup");
      setSelected(id);
    }
  };

  // The Rigs workspace: k identical units as a stereo pair or a ring, and what they cover (P11).
  const rigsLeft =
    rigParams && rigAnalysis ? (
      <RigForm
        params={rigParams}
        onChange={setRig}
        analysis={rigAnalysis}
        project={project}
        head={headParams}
        onSave={project?.write ? (p) => studio.saveLibraryItem(toRigItem(p)) : null}
        onPlace={current ? placeRig : null}
      />
    ) : null;
  const rigsMain =
    rigParams && rigAnalysis ? (
      typeof rigAnalysis === "string" ? (
        <div className="p-4">
          <Callout tone="warning" title="The kernel refused this rig">
            <p>{rigAnalysis}</p>
          </Callout>
        </div>
      ) : (
        <div className="grid h-full min-h-0 grid-rows-[3fr_2fr] divide-y divide-line">
          <div className="relative min-h-0">
            <RigView analysis={rigAnalysis} show={voxels} selected={rigSelected} onSelect={setRigSelected} />
            <div className="pointer-events-none absolute bottom-2 left-3 rounded bg-surface/80 px-2 py-1">
              <CoverageLegend cameras={rigAnalysis.cameras.length} pairs={rigAnalysis.fans.length} />
            </div>
          </div>
          <div className="grid min-h-0 grid-cols-[2fr_1fr] divide-x divide-line">
            <SliceMaps analysis={rigAnalysis} params={rigParams} onChange={setRig} />
            <div className="min-h-0 p-2" data-testid="rig-tilt-chart">
              <TiltChart params={rigParams} />
            </div>
          </div>
        </div>
      )
    ) : (
      <Empty>{loading ? "Loading…" : "The Rigs workspace needs a camera or a head in the project's library."}</Empty>
    );
  const rigsRight =
    rigParams && rigAnalysis ? (
      <RigReadouts params={rigParams} analysis={rigAnalysis} show={voxels} onShow={setVoxels} />
    ) : null;

  // The Optics workspace: a head designed against its window (P10).
  const opticsLeft =
    headParams && headAnalysis ? (
      <OpticsForm
        params={headParams}
        onChange={setHead}
        analysis={headAnalysis}
        project={project}
        onSave={project?.write ? (p) => studio.saveLibraryItem(toItem(p)) : null}
        onPlace={current ? placeHead : null}
      />
    ) : null;
  const opticsMain =
    headParams && headAnalysis ? (
      typeof headAnalysis === "string" ? (
        <div className="p-4">
          <Callout tone="warning" title="The kernel refused this head">
            <p>{headAnalysis}</p>
          </Callout>
        </div>
      ) : (
        <div className="grid h-full min-h-0 grid-rows-[3fr_2fr] divide-y divide-line">
          <SectionView section={headAnalysis.section} params={headParams} onChange={setHead} />
          <div className="grid min-h-0 grid-cols-[minmax(0,3fr)_minmax(0,2fr)] divide-x divide-line">
            <PlaneMap
              map={headAnalysis.map}
              range={headAnalysis.range}
              width={headParams.width_m}
              blurLimitPx={headParams.max_blur_px}
            />
            <RangeViews
              map={headAnalysis.map}
              range={headAnalysis.range}
              resolution={headAnalysis.design.camera.resolution}
              blurLimitPx={headParams.max_blur_px}
              focus={headParams.laser.beam_waist_m !== null ? headParams.laser.waist_distance_m : null}
            />
          </div>
        </div>
      )
    ) : (
      <Empty>{loading ? "Loading…" : "The Optics workspace needs a project's parts."}</Empty>
    );
  const opticsRight =
    headParams && headAnalysis ? <OpticsReadouts params={headParams} analysis={headAnalysis} /> : null;

  const left =
    current && tree && mode === "setup" ? (
      <Navigator
        tree={tree}
        selected={selected}
        onSelect={setSelected}
        onAdd={add}
        robots={library}
        onAddRobot={addRobot}
        onBrowseRobots={() => setRobotLibrary(true)}
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
            undo={undo}
            redo={redo}
            onUndo={studio.undo}
            onRedo={studio.redo}
            example={example}
            project={project ? { name: project.manifest.name, writable: project.write !== null } : null}
            cells={project?.cells ?? NO_CELLS}
            onExample={studio.openExample}
            onProject={studio.canOpenProjects() ? (create) => void studio.openProject(create) : null}
            reopenable={reopenable}
            onReopen={() => void studio.reopenProject()}
            onSaveAsCell={current && project?.write ? () => setCellDialog(true) : null}
            onFiles={studio.openFiles}
            onPaths={studio.openPaths}
            onPoses={current ? importPoses : null}
            onSave={current ? (as) => void studio.save(as) : null}
            exports={exports}
          />
        }
        left={mode === "optics" ? opticsLeft : mode === "rigs" ? rigsLeft : left}
        leftLabel={mode === "optics" || mode === "rigs" ? "Design" : "Scene frames"}
        main={mode === "optics" ? opticsMain : mode === "rigs" ? rigsMain : main}
        right={mode === "optics" ? opticsRight : mode === "rigs" ? rigsRight : right}
        rightLabel={mode === "optics" || mode === "rigs" ? "Figures" : mode === "motion" ? "Program" : "Inspector"}
        rightSize={{ defaultSize: 380, minSize: 260, maxSize: "50%" }}
        bottom={
          current &&
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
      <Toaster />
    </>
  );
}
