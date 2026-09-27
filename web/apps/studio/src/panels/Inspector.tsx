import type { Iso3Schema } from "@etendue/wasm";
import { type FrameTreeRuntime, quaternionFromRpy, rpyFromQuaternion } from "@vitavision/three";
import { Badge, Callout, Panel, ReadoutStrip, Section, Table } from "@vitavision/ui";
import { PoseInput, type RotationView } from "@vitavision/ui-next";
import type { Playhead } from "@vitavision/workbench";
import type { ReactNode } from "react";

import type { Baked, KernelFailure } from "../kernel/etendue";
import { useThrottledPlayhead } from "../kernel/useThrottled";

const DEG = 180 / Math.PI;
const mm = (m: number) => `${(m * 1000).toFixed(1)} mm`;
const deg = (rad: number) => `${(rad * DEG).toFixed(2)}°`;

/** Roll, pitch, yaw in the URDF convention — the one `@vitavision/three` defines. */
const URDF_RPY: RotationView = { toEuler: rpyFromQuaternion, fromEuler: quaternionFromRpy };

/** A pose as translation (mm) and roll/pitch/yaw (°). */
function PoseReadout({ pose, label }: { pose: Iso3Schema; label: string }) {
  const [qx = 0, qy = 0, qz = 0, qw = 1] = pose.rotation;
  const [tx = 0, ty = 0, tz = 0] = pose.translation;
  return (
    <PoseInput
      readOnly
      aria-label={label}
      value={{ rotation: [qx, qy, qz, qw], translation: [tx, ty, tz] }}
      rotationView={URDF_RPY}
      translationUnit="mm"
    />
  );
}

type Entity = { id: string; parent: string; parent_se3_self: Iso3Schema };

function entityOf(current: Baked, id: string): { kind: string; entity: Entity } | null {
  const s = current.loaded.scene;
  for (const [kind, list] of [
    ["robot", s.robots],
    ["frame", s.frames],
    ["rig", s.rigs],
    ["camera", s.cameras],
    ["laser", s.lasers],
    ["light", s.lights],
    ["target", s.targets],
    ["part", s.parts],
  ] as const) {
    const e = (list as Entity[] | undefined)?.find((x) => x.id === id);
    if (e) return { kind, entity: e };
  }
  return null;
}

function Details({ current, id, k }: { current: Baked; id: string; k: number }): ReactNode {
  const s = current.loaded.scene;
  const camera = s.cameras?.find((c) => c.id === id);
  if (camera) {
    const { params } = camera;
    const intr = params.intrinsics as Record<string, unknown>;
    const dist = params.distortion as Record<string, unknown>;
    const num = (v: unknown) => (typeof v === "number" ? v.toFixed(v % 1 === 0 ? 0 : 3) : String(v));
    return (
      <Section title="Camera">
        <ReadoutStrip items={[{ label: "image", value: `${camera.resolution[0]} × ${camera.resolution[1]} px` }]} />
        <ReadoutStrip
          items={Object.entries(intr)
            .filter(([key]) => key !== "type")
            .map(([key, v]) => ({ label: key, value: num(v) }))}
        />
        <ReadoutStrip
          items={[
            { label: "distortion", value: String(dist.type) },
            ...Object.entries(dist)
              .filter(([key]) => key !== "type")
              .map(([key, v]) => ({ label: key, value: num(v) })),
          ]}
        />
        <ReadoutStrip items={[{ label: "sensor", value: String((params.sensor as { type: string }).type) }]} />
      </Section>
    );
  }
  const laser = s.lasers?.find((l) => l.id === id);
  if (laser) {
    return (
      <Section title="Laser">
        <ReadoutStrip
          items={[
            { label: "fan", value: deg(2 * laser.fan_half_angle) },
            { label: "reach", value: mm(laser.fan_length) },
            { label: "λ", value: `${laser.wavelength_nm} nm` },
            { label: "waist", value: `${(laser.beam_waist_m * 1e6).toFixed(0)} µm` },
          ]}
        />
      </Section>
    );
  }
  const target = s.targets?.find((t) => t.id === id);
  if (target) {
    const extent = current.session.targetExtent(target.id);
    const g = target.geometry;
    const what =
      g.type === "board"
        ? Object.entries(g.board)
            .map(([key, v]) => `${key} ${String(v)}`)
            .join(", ")
        : `${mm(g.width)} × ${mm(g.height)}`;
    return (
      <Section title="Target">
        <p className="font-mono text-xs text-fg-muted">{what}</p>
        {extent && <ReadoutStrip items={[{ label: "extent", value: `${mm(extent[0])} × ${mm(extent[1])}` }]} />}
      </Section>
    );
  }
  const light = s.lights?.find((l) => l.id === id);
  if (light) {
    return (
      <Section title="Light">
        <ReadoutStrip
          items={[
            { label: "shape", value: light.shape.type },
            { label: "power", value: `${light.power_w} W` },
            { label: "rgb", value: light.color.map((c) => c.toFixed(2)).join(" ") },
          ]}
        />
      </Section>
    );
  }
  const r = current.baked.robots.findIndex((x) => x.id === id);
  if (r >= 0) {
    const loaded = current.loaded.robots.find((x) => x.id === id)!;
    const q = current.baked.samples[k]?.joint_positions[r] ?? [];
    const rows = loaded.manifest.joints.map((j, i) => ({ ...j, q: q[i] ?? 0 }));
    return (
      <Section title={loaded.manifest.name}>
        <ReadoutStrip items={[{ label: "base", value: loaded.manifest.base_link }, { label: "tcp", value: loaded.manifest.tcp_link }]} />
        <Table
          rows={rows}
          rowKey={(row) => row.name}
          columns={[
            { key: "name", header: "joint", cell: (row) => row.name },
            { key: "q", header: "q", numeric: true, cell: (row) => `${(row.q * DEG).toFixed(1)}°` },
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
  return null;
}

/** The selected thing's parameters and poses, or a summary of the scene. */
export function Inspector({
  current,
  runtime,
  selected,
  failure,
  playhead,
}: {
  current: Baked;
  runtime: FrameTreeRuntime | null;
  selected: string | null;
  failure: KernelFailure | null;
  playhead: Playhead;
}) {
  const k = useThrottledPlayhead(playhead, 100);
  const { scene } = current.loaded;
  const { baked } = current;
  const found = selected !== null ? entityOf(current, selected) : null;
  const frame = selected === null ? null : found?.kind === "robot" ? `${selected}/${current.loaded.robots.find((r) => r.id === selected)?.manifest.base_link}` : selected;
  const world = frame !== null && runtime ? runtime.pose(frame, k) : undefined;
  const sample = baked.samples[k];

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
              <span className="font-mono">{selected}</span>
              <Badge>{found?.kind ?? (selected.includes("/") ? "link" : "frame")}</Badge>
            </span>
          }
        >
          <div className="space-y-3">
            {found && (
              <Section title={`In ${found.entity.parent}`}>
                <PoseReadout pose={found.entity.parent_se3_self} label={`Pose in ${found.entity.parent}`} />
              </Section>
            )}
            {world && (
              <div data-testid="world-pose">
                <Section title={`In world at t = ${(sample?.t ?? 0).toFixed(2)} s`}>
                  <PoseReadout pose={world} label="Pose in world" />
                </Section>
              </div>
            )}
            <Details current={current} id={selected} k={k} />
          </div>
        </Panel>
      )}
    </div>
  );
}
