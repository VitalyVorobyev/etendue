/**
 * The cell builder: a stepper in the Setup workspace that makes a new cell from library items —
 * optics, a rig, how it is mounted, a target and a robot — with the kernel's checks at each step.
 * The cell is shown in the 3D view as it is built (an edit in progress) and becomes one undoable
 * edit when it is built; Cancel puts back what was open. Library items from the bank or the
 * built-ins that the cell uses are copied into the open project when it can be written.
 */

import type { Iso3Schema } from "@etendue/wasm";
import { quaternionFromRpy, rpyFromQuaternion } from "@vitavision/three";
import {
  Badge,
  Button,
  Callout,
  Field,
  Input,
  NumberInput,
  PoseInput,
  SegmentedControl,
  Select,
  Textarea,
  toast,
} from "@vitavision/ui";
import { Check, ChevronLeft, ChevronRight, Crosshair, Hammer, X } from "lucide-react";
import { type ReactNode, useEffect, useMemo, useState } from "react";
import type { LibraryRobot } from "virtual:etendue-robots";

import { type LoadedRobot, libraryPath } from "../io/load";
import { type LibraryKind, type Project, itemsOf } from "../io/project";
import { expandRig } from "../kernel/etendue";
import { layerTag } from "../panels/controls/Library";
import type { Studio, StudioState } from "../state/studio";
import {
  type CellDraft,
  IDENTITY,
  type Problem,
  STEPS,
  type StepId,
  assembleCell,
  defaultPositions,
  layoutCameraId,
  nominalDistance,
  opticsUnit,
  renamePositions,
  targetCoverage,
} from "./assemble";
import { type CellChecks, type View, aimRobot, checkCell } from "./checks";

const DEG = 180 / Math.PI;
/** The edit's label: what Undo names. */
const LABEL = "build a cell";
const RPY = { toEuler: rpyFromQuaternion, fromEuler: quaternionFromRpy, labels: ["roll", "pitch", "yaw"] } as const;

const TITLES: Record<StepId | "review", string> = {
  optics: "Optics",
  rig: "Rig",
  mounting: "Mounting",
  target: "Target",
  robot: "Robot",
  review: "Review",
};
const PAGES = [...STEPS, "review"] as const;
type Page = (typeof PAGES)[number];

/** A first draft: the project's default camera and target, the open cell's robot, eye-in-hand. */
function initialDraft(
  project: Project | null,
  robots: readonly LibraryRobot[],
  open: StudioState["current"],
): CellDraft {
  const ref = (kind: "camera" | "target") => {
    const named = project?.manifest.defaults?.[kind];
    if (named && project?.library.has(named)) return named;
    return project ? (itemsOf(project, kind)[0]?.[0] ?? null) : null;
  };
  const scene = open?.loaded.scene;
  // The open cell's robot, as the library robot its manifest names (scenes name it relative to themselves).
  const libraryOf = (manifest: string) => robots.find((l) => l.manifest === (libraryPath(manifest) ?? manifest));
  const openRobot = scene?.robots?.find((r) => libraryOf(r.manifest) !== undefined);
  const robot = (openRobot && libraryOf(openRobot.manifest)) ?? robots[0] ?? null;
  return {
    description: "",
    optics: ref("camera"),
    rig: { id: "rig", design: null, cameraId: "cam", positions: [] },
    mounting: "eye_in_hand",
    mount: null,
    flange: IDENTITY,
    stand: { id: "stand", pose: { rotation: [0, 0, 0, 1], translation: [0.5, -0.6, 0] } },
    // Half a metre up, looking along the world's +Y.
    rigOnStand: { rotation: [-Math.SQRT1_2, 0, 0, Math.SQRT1_2], translation: [0, 0, 0.5] },
    target: { id: "board", item: ref("target"), pose: { rotation: [0, 0, 0, 1], translation: [0.45, 0, 0] } },
    robot: {
      id: robot?.id ?? "robot",
      manifest: robot?.manifest ?? null,
      base: IDENTITY,
      q: (openRobot && robot && libraryOf(openRobot.manifest) === robot ? openRobot.initial_q : null) ?? [],
    },
    workingDistance: null,
  };
}

/** Whether two lists name the same robots with the same models. */
function sameRobots(a: readonly LoadedRobot[], b: readonly LoadedRobot[]): boolean {
  return (
    a.length === b.length &&
    a.every(
      (r, i) =>
        r.id === b[i]!.id && r.urdf === b[i]!.urdf && JSON.stringify(r.manifest) === JSON.stringify(b[i]!.manifest),
    )
  );
}

/** Every joint at 0, or at its nearest limit. */
function restQ(robot: LoadedRobot): number[] {
  return robot.manifest.joints.map((j) => Math.min(Math.max(0, j.lower), j.upper));
}

function Pose({
  label,
  value,
  onChange,
  description,
}: {
  label: string;
  value: Iso3Schema;
  onChange: (pose: Iso3Schema) => void;
  description?: string;
}) {
  return (
    <Field label={label} as="group" description={description}>
      <PoseInput
        aria-label={`${label}: pose`}
        value={value}
        rotationView={RPY}
        translationUnit="mm"
        onValueChange={(v) => onChange({ rotation: [...v.rotation], translation: [...v.translation] })}
      />
    </Field>
  );
}

function Id({ label, value, onChange }: { label: string; value: string; onChange: (id: string) => void }) {
  return (
    <Field label={label}>
      <Input
        aria-label={label}
        className="font-mono"
        value={value}
        spellCheck={false}
        onChange={(e) => onChange(e.target.value.trim())}
      />
    </Field>
  );
}

/** A picker of library items of `kinds`, by name, tagged with their layer. */
function ItemSelect({
  label,
  project,
  kinds,
  value,
  onChange,
  unset,
}: {
  label: string;
  project: Project | null;
  kinds: LibraryKind[];
  value: string | null;
  onChange: (ref: string | null) => void;
  /** The entry for none, when none is a choice. */
  unset?: string;
}) {
  const items = project ? kinds.flatMap((k) => itemsOf(project, k)) : [];
  return (
    <Field label={label}>
      <Select
        aria-label={label}
        value={value ?? ""}
        placeholder={unset ?? "Choose…"}
        unsetLabel={unset}
        options={items.map(([ref, i]) => ({ value: ref, label: `${i.name}${layerTag(project, ref)}`, note: ref }))}
        onValueChange={(v) => onChange(v === "" ? null : v)}
      />
    </Field>
  );
}

function Views({ checks }: { checks: CellChecks | null }) {
  if (!checks) return null;
  if (checks.problem) {
    return (
      <Callout tone="error" title="The kernel refuses this cell">
        {checks.problem}
      </Callout>
    );
  }
  return (
    <div className="space-y-1" data-testid="cell-views">
      <p className="text-xs text-fg-muted">At the start pose:</p>
      <ul className="space-y-1">
        {checks.views.map((v: View) => (
          <li key={v.camera} className="flex items-center gap-2 text-xs">
            <span className="min-w-0 flex-1 truncate font-mono">{v.camera}</span>
            {v.distance_m !== null && (
              <span className="font-mono tabular-nums text-fg-muted">{(v.distance_m * 1000).toFixed(0)} mm</span>
            )}
            <Badge tone={v.total === 0 ? "neutral" : v.seen === v.total ? "normal" : v.seen > 0 ? "warning" : "defect"}>
              {v.total === 0 ? "no points" : `sees ${v.seen} of ${v.total}`}
            </Badge>
          </li>
        ))}
      </ul>
    </div>
  );
}

function ProblemList({ problems }: { problems: Problem[] }) {
  if (problems.length === 0) return null;
  return (
    <ul className="space-y-0.5 text-xs text-warn" data-testid="cell-problems">
      {problems.map((p) => (
        <li key={`${p.step}:${p.message}`}>{p.message}</li>
      ))}
    </ul>
  );
}

/**
 * The builder's state and panels. `start` opens it (in the Setup workspace); while it is open,
 * `left` (the steps) and `right` (the step) replace the Navigator and the Inspector.
 */
export function useCellBuilder(
  studio: Studio,
  state: StudioState,
  robots: readonly LibraryRobot[],
  onOptics: () => void,
): { active: boolean; start: () => void; left: ReactNode; right: ReactNode } {
  const { project } = state;
  const [draft, setDraft] = useState<CellDraft | null>(null);
  const [page, setPage] = useState<Page>("optics");
  const [loaded, setLoaded] = useState<{ manifest: string; robot: LoadedRobot } | null>(null);
  const [aim, setAim] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const library = project?.library;

  // The chosen robot's model, read once per robot.
  const manifest = draft?.robot.manifest ?? null;
  useEffect(() => {
    if (manifest === null || loaded?.manifest === manifest) return;
    let live = true;
    const lib = robots.find((r) => r.manifest === manifest);
    void studio
      .loadLibraryRobot(manifest, lib?.id ?? "robot")
      .then((robot) => {
        if (!live) return;
        setLoaded({ manifest, robot });
        // A robot of another arm starts at rest.
        setDraft((d) =>
          d && d.robot.manifest === manifest && d.robot.q.length !== robot.manifest.joints.length
            ? { ...d, robot: { ...d.robot, q: restQ(robot) } }
            : d,
        );
      })
      .catch((e: unknown) => {
        if (live) toast({ title: "Could not read the robot", description: String(e), tone: "error" });
      });
    return () => {
      live = false;
    };
  }, [manifest, loaded, robots, studio]);

  const robot = loaded && draft && loaded.manifest === draft.robot.manifest ? loaded.robot : null;
  const robotId = draft?.robot.id;
  // The scene's robot under the draft's id; the same object while neither changes, so the kernel
  // parses the URDF once.
  const sceneRobots = useMemo(() => (robot && robotId ? [{ ...robot, id: robotId }] : null), [robot, robotId]);

  const assembled = useMemo(
    () =>
      draft && library
        ? assembleCell(draft, { library, tcpLink: robot?.manifest.tcp_link ?? null, expand: expandRig })
        : null,
    [draft, library, robot],
  );
  const cell = assembled?.cell ?? null;
  const problems = assembled?.problems ?? [];
  const targetId = draft?.target.id;
  const checks = useMemo(
    () => (cell && sceneRobots && targetId ? checkCell(cell.scene, sceneRobots, targetId) : null),
    [cell, sceneRobots, targetId],
  );

  // The cell as built so far, in the 3D view: an edit in progress. The open document's robot
  // models are kept when they are the cell's (the kernel's session is kept with them).
  useEffect(() => {
    if (!cell || !sceneRobots) return;
    studio.preview(LABEL, (d) => ({
      ...d,
      scene: cell.scene,
      scenario: cell.scenario,
      robots: sameRobots(d.robots, sceneRobots) ? d.robots : sceneRobots,
    }));
  }, [cell, sceneRobots, studio]);

  const start = () => {
    studio.commit();
    studio.setPlaying(false);
    setDraft(initialDraft(project, robots, state.current));
    setPage("optics");
    setAim(null);
  };
  const cancel = () => {
    studio.cancel();
    setDraft(null);
  };
  const build = async () => {
    if (!cell) return;
    setBusy(true);
    try {
      if (!studio.commitNew()) return;
      setDraft(null);
      const open = studio.get().project;
      if (open?.write) {
        for (const ref of cell.used) {
          const layer = open.layers.get(ref)?.layer;
          if (layer === "bank" || layer === "builtin") await studio.copyIntoProject(ref);
        }
      }
      toast({
        title: "Built the cell",
        description: open?.write ? "Save it as a cell of the project (File menu)." : "Save it from the File menu.",
        tone: "success",
        duration: 3000,
      });
    } finally {
      setBusy(false);
    }
  };

  if (!draft || !library) return { active: false, start, left: null, right: null };

  const set = (patch: Partial<CellDraft>) => setDraft({ ...draft, ...patch });
  const stepProblems = (step: StepId) => problems.filter((p) => p.step === step);
  const unit = opticsUnit(draft, library);
  const design = draft.rig.design ? library.get(draft.rig.design)?.rig?.layout : undefined;
  const distance = nominalDistance(draft, library);
  const eyeInHand = draft.mounting === "eye_in_hand";

  const left = (
    <nav className="flex h-full min-h-0 flex-col" aria-label="Cell builder steps" data-testid="cell-steps">
      <div className="flex h-9 shrink-0 items-center border-b border-line px-3">
        <span className="flex-1 text-xs font-medium text-fg-muted">New cell</span>
      </div>
      <ol className="flex flex-col gap-0.5 p-2">
        {PAGES.map((p, i) => {
          const issues = p === "review" ? problems.length : stepProblems(p).length;
          return (
            <li key={p}>
              <button
                type="button"
                aria-current={p === page ? "step" : undefined}
                className={`flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm ${
                  p === page ? "bg-surface-raised font-medium text-fg" : "text-fg-muted hover:bg-surface-raised"
                }`}
                onClick={() => setPage(p)}
              >
                <span className="w-5 font-mono text-xs tabular-nums">{String(i + 1).padStart(2, "0")}</span>
                <span className="flex-1">{TITLES[p]}</span>
                {p !== "review" &&
                  (issues > 0 ? (
                    <span className="text-xs text-warn">{issues}</span>
                  ) : (
                    <Check className="size-3.5 text-signal" aria-label="ready" />
                  ))}
              </button>
            </li>
          );
        })}
      </ol>
    </nav>
  );

  let body: ReactNode;
  switch (page) {
    case "optics": {
      body = (
        <>
          <ItemSelect
            label="Optics"
            project={project}
            kinds={["camera", "head"]}
            value={draft.optics}
            unset="None: the rig design's own cameras"
            onChange={(optics) => set({ optics })}
          />
          {unit && (
            <p className="font-mono text-xs text-fg-muted">
              {unit.camera.resolution[0]}×{unit.camera.resolution[1]} px · f{" "}
              {unit.camera.params.intrinsics.fx.toFixed(1)} px{unit.camera.optics ? " · physical optics" : ""}
              {unit.laser ? " · with its laser" : ""}
            </p>
          )}
          <p className="text-xs text-fg-muted">
            A camera, or a head (a camera with its line laser). To design one, use the Optics workspace and save it to
            the library.
          </p>
          <Button size="sm" variant="ghost" onClick={onOptics}>
            Design optics…
          </Button>
        </>
      );
      break;
    }
    case "rig": {
      const setRig = (patch: Partial<CellDraft["rig"]>) => set({ rig: { ...draft.rig, ...patch } });
      const kept =
        design !== undefined &&
        draft.rig.positions.every((p, i) => p.id === layoutCameraId(draft.rig.id, i) && p.camera === null);
      body = (
        <>
          <SegmentedControl
            value={draft.rig.design === null ? "single" : "design"}
            options={[
              { value: "single", label: "One camera" },
              { value: "design", label: "Rig design" },
            ]}
            aria-label="Rig"
            onValueChange={(v) => {
              if (v === "single") setRig({ design: null, positions: [] });
              else if (v === "design" && draft.rig.design === null) {
                const first = project ? itemsOf(project, "rig")[0] : undefined;
                setRig({
                  design: first?.[0] ?? null,
                  positions: defaultPositions(draft.rig.id, first?.[1].rig?.layout.count ?? 0),
                });
              }
            }}
          />
          <Id
            label="Rig id"
            value={draft.rig.id}
            onChange={(id) => setRig({ id, positions: renamePositions(draft.rig.positions, draft.rig.id, id) })}
          />
          {draft.rig.design === null ? (
            <Id label="Camera id" value={draft.rig.cameraId} onChange={(cameraId) => setRig({ cameraId })} />
          ) : (
            <>
              <ItemSelect
                label="Rig design"
                project={project}
                kinds={["rig"]}
                value={draft.rig.design}
                onChange={(ref) => {
                  const count = ref ? (library.get(ref)?.rig?.layout.count ?? 0) : 0;
                  setRig({ design: ref, positions: defaultPositions(draft.rig.id, count) });
                }}
              />
              {design && (
                <p className="font-mono text-xs text-fg-muted">
                  {design.count} camera{design.count === 1 ? "" : "s"} · ρ {(design.radius_m * 1000).toFixed(1)} mm ·{" "}
                  {design.lasers ? "lasers · " : ""}working distance {(design.working_distance_m * 1000).toFixed(0)} mm
                </p>
              )}
              <div className="space-y-2" data-testid="rig-positions">
                {draft.rig.positions.map((p, i) => (
                  // A design's positions are fixed: the index is the position.
                  // eslint-disable-next-line react/no-array-index-key
                  <div key={i} className="grid grid-cols-2 gap-2">
                    <Id
                      label={`Camera ${i + 1} id`}
                      value={p.id}
                      onChange={(id) =>
                        setRig({ positions: draft.rig.positions.map((q, j) => (j === i ? { ...q, id } : q)) })
                      }
                    />
                    <ItemSelect
                      label={`Camera ${i + 1}`}
                      project={project}
                      kinds={["camera"]}
                      value={p.camera}
                      unset={unit ? "The optics" : "As designed"}
                      onChange={(camera) =>
                        setRig({ positions: draft.rig.positions.map((q, j) => (j === i ? { ...q, camera } : q)) })
                      }
                    />
                  </div>
                ))}
              </div>
              <p className="text-xs text-fg-muted">
                {kept
                  ? "The rig keeps its layout: it can be laid out again from the Inspector."
                  : "Renamed or individually chosen cameras make a plain rig: the layout is not kept with it."}
              </p>
            </>
          )}
        </>
      );
      break;
    }
    case "mounting": {
      body = (
        <>
          <SegmentedControl
            value={draft.mounting}
            options={[
              { value: "eye_in_hand", label: "Eye-in-hand" },
              { value: "eye_to_hand", label: "Eye-to-hand" },
            ]}
            aria-label="Mounting"
            onValueChange={(v) => (v === "eye_in_hand" || v === "eye_to_hand") && set({ mounting: v })}
          />
          <p className="text-xs text-fg-muted">
            {eyeInHand
              ? "The rig rides on the robot's tool flange; the target stands in the cell."
              : "The rig stands in the cell on a fixed stand; the robot holds the target on its flange."}
          </p>
          <ItemSelect
            label={eyeInHand ? "Mount (rig on the flange)" : "Mount (target on the flange)"}
            project={project}
            kinds={["mount"]}
            value={draft.mount}
            unset="No mount"
            onChange={(mount) => set({ mount })}
          />
          {draft.mount === null && (
            <Pose
              label="On the flange"
              description="Where the device sits in the tool flange's frame."
              value={draft.flange}
              onChange={(flange) => set({ flange })}
            />
          )}
          {!eyeInHand && (
            <>
              <Id label="Stand id" value={draft.stand.id} onChange={(id) => set({ stand: { ...draft.stand, id } })} />
              <Pose
                label="Stand on the world"
                value={draft.stand.pose}
                onChange={(pose) => set({ stand: { ...draft.stand, pose } })}
              />
              <Pose label="Rig on the stand" value={draft.rigOnStand} onChange={(rigOnStand) => set({ rigOnStand })} />
            </>
          )}
        </>
      );
      break;
    }
    case "target": {
      const template = unit?.camera ?? design?.camera;
      const coverage = checks?.extent && template ? targetCoverage(checks.extent, template, distance) : null;
      body = (
        <>
          <ItemSelect
            label="Target"
            project={project}
            kinds={["target"]}
            value={draft.target.item}
            onChange={(item) => set({ target: { ...draft.target, item } })}
          />
          <Id label="Target id" value={draft.target.id} onChange={(id) => set({ target: { ...draft.target, id } })} />
          {eyeInHand && (
            <Pose
              label="Target on the world"
              value={draft.target.pose}
              onChange={(pose) => set({ target: { ...draft.target, pose } })}
            />
          )}
          <Field label="Working distance" description="Where the target is judged, from the rig along its axis.">
            <NumberInput
              aria-label="Working distance"
              value={distance * 1000}
              unit="mm"
              precision={0}
              min={1}
              step="any"
              onValueChange={(v) => set({ workingDistance: v / 1000 })}
            />
          </Field>
          {coverage && checks?.extent && (
            <div className="space-y-1" data-testid="target-coverage">
              <p className="text-xs">
                {(checks.extent[0] * 1000).toFixed(0)} × {(checks.extent[1] * 1000).toFixed(0)} mm: it spans{" "}
                <span className="font-mono tabular-nums">{(coverage.fraction * 100).toFixed(0)} %</span> of the image
                width at {(distance * 1000).toFixed(0)} mm (the image is {(coverage.fov_m * 1000).toFixed(0)} mm wide
                there).
              </p>
              {coverage.verdict !== "ok" && (
                <Callout tone="warning" title={coverage.verdict === "small" ? "A small target" : "A large target"}>
                  {coverage.verdict === "small"
                    ? "Under a fifth of the image width: the corners crowd the middle, and the distortion at the edges is barely observed. A larger target, or a shorter distance."
                    : "Over nine tenths of the image width: it leaves the image as soon as it tilts or moves. A smaller target, or a longer distance."}
                </Callout>
              )}
            </div>
          )}
          <Views checks={checks} />
        </>
      );
      break;
    }
    case "robot": {
      const setRobot = (patch: Partial<CellDraft["robot"]>) => set({ robot: { ...draft.robot, ...patch } });
      const runAim = () => {
        if (!cell || !sceneRobots || !robot) return;
        const answer = aimRobot(cell.scene, sceneRobots, {
          robot: draft.robot.id,
          baseLink: robot.manifest.base_link,
          tcpLink: robot.manifest.tcp_link,
          rig: draft.rig.id,
          target: draft.target.id,
          eyeInHand,
          distance,
          seed: draft.robot.q,
        });
        if (typeof answer === "string") setAim(answer);
        else if (!answer.converged)
          setAim(
            `Out of reach: no joints put the target ${(distance * 1000).toFixed(0)} mm in front of the rig from here.`,
          );
        else {
          setAim(null);
          setRobot({ q: answer.q });
        }
      };
      body = (
        <>
          <Field label="Robot">
            <Select
              aria-label="Robot"
              value={draft.robot.manifest ?? ""}
              options={robots.map((r) => ({ value: r.manifest, label: r.local ? `${r.name} (local only)` : r.name }))}
              onValueChange={(m) => {
                const lib = robots.find((r) => r.manifest === m);
                if (lib) setRobot({ manifest: m, id: lib.id, q: [] });
              }}
            />
          </Field>
          <Id label="Robot id" value={draft.robot.id} onChange={(id) => setRobot({ id })} />
          <Pose label="Base on the world" value={draft.robot.base} onChange={(base) => setRobot({ base })} />
          {robot ? (
            <Field label="Start pose" as="group" description="The joints the cell starts at (degrees).">
              <div className="grid grid-cols-2 gap-x-3 gap-y-1.5" data-testid="start-joints">
                {robot.manifest.joints.map((j, i) => (
                  <label key={j.name} className="flex min-w-0 items-center gap-2">
                    <span className="w-28 truncate font-mono text-[11px] text-fg-muted" title={j.name}>
                      {j.name}
                    </span>
                    <NumberInput
                      aria-label={j.name}
                      value={(draft.robot.q[i] ?? 0) * DEG}
                      unit="°"
                      precision={1}
                      min={j.lower * DEG}
                      max={j.upper * DEG}
                      step="any"
                      onValueChange={(v) => setRobot({ q: draft.robot.q.map((q, k) => (k === i ? v / DEG : q)) })}
                    />
                  </label>
                ))}
              </div>
            </Field>
          ) : (
            <p className="text-xs text-fg-muted">Reading the robot…</p>
          )}
          <div className="space-y-1">
            <Button
              size="sm"
              variant="ghost"
              icon={<Crosshair className="size-4" />}
              disabled={!cell || !robot}
              onClick={runAim}
              title={
                eyeInHand
                  ? "Joints that hold the rig over the target, at the working distance"
                  : "Joints that hold the target in front of the rig, at the working distance"
              }
            >
              Aim at the target
            </Button>
            {aim && <p className="text-xs text-warn">{aim}</p>}
          </div>
          <Views checks={checks} />
        </>
      );
      break;
    }
    case "review": {
      body = (
        <>
          <Field label="Description">
            <Textarea
              aria-label="Description"
              value={draft.description}
              minRows={2}
              onValueChange={(description) => set({ description })}
            />
          </Field>
          {cell && (
            <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs" data-testid="cell-summary">
              <dt className="text-fg-muted">mounting</dt>
              <dd>{eyeInHand ? "eye-in-hand" : "eye-to-hand"}</dd>
              <dt className="text-fg-muted">cameras</dt>
              <dd className="font-mono">{(cell.scene.cameras ?? []).map((c) => c.id).join(", ")}</dd>
              <dt className="text-fg-muted">lasers</dt>
              <dd className="font-mono">{(cell.scene.lasers ?? []).map((l) => l.id).join(", ") || "none"}</dd>
              <dt className="text-fg-muted">from the library</dt>
              <dd className="font-mono">{cell.used.join(", ")}</dd>
            </dl>
          )}
          {project?.write && cell && (
            <p className="text-xs text-fg-muted">
              The bank and built-in items the cell uses are copied into {project.manifest.name}'s library.
            </p>
          )}
          <ProblemList problems={problems} />
          <Views checks={checks} />
        </>
      );
      break;
    }
  }

  const index = PAGES.indexOf(page);
  const ready = cell !== null && sceneRobots !== null && checks !== null && checks.problem === null;
  const right = (
    <div className="flex h-full min-h-0 flex-col" data-testid="cell-builder">
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-line px-3">
        <span className="flex-1 text-xs font-medium">
          <span className="font-mono text-fg-muted">{String(index + 1).padStart(2, "0")}</span> {TITLES[page]}
        </span>
        <Button size="sm" variant="ghost" icon={<X className="size-4" />} onClick={cancel}>
          Cancel
        </Button>
      </div>
      <div className="min-h-0 flex-1 space-y-3 overflow-auto p-3">
        {body}
        {page !== "review" && <ProblemList problems={stepProblems(page)} />}
      </div>
      <div className="flex shrink-0 items-center gap-2 border-t border-line p-2">
        <Button
          size="sm"
          variant="ghost"
          icon={<ChevronLeft className="size-4" />}
          disabled={index === 0}
          onClick={() => setPage(PAGES[index - 1] ?? page)}
        >
          Back
        </Button>
        <span className="flex-1" />
        {page !== "review" && (
          <Button
            size="sm"
            variant="ghost"
            icon={<ChevronRight className="size-4" />}
            onClick={() => setPage(PAGES[index + 1] ?? page)}
          >
            Next
          </Button>
        )}
        <Button
          size="sm"
          icon={<Hammer className="size-4" />}
          disabled={!ready}
          loading={busy}
          onClick={() => void build()}
          title={ready ? "Make this the open cell (one undoable edit)" : "Complete the steps first"}
        >
          Build cell
        </Button>
      </div>
    </div>
  );
  return { active: true, start, left, right };
}
