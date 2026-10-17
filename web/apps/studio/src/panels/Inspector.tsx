import type { CameraSpec, Iso3Schema, LaserSpec, LightSpec, SceneSpec } from "@etendue/wasm";
import { type FrameTreeRuntime, quaternionFromRpy, rpyFromQuaternion } from "@vitavision/three";
import {
  Badge,
  Callout,
  Input,
  NumberInput,
  Panel,
  PoseInput,
  ReadoutStrip,
  type RotationView,
  Section,
  Select,
  Table,
  VectorInput,
} from "@vitavision/ui";
import type { Playhead } from "@vitavision/workbench";
import { type ReactNode, useState } from "react";

import type { Baked, KernelFailure } from "../kernel/etendue";
import { useThrottledPlayhead } from "../kernel/useThrottled";
import { entityOf, listOf, renameEntity, updateEntity } from "../scene/edit";
import { RigBuilder } from "./RigBuilder";
import { TargetEditor } from "./TargetEditor";
import type { Doc } from "../state/studio";

const DEG = 180 / Math.PI;

/** Roll, pitch, yaw in the URDF convention — the one `@vitavision/three` defines. */
const URDF_RPY: RotationView = { toEuler: rpyFromQuaternion, fromEuler: quaternionFromRpy };

/** Apply an edit of the document; whether it was accepted. */
export type OnEdit = (label: string, change: (doc: Doc) => Doc) => boolean;

/** A pose as translation (mm) and roll/pitch/yaw (°), editable when `onChange` is given. */
function Pose({
  pose,
  label,
  onChange,
}: {
  pose: Iso3Schema;
  label: string;
  onChange?: ((pose: Iso3Schema) => void) | undefined;
}) {
  const [qx, qy, qz, qw] = pose.rotation;
  const [tx, ty, tz] = pose.translation;
  return (
    <PoseInput
      readOnly={onChange === undefined}
      aria-label={label}
      value={{ rotation: [qx, qy, qz, qw], translation: [tx, ty, tz] }}
      onValueChange={
        onChange &&
        ((v) =>
          onChange({
            rotation: [v.rotation[0], v.rotation[1], v.rotation[2], v.rotation[3]],
            translation: [v.translation[0], v.translation[1], v.translation[2]],
          }))
      }
      rotationView={URDF_RPY}
      translationUnit="mm"
    />
  );
}

/** A labelled row of the parameter grid. */
function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <span className="self-center truncate text-xs text-fg-muted" title={label}>
        {label}
      </span>
      <div className="min-w-0">{children}</div>
    </>
  );
}

function Grid({ children }: { children: ReactNode }) {
  return <div className="grid grid-cols-[minmax(4.5rem,auto)_1fr] gap-x-3 gap-y-1.5">{children}</div>;
}

/**
 * A number shown as `value · scale` in `unit` (metres shown as millimetres, say), reported back
 * divided by `scale`.
 */
function Num({
  label,
  value,
  onChange,
  unit,
  scale = 1,
  precision,
  min,
  max,
  step,
  integer = false,
}: {
  label: string;
  value: number;
  onChange: (value: number) => void;
  unit?: string | undefined;
  scale?: number | undefined;
  precision?: number | undefined;
  min?: number | undefined;
  max?: number | undefined;
  step?: number | undefined;
  integer?: boolean | undefined;
}) {
  return (
    <Row label={label}>
      <NumberInput
        aria-label={label}
        value={value * scale}
        precision={integer ? 0 : precision}
        unit={unit}
        min={min}
        max={max}
        step={step ?? (integer ? 1 : "any")}
        onValueChange={(v) => onChange((integer ? Math.round(v) : v) / scale)}
      />
    </Row>
  );
}

/** The entity's id, renamed on Enter or when the field is left. */
function IdField({ id, onRename }: { id: string; onRename: ((to: string) => boolean) | null }) {
  const [draft, setDraft] = useState<{ id: string; text: string } | null>(null);
  const text = draft?.id === id ? draft.text : id;
  if (onRename === null) return <span className="font-mono">{id}</span>;
  const commit = () => {
    if (text !== id && !onRename(text)) return;
    setDraft(null);
  };
  return (
    <Input
      aria-label="Id"
      value={text}
      className="h-7 font-mono text-sm"
      onChange={(e) => setDraft({ id, text: e.target.value })}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") commit();
        if (e.key === "Escape") setDraft(null);
      }}
    />
  );
}

/** Numeric fields of a tagged parameter object (`{type, …numbers}`), editable. */
function NumberFields({
  value,
  onChange,
  prefix,
}: {
  value: Record<string, unknown>;
  onChange: (value: Record<string, unknown>) => void;
  prefix: string;
}) {
  return (
    <>
      {Object.entries(value)
        .filter(([, v]) => typeof v === "number")
        .map(([key, v]) => (
          <Num
            key={key}
            label={`${prefix}${key}`}
            value={v as number}
            integer={key === "iters"}
            onChange={(n) => onChange({ ...value, [key]: n })}
          />
        ))}
    </>
  );
}

function CameraDetails({ camera, set }: { camera: CameraSpec; set: (c: CameraSpec) => void }) {
  const { params } = camera;
  const intr = params.intrinsics as unknown as Record<string, unknown>;
  const dist = params.distortion as unknown as Record<string, unknown>;
  const setParams = (patch: Partial<CameraSpec["params"]>) => set({ ...camera, params: { ...params, ...patch } });
  return (
    <Section title="Camera">
      <Grid>
        <Row label="image">
          <VectorInput
            aria-label="Image size"
            value={camera.resolution}
            labels={["w", "h"]}
            unit="px"
            precision={0}
            step={1}
            min={1}
            onValueChange={([w, h]) => set({ ...camera, resolution: [Math.round(w!), Math.round(h!)] })}
          />
        </Row>
        <NumberFields
          prefix=""
          value={intr}
          onChange={(v) => setParams({ intrinsics: v as unknown as CameraSpec["params"]["intrinsics"] })}
        />
        <Row label="distortion">
          <Select
            aria-label="Distortion model"
            value={String(dist.type)}
            options={[
              { value: "none", label: "none" },
              { value: "brown_conrady5", label: "Brown–Conrady (5)" },
            ]}
            onValueChange={(type) =>
              type !== "" &&
              type !== dist.type &&
              setParams({
                distortion: (type === "none"
                  ? { type: "none" }
                  : {
                      type,
                      k1: 0,
                      k2: 0,
                      k3: 0,
                      p1: 0,
                      p2: 0,
                      iters: 8,
                    }) as unknown as CameraSpec["params"]["distortion"],
              })
            }
          />
        </Row>
        <NumberFields
          prefix=""
          value={dist}
          onChange={(v) => setParams({ distortion: v as unknown as CameraSpec["params"]["distortion"] })}
        />
        <Row label="sensor">
          <span className="font-mono text-xs">{(params.sensor as { type: string }).type}</span>
        </Row>
      </Grid>
    </Section>
  );
}

function LaserDetails({ laser, set }: { laser: LaserSpec; set: (l: LaserSpec) => void }) {
  return (
    <Section title="Laser">
      <Grid>
        <Num
          label="fan"
          value={2 * laser.fan_half_angle}
          scale={DEG}
          unit="°"
          precision={1}
          min={0.1}
          max={179}
          onChange={(v) => set({ ...laser, fan_half_angle: v / 2 })}
        />
        <Num
          label="reach"
          value={laser.fan_length}
          scale={1000}
          unit="mm"
          precision={0}
          min={1}
          onChange={(fan_length) => set({ ...laser, fan_length })}
        />
        <Num
          label="λ"
          value={laser.wavelength_nm}
          unit="nm"
          precision={0}
          min={200}
          max={2000}
          onChange={(wavelength_nm) => set({ ...laser, wavelength_nm })}
        />
        <Num
          label="waist"
          value={laser.beam_waist_m}
          scale={1e6}
          unit="µm"
          precision={0}
          min={1}
          onChange={(beam_waist_m) => set({ ...laser, beam_waist_m })}
        />
      </Grid>
    </Section>
  );
}

function LightDetails({ light, set }: { light: LightSpec; set: (l: LightSpec) => void }) {
  return (
    <Section title="Light">
      <Grid>
        <Row label="shape">
          <span className="font-mono text-xs">{light.shape.type}</span>
        </Row>
        <Num
          label="power"
          value={light.power_w}
          unit="W"
          precision={1}
          min={0}
          onChange={(power_w) => set({ ...light, power_w })}
        />
        <Row label="colour">
          <VectorInput
            aria-label="Linear RGB"
            value={light.color}
            labels={["r", "g", "b"]}
            precision={2}
            min={0}
            max={1}
            step={0.05}
            onValueChange={([r, g, b]) => set({ ...light, color: [r!, g!, b!] })}
          />
        </Row>
      </Grid>
    </Section>
  );
}

/** Frames an entity may hang on: every baked frame but itself and what hangs on it. */
function parentOptions(current: Baked, id: string): { value: string; label: string }[] {
  const scene = current.loaded.scene;
  const below = new Set<string>([id]);
  const placed = [
    ...(scene.frames ?? []),
    ...(scene.robots ?? []),
    ...(scene.rigs ?? []),
    ...(scene.cameras ?? []),
    ...(scene.lasers ?? []),
    ...(scene.lights ?? []),
    ...(scene.targets ?? []),
    ...(scene.parts ?? []),
  ];
  for (let grew = true; grew;) {
    grew = false;
    for (const e of placed) {
      const parent = e.parent.includes("/") ? e.parent.slice(0, e.parent.indexOf("/")) : e.parent;
      if (!below.has(e.id) && below.has(parent)) {
        below.add(e.id);
        grew = true;
      }
    }
  }
  return current.baked.frames
    .filter((f) => !below.has(f) && !below.has(f.slice(0, f.indexOf("/"))))
    .map((f) => ({ value: f, label: f }));
}

function setRobot(scene: SceneSpec, id: string, change: (r: NonNullable<SceneSpec["robots"]>[number]) => typeof r) {
  return { ...scene, robots: (scene.robots ?? []).map((r) => (r.id === id ? change(r) : r)) };
}

function RobotDetails({ current, id, k, onEdit }: { current: Baked; id: string; k: number; onEdit: OnEdit | null }) {
  const r = current.baked.robots.findIndex((x) => x.id === id);
  const loaded = current.loaded.robots.find((x) => x.id === id)!;
  const spec = current.loaded.scene.robots?.find((x) => x.id === id);
  const q = current.baked.samples[k]?.joint_positions[r] ?? [];
  const initial = spec?.initial_q ?? loaded.manifest.joints.map(() => 0);
  const rows = loaded.manifest.joints.map((j, i) => ({ ...j, i, q: q[i] ?? 0 }));
  const setInitial = (i: number, v: number) =>
    onEdit?.(`set ${id} rest pose`, (d) => ({
      ...d,
      scene: setRobot(d.scene, id, (x) => ({ ...x, initial_q: initial.map((old, j) => (j === i ? v : old)) })),
    }));
  return (
    <Section title={loaded.manifest.name}>
      <ReadoutStrip
        items={[
          { label: "base", value: loaded.manifest.base_link },
          { label: "tcp", value: loaded.manifest.tcp_link },
        ]}
      />
      <Table
        rows={rows}
        rowKey={(row) => row.name}
        columns={[
          { key: "name", header: "joint", cell: (row) => row.name },
          { key: "q", header: "q", numeric: true, cell: (row) => `${(row.q * DEG).toFixed(1)}°` },
          {
            key: "rest",
            header: "rest",
            numeric: true,
            cell: (row) =>
              onEdit ? (
                <NumberInput
                  aria-label={`${row.name} rest position`}
                  className="h-6 w-20 text-xs"
                  value={(initial[row.i] ?? 0) * DEG}
                  precision={1}
                  unit="°"
                  min={row.lower * DEG}
                  max={row.upper * DEG}
                  onValueChange={(v) => setInitial(row.i, v / DEG)}
                />
              ) : (
                `${((initial[row.i] ?? 0) * DEG).toFixed(1)}°`
              ),
          },
          {
            key: "range",
            header: "limits",
            numeric: true,
            cell: (row) =>
              row.lower === -row.upper
                ? `±${(row.upper * DEG).toFixed(0)}°`
                : `${(row.lower * DEG).toFixed(0)}° … ${(row.upper * DEG).toFixed(0)}°`,
          },
        ]}
      />
    </Section>
  );
}

/** The kind of the selected thing, for its badge. */
function kindOf(current: Baked, id: string): string {
  const scene = current.loaded.scene;
  if (scene.robots?.some((r) => r.id === id)) return "robot";
  if (scene.parts?.some((p) => p.id === id)) return "part";
  const list = listOf(scene, id);
  if (list !== null) return list.slice(0, -1);
  return id.includes("/") ? "link" : "frame";
}

/**
 * The selected thing's parameters and poses, editable (each change is one undoable edit, baked
 * at once), or a summary of the scene.
 */
export function Inspector({
  current,
  runtime,
  selected,
  failure,
  playhead,
  onEdit,
  onRenamed,
}: {
  current: Baked;
  runtime: FrameTreeRuntime | null;
  selected: string | null;
  failure: KernelFailure | null;
  playhead: Playhead;
  /** Apply an edit; `null` for a read-only inspector. */
  onEdit: OnEdit | null;
  /** The selection was renamed. */
  onRenamed: (id: string) => void;
}) {
  const k = useThrottledPlayhead(playhead, 100);
  const { scene } = current.loaded;
  const { baked } = current;
  const entity = selected !== null ? entityOf(scene, selected) : null;
  const robotSpec = selected !== null ? scene.robots?.find((r) => r.id === selected) : undefined;
  const placed = entity ?? robotSpec ?? scene.parts?.find((p) => p.id === selected) ?? null;
  const frame =
    selected === null
      ? null
      : robotSpec
        ? `${selected}/${current.loaded.robots.find((r) => r.id === selected)?.manifest.base_link}`
        : selected;
  const world = frame !== null && runtime ? runtime.pose(frame, k) : undefined;
  const sample = baked.samples[k];

  const editEntity = <E,>(label: string) =>
    onEdit && entity && selected !== null
      ? (next: E) =>
          onEdit(label, (d) => ({ ...d, scene: updateEntity(d.scene, selected, () => next as typeof entity) }))
      : undefined;
  const move =
    onEdit && selected !== null && (entity || robotSpec)
      ? (pose: Iso3Schema) =>
          onEdit(`move ${selected}`, (d) => ({
            ...d,
            scene: entity
              ? updateEntity(d.scene, selected, (e) => ({ ...e, parent_se3_self: pose }))
              : setRobot(d.scene, selected, (r) => ({ ...r, parent_se3_self: pose })),
          }))
      : undefined;
  const rename =
    onEdit && entity && selected !== null
      ? (to: string) => {
          // An empty or taken id is refused here; the field keeps the text to fix.
          if (renameEntity(scene, selected, to) === null) return false;
          const ok = onEdit(`rename ${selected}`, (d) => ({
            ...d,
            scene: renameEntity(d.scene, selected, to) ?? d.scene,
          }));
          if (ok) onRenamed(to);
          return ok;
        }
      : null;

  let details: ReactNode = null;
  if (selected !== null) {
    const camera = scene.cameras?.find((c) => c.id === selected);
    const laser = scene.lasers?.find((l) => l.id === selected);
    const target = scene.targets?.find((t) => t.id === selected);
    const light = scene.lights?.find((l) => l.id === selected);
    const readOnly = () => {};
    if (camera)
      details = <CameraDetails camera={camera} set={editEntity<CameraSpec>(`edit ${selected}`) ?? readOnly} />;
    else if (laser)
      details = <LaserDetails laser={laser} set={editEntity<LaserSpec>(`edit ${selected}`) ?? readOnly} />;
    else if (target)
      details = (
        <TargetEditor
          target={target}
          extent={current.session.targetExtent(target.id)}
          set={
            onEdit &&
            ((next, field) =>
              onEdit(`set ${selected} ${field}`, (d) => ({
                ...d,
                scene: updateEntity(d.scene, selected, () => next),
              })))
          }
        />
      );
    else if (light)
      details = <LightDetails light={light} set={editEntity<LightSpec>(`edit ${selected}`) ?? readOnly} />;
    else if (robotSpec) details = <RobotDetails current={current} id={selected} k={k} onEdit={onEdit} />;
    else if (onEdit && scene.rigs?.some((r) => r.id === selected))
      details = <RigBuilder scene={scene} rig={selected} onEdit={onEdit} />;
  }

  return (
    <div className="space-y-3 p-3" data-testid="inspector">
      {failure && (
        <Callout tone="error" title={failure.title}>
          <p>{failure.message}</p>
          {failure.issues.length > 0 && (
            <ul className="mt-1 list-disc pl-4 font-mono text-xs">
              {failure.issues.map((i) => (
                <li key={i.path + i.message}>
                  {i.path}: {i.message}
                </li>
              ))}
            </ul>
          )}
        </Callout>
      )}
      {selected === null ? (
        <Panel title="Scene">
          <div className="space-y-1">
            <ReadoutStrip
              items={[
                { label: "robots", value: scene.robots?.length ?? 0 },
                { label: "cameras", value: scene.cameras?.length ?? 0 },
                { label: "targets", value: scene.targets?.length ?? 0 },
              ]}
            />
            <ReadoutStrip
              items={[
                { label: "lasers", value: scene.lasers?.length ?? 0 },
                { label: "lights", value: scene.lights?.length ?? 0 },
              ]}
            />
            <ReadoutStrip
              items={[
                { label: "duration", value: `${((baked.samples.length - 1) * baked.dt).toFixed(2)} s` },
                { label: "captures", value: baked.samples.filter((s) => s.capture).length },
              ]}
            />
          </div>
          <p className="mt-2 text-xs text-fg-muted">Select a frame in the tree or the viewport.</p>
        </Panel>
      ) : (
        <Panel
          title={
            <span className="flex items-center gap-2">
              <IdField id={selected} onRename={rename} />
              <Badge>{kindOf(current, selected)}</Badge>
            </span>
          }
        >
          <div className="space-y-3">
            {placed && (
              <Section title="Placement">
                <Grid>
                  <Row label="parent">
                    {onEdit && (entity || robotSpec) ? (
                      <Select
                        aria-label="Parent frame"
                        value={placed.parent}
                        options={parentOptions(current, selected)}
                        onValueChange={(parent) =>
                          parent !== "" &&
                          onEdit(`attach ${selected}`, (d) => ({
                            ...d,
                            scene: entity
                              ? updateEntity(d.scene, selected, (e) => ({ ...e, parent }))
                              : setRobot(d.scene, selected, (r) => ({ ...r, parent })),
                          }))
                        }
                      />
                    ) : (
                      <span className="font-mono text-xs">{placed.parent}</span>
                    )}
                  </Row>
                </Grid>
                <div className="mt-2">
                  <Pose pose={placed.parent_se3_self} label={`Pose in ${placed.parent}`} onChange={move} />
                </div>
              </Section>
            )}
            {world && (
              <div data-testid="world-pose">
                <Section title={`In world at t = ${(sample?.t ?? 0).toFixed(2)} s`}>
                  <Pose pose={world} label="Pose in world" />
                </Section>
              </div>
            )}
            {details}
          </div>
        </Panel>
      )}
    </div>
  );
}
