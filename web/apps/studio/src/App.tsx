import { type FrameTreeRuntime, type Iso3, invertIso3 } from "@vitavision/three";
import { Button, Callout, ConfirmDialog, Empty } from "@vitavision/ui";
import {
  AppShell,
  PlaybackBar,
  type PlaybackMarker,
  Toaster,
  createPlayhead,
  toast,
  usePlaybackClock,
} from "@vitavision/workbench";
import { Box, Minimize2 } from "lucide-react";
import { type ReactNode, useCallback, useEffect, useMemo, useState } from "react";
import library from "virtual:etendue-robots";

import type { Cell } from "./io/project";
import { fileUrl, isTauri } from "./io/tauri";
import { type Bodies, type PoseOverride, bodyPoses, posed } from "./kernel/posed";
import { scenarioFromPoses } from "./kernel/native";
import { useLibraryWorkspace } from "./library/Workspace";
import { useMotionWorkspace } from "./motion/Workspace";
import { useOpticsWorkspace } from "./optics/Workspace";
import { useRigsWorkspace } from "./rigs/Workspace";
import { useSetupWorkspace } from "./setup/Workspace";
import { useCellBuilder } from "./cell/CellBuilder";
import { CameraGrid } from "./panels/CameraGrid";
import { CellDialog } from "./panels/CellDialog";
import { ErrorBoundary } from "./panels/ErrorBoundary";
import { exportsOf } from "./panels/exports";
import { Header, type Mode } from "./panels/Header";
import { Viewport } from "./panels/Viewport";
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
  const { playhead } = studio;
  const [mode, setModeState] = useState<Mode>(() => storedMode(MODES));
  const [selection, setSelection] = useState<Scoped<string | null>>({ opened: 0, value: null });
  const [runtime, setRuntime] = useState<FrameTreeRuntime | null>(null);
  const [focusCamera, setFocusCamera] = useState<string | null>(null);
  const [insetOpen, setInsetOpen] = useState(true);
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
  const robots = current?.baked.robots ?? [];
  const selectedRobot = robots.find((r) => r.id === selected || (selected !== null && selected.startsWith(`${r.id}/`)));
  // The Motion workspace (its jogged robot drawn over the bake, its paths in the 3-D view).
  const motion = useMotionWorkspace({
    active: mode === "motion",
    studio,
    current,
    opened,
    playing,
    playhead,
    runtime,
    selectedRobot: selectedRobot?.id ?? null,
  });
  const jogOverridesNow = motion.overrides;
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

  usePlaybackClock({ playhead, playing, speed, loop, onEnd: () => studio.setPlaying(false) });

  useEffect(() => {
    studio.start(initialExample());
  }, [studio]);
  useEffect(() => () => studio.dispose(), [studio]);

  useUnsavedGuard(studio);

  // Undo, redo and save act only where the cell is shown.
  const sceneShown = SCENE_MODES.includes(mode);
  useEditShortcuts(studio, sceneShown);

  const markers = useMemo<PlaybackMarker[]>(
    () =>
      current?.baked.samples.flatMap((s, index) => (s.capture ? [{ index, label: `capture ${s.capture.id}` }] : [])) ??
      [],
    [current],
  );

  const scene = current?.loaded.scene;
  const cameras = scene?.cameras ?? [];
  const { shownRobot } = motion;
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

  const exports = useMemo(() => exportsOf(current, shownRobot), [current, shownRobot]);
  // The Setup workspace (the frame tree, the Inspector, the selection's handle).
  const setup = useSetupWorkspace({
    active: mode === "setup",
    studio,
    current,
    editing,
    project,
    failure,
    selected,
    onSelect: setSelected,
    cellBuilder,
    poses: bodyPose,
    playhead,
  });
  const gizmo = motion.gizmo ?? setup.gizmo;

  const viewport = current ? (
    <Viewport
      current={current}
      playhead={playhead}
      selected={selected}
      onSelect={setSelected}
      onRuntime={onRuntime}
      candidates={motion.candidates}
      overrides={overrides}
      bodies={bodies}
      poses={bodyPose}
      gizmo={gizmo}
      paths={mode === "motion" ? motion.paths : undefined}
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
      {mode === "motion" && motion.chart}
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

  const left = mode === "setup" ? setup.left : null;
  const right = mode === "setup" ? setup.right : mode === "motion" ? motion.right : null;

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
      {setup.dialog}
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
