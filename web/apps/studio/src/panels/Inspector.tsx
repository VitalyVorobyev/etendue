import type { CameraSpec, Iso3Schema, LaserSpec, LightSpec, SceneSpec } from "@etendue/wasm";
import { Badge, Callout, Input, NumberInput, ReadoutStrip, SegmentedControl, Select, Table } from "@vitavision/ui";
import type { Playhead } from "@vitavision/workbench";
import { type ReactNode, useState } from "react";

import type { Baked, KernelFailure } from "../kernel/etendue";
import type { PoseSource } from "../kernel/posed";
import { useThrottledPlayhead } from "../kernel/useThrottled";
import { entityOf, listOf, renameEntity, updateEntity } from "../scene/edit";
import type { Doc, Preview } from "../state/studio";
import { Fold, RowLabel, Rows } from "./controls/Fold";
import { type Live, NumberRow, PoseReadout, PoseRows, SliderRow, poseSummary } from "./controls/SliderRow";
import { RigBuilder } from "./RigBuilder";
import { TargetEditor } from "./TargetEditor";

const DEG = 180 / Math.PI;

/** Apply an edit of the document; whether it was accepted. */
export type OnEdit = (label: string, change: (doc: Doc) => Doc) => boolean;

/** Edits shown while they are made, recorded once (the studio's `preview`, `commit`, `cancel`). */
export interface LiveEdit {
  preview: (label: string, change: (doc: Doc) => Doc) => void;
  commit: () => void;
  cancel: () => void;
}

/** Translation slider spans (± mm) to choose from. */
const SPANS = [10, 100, 1000] as const;

/** The entity's id, renamed on Enter or when the field is left. */
function IdField({ id, onRename }: { id: string; onRename: ((to: string) => boolean) | null }) {
  const [draft, setDraft] = useState<{ id: string; text: string } | null>(null);
  const text = draft?.id === id ? draft.text : id;
  if (onRename === null) return <span className="truncate font-mono text-sm">{id}</span>;
  const commit = () => {
    if (text !== id && !onRename(text)) return;
    setDraft(null);
  };
  return (
    <Input
      aria-label="Id"
      value={text}
      className="h-7 min-w-0 flex-1 font-mono text-sm"
      onChange={(e) => setDraft({ id, text: e.target.value })}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") commit();
        if (e.key === "Escape") setDraft(null);
      }}
    />
  );
}

/** A live edit of one field of `entity`: `with(v)` is the entity with the new value. */
type FieldLive = <T>(with_: (v: T) => object) => Live<T> | null;

/** Numeric fields of a tagged parameter object (`{type, …numbers}`). */
function NumberFields({
  value,
  onChange,
}: {
  value: Record<string, unknown>;
  onChange: (key: string) => Live<number> | null;
}) {
  return (
    <>
      {Object.entries(value)
        .filter(([, v]) => typeof v === "number")
        .map(([key, v]) => (
          <NumberRow
            key={key}
            label={key}
            value={v as number}
            integer={key === "iters"}
            precision={key === "iters" ? 0 : undefined}
            live={onChange(key)}
          />
        ))}
    </>
  );
}

function CameraDetails({ camera, field }: { camera: CameraSpec; field: FieldLive }) {
  const { params } = camera;
  const intr = params.intrinsics as unknown as Record<string, unknown>;
  const dist = params.distortion as unknown as Record<string, unknown>;
  const withParams = (patch: Partial<CameraSpec["params"]>) => ({ ...camera, params: { ...params, ...patch } });
  const setDistortion = field<string>((type) =>
    withParams({
      distortion: (type === "none"
        ? { type: "none" }
        : { type, k1: 0, k2: 0, k3: 0, p1: 0, p2: 0, iters: 8 }) as unknown as CameraSpec["params"]["distortion"],
    }),
  );
  const fx = typeof intr.fx === "number" ? intr.fx : null;
  const sensor = (params.sensor as { type: string }).type;
  return (
    <>
      <Fold
        title="Image"
        summary={`${camera.resolution[0]}×${camera.resolution[1]} px${fx !== null ? ` · f ${fx.toFixed(0)} px` : ""}`}
      >
        <Rows>
          {(["width", "height"] as const).map((label, i) => (
            <NumberRow
              key={label}
              label={label}
              value={camera.resolution[i]!}
              unit="px"
              integer
              limits={[1, 65_536]}
              live={field<number>((v) => ({
                ...camera,
                resolution: camera.resolution.map((x, j) => (j === i ? v : x)) as [number, number],
              }))}
            />
          ))}
        </Rows>
      </Fold>
      <Fold title="Intrinsics" summary={typeof intr.type === "string" ? intr.type : ""} defaultOpen={false}>
        <Rows>
          <NumberFields
            value={intr}
            onChange={(key) =>
              field<number>((v) =>
                withParams({ intrinsics: { ...intr, [key]: v } as unknown as CameraSpec["params"]["intrinsics"] }),
              )
            }
          />
        </Rows>
      </Fold>
      <Fold title="Distortion" summary={String(dist.type)} defaultOpen={false}>
        <Rows>
          <RowLabel>model</RowLabel>
          <Select
            aria-label="Distortion model"
            value={String(dist.type)}
            options={[
              { value: "none", label: "none" },
              { value: "brown_conrady5", label: "Brown–Conrady (5)" },
            ]}
            onValueChange={(type) => {
              if (type === "" || type === dist.type) return;
              setDistortion?.preview(type);
              setDistortion?.commit();
            }}
          />
          <NumberFields
            value={dist}
            onChange={(key) =>
              field<number>((v) =>
                withParams({ distortion: { ...dist, [key]: v } as unknown as CameraSpec["params"]["distortion"] }),
              )
            }
          />
        </Rows>
        <p className="text-[11px] text-fg-muted">
          Sensor model: <span className="font-mono">{sensor}</span>
        </p>
      </Fold>
    </>
  );
}

function LaserDetails({ laser, field }: { laser: LaserSpec; field: FieldLive }) {
  return (
    <Fold
      title="Laser"
      summary={`${(2 * laser.fan_half_angle * DEG).toFixed(0)}° fan · ${laser.wavelength_nm.toFixed(0)} nm`}
    >
      <Rows>
        <SliderRow
          label="fan"
          value={2 * laser.fan_half_angle}
          scale={DEG}
          unit="°"
          range={[1, 170]}
          limits={[0.1, 179]}
          step={0.5}
          live={field<number>((v) => ({ ...laser, fan_half_angle: v / 2 }))}
        />
        <SliderRow
          label="reach"
          value={laser.fan_length}
          scale={1000}
          unit="mm"
          precision={0}
          range={[10, 3000]}
          limits={[1, 100_000]}
          step={10}
          live={field<number>((fan_length) => ({ ...laser, fan_length }))}
        />
        <NumberRow
          label="λ"
          value={laser.wavelength_nm}
          unit="nm"
          precision={0}
          limits={[200, 2000]}
          live={field<number>((wavelength_nm) => ({ ...laser, wavelength_nm }))}
        />
        <SliderRow
          label="waist"
          value={laser.beam_waist_m}
          scale={1e6}
          unit="µm"
          precision={0}
          range={[10, 2000]}
          limits={[1, 100_000]}
          step={5}
          live={field<number>((beam_waist_m) => ({ ...laser, beam_waist_m }))}
        />
      </Rows>
    </Fold>
  );
}

function LightDetails({ light, field }: { light: LightSpec; field: FieldLive }) {
  return (
    <Fold title="Light" summary={`${light.shape.type} · ${light.power_w.toFixed(0)} W`}>
      <Rows>
        <SliderRow
          label="power"
          value={light.power_w}
          unit="W"
          precision={1}
          range={[0, 500]}
          limits={[0, 1e6]}
          step={1}
          live={field<number>((power_w) => ({ ...light, power_w }))}
        />
        {(["r", "g", "b"] as const).map((c, i) => (
          <SliderRow
            key={c}
            label={c}
            value={light.color[i]!}
            precision={2}
            range={[0, 1]}
            limits={[0, 1]}
            step={0.01}
            live={field<number>((v) => ({
              ...light,
              color: light.color.map((x, j) => (j === i ? v : x)) as LightSpec["color"],
            }))}
          />
        ))}
      </Rows>
    </Fold>
  );
}

/** Frames an entity may hang on: every baked frame but itself and what hangs on it. */
function parentOptions(current: Baked, scene: SceneSpec, id: string): { value: string; label: string }[] {
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

function RobotDetails({
  current,
  scene,
  id,
  k,
  live,
}: {
  current: Baked;
  scene: SceneSpec;
  id: string;
  k: number;
  live: LiveEdit | null;
}) {
  const r = current.baked.robots.findIndex((x) => x.id === id);
  const loaded = current.loaded.robots.find((x) => x.id === id)!;
  const spec = scene.robots?.find((x) => x.id === id);
  const q = current.baked.samples[k]?.joint_positions[r] ?? [];
  const initial = spec?.initial_q ?? loaded.manifest.joints.map(() => 0);
  const rows = loaded.manifest.joints.map((j, i) => ({ ...j, i, q: q[i] ?? 0 }));
  const setInitial = (i: number, v: number) =>
    live?.preview(`set ${id} rest pose`, (d) => ({
      ...d,
      scene: setRobot(d.scene, id, (x) => ({ ...x, initial_q: initial.map((old, j) => (j === i ? v : old)) })),
    }));
  return (
    <Fold title={loaded.manifest.name} summary={`${loaded.manifest.joints.length} joints`}>
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
              live ? (
                <NumberInput
                  aria-label={`${row.name} rest position`}
                  className="h-6 w-20 text-xs tabular-nums"
                  value={(initial[row.i] ?? 0) * DEG}
                  precision={1}
                  unit="°"
                  min={row.lower * DEG}
                  max={row.upper * DEG}
                  onValueChange={(v) => setInitial(row.i, v / DEG)}
                  onBlur={live.commit}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") live.commit();
                    if (e.key === "Escape") live.cancel();
                  }}
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
    </Fold>
  );
}

/** The kind of the selected thing, for its badge. */
function kindOf(scene: SceneSpec, id: string): string {
  if (scene.robots?.some((r) => r.id === id)) return "robot";
  if (scene.parts?.some((p) => p.id === id)) return "part";
  const list = listOf(scene, id);
  if (list !== null) return list.slice(0, -1);
  return id.includes("/") ? "link" : "frame";
}

/**
 * The selected thing's placement and parameters. Sliders and fields show their edit in the
 * scene as it is made (a move is drawn over the bake, a parameter replaces the kernel's scene)
 * and record it as one undoable step when the drag ends or the field is left. With nothing
 * selected, a summary of the scene.
 */
export function Inspector({
  current,
  preview,
  poses,
  selected,
  failure,
  playhead,
  live,
  onEdit,
  onRenamed,
}: {
  current: Baked;
  /** The edit in progress: the inspector shows its document. */
  preview: Preview | null;
  /** World poses at the playhead, the edit in progress included; `null` before the first frame. */
  poses: PoseSource | null;
  selected: string | null;
  failure: KernelFailure | null;
  playhead: Playhead;
  /** Live edits; `null` for a read-only inspector. */
  live: LiveEdit | null;
  /** Discrete edits (a parent, a name, a rig layout). */
  onEdit: OnEdit | null;
  /** The selection was renamed. */
  onRenamed: (id: string) => void;
}) {
  const k = useThrottledPlayhead(playhead, 100);
  const [span, setSpan] = useState<(typeof SPANS)[number]>(100);
  const scene = preview?.doc.scene ?? current.loaded.scene;
  const committed = current.loaded.scene;
  const { baked } = current;
  const entity = selected !== null ? entityOf(scene, selected) : null;
  const robotSpec = selected !== null ? scene.robots?.find((r) => r.id === selected) : undefined;
  const placed = entity ?? robotSpec ?? scene.parts?.find((p) => p.id === selected) ?? null;
  const placedBefore =
    selected === null
      ? null
      : (entityOf(committed, selected) ??
        committed.robots?.find((r) => r.id === selected) ??
        committed.parts?.find((p) => p.id === selected) ??
        null);
  const frame =
    selected === null
      ? null
      : robotSpec
        ? `${selected}/${current.loaded.robots.find((r) => r.id === selected)?.manifest.base_link}`
        : selected;
  const world = frame !== null && poses ? poses.pose(frame, k) : undefined;
  const sample = baked.samples[k];

  const field: FieldLive = <T,>(with_: (v: T) => object): Live<T> | null =>
    live && entity && selected !== null
      ? {
          preview: (v: T) =>
            live.preview(`edit ${selected}`, (d) => ({
              ...d,
              scene: updateEntity(d.scene, selected, () => with_(v) as typeof entity),
            })),
          commit: live.commit,
          cancel: live.cancel,
        }
      : null;
  const move: Live<Iso3Schema> | null =
    live && selected !== null && (entity || robotSpec)
      ? {
          preview: (pose) =>
            live.preview(`move ${selected}`, (d) => ({
              ...d,
              scene: entity
                ? updateEntity(d.scene, selected, (e) => ({ ...e, parent_se3_self: pose }))
                : setRobot(d.scene, selected, (r) => ({ ...r, parent_se3_self: pose })),
            })),
          commit: live.commit,
          cancel: live.cancel,
        }
      : null;
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
    if (camera) details = <CameraDetails camera={camera} field={field} />;
    else if (laser) details = <LaserDetails laser={laser} field={field} />;
    else if (target)
      details = (
        <Fold title="Target" summary={target.geometry.type}>
          <TargetEditor
            target={target}
            extent={current.session.targetExtent(target.id)}
            set={
              onEdit &&
              ((next, f) =>
                onEdit(`set ${selected} ${f}`, (d) => ({
                  ...d,
                  scene: updateEntity(d.scene, selected, () => next),
                })))
            }
          />
        </Fold>
      );
    else if (light) details = <LightDetails light={light} field={field} />;
    else if (robotSpec) details = <RobotDetails current={current} scene={scene} id={selected} k={k} live={live} />;
    else if (onEdit && scene.rigs?.some((r) => r.id === selected))
      details = (
        <Fold title="Layout">
          <RigBuilder scene={scene} rig={selected} onEdit={onEdit} />
        </Fold>
      );
  }

  return (
    <div className="pb-3" data-testid="inspector">
      {failure && (
        <div className="p-3">
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
        </div>
      )}
      {selected === null ? (
        <div className="space-y-2 p-3">
          <h2 className="text-sm font-medium">Scene</h2>
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
          <p className="text-xs text-fg-muted">Select a frame in the tree or the viewport.</p>
        </div>
      ) : (
        <>
          <div className="flex items-center gap-2 px-3 py-2">
            <IdField id={selected} onRename={rename} />
            <Badge>{kindOf(scene, selected)}</Badge>
          </div>
          {placed && (
            <Fold title="Placement" summary={`in ${placed.parent}`} testId="placement">
              <Rows>
                <RowLabel>parent</RowLabel>
                {onEdit && (entity || robotSpec) ? (
                  <Select
                    aria-label="Parent frame"
                    value={placed.parent}
                    options={parentOptions(current, scene, selected)}
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
                <RowLabel>range</RowLabel>
                <SegmentedControl
                  aria-label="Translation slider range"
                  value={String(span)}
                  options={SPANS.map((s) => ({ value: String(s), label: `±${s} mm` }))}
                  onValueChange={(v) => {
                    const s = SPANS.find((x) => String(x) === v);
                    if (s) setSpan(s);
                  }}
                />
              </Rows>
              <PoseRows
                value={placed.parent_se3_self}
                centre={placedBefore?.parent_se3_self ?? placed.parent_se3_self}
                span={span}
                live={move}
                testId="pose"
              />
            </Fold>
          )}
          {world && (
            <Fold
              title="World"
              summary={`t = ${(sample?.t ?? 0).toFixed(2)} s · ${world.translation.map((x) => (x * 1000).toFixed(1)).join(", ")} mm`}
              defaultOpen={false}
              testId="world-pose"
            >
              <PoseReadout pose={world} testId="world-readout" />
              <p className="truncate font-mono text-[11px] text-fg-subtle" title={poseSummary(world)}>
                {frame}
              </p>
            </Fold>
          )}
          {details}
        </>
      )}
    </div>
  );
}
