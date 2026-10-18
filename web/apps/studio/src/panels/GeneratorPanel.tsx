import type { EtendueScene, GeneratedPoses, GeneratorSpec, SceneSpec } from "@etendue/wasm";
import { Button, Callout, Input, NumberInput, ReadoutStrip, Select, Switch } from "@vitavision/ui";
import { Check } from "lucide-react";
import { type ReactNode, useEffect, useMemo, useState } from "react";

import { describeFailure } from "../kernel/etendue";

const DEG = 180 / Math.PI;

/** A spec for `scene`: the sphere generator over its first robot, the rig (or camera) it carries, and its first target. */
export function defaultSpec(scene: SceneSpec): GeneratorSpec | null {
  const robot = scene.robots?.[0]?.id;
  const target = scene.targets?.[0]?.id;
  if (!robot || !target) return null;
  const onRobot = (parent: string) => parent.startsWith(`${robot}/`);
  const look =
    scene.rigs?.find((r) => onRobot(r.parent))?.id ??
    scene.cameras?.find((c) => onRobot(c.parent))?.id ??
    scene.rigs?.[0]?.id ??
    scene.cameras?.[0]?.id;
  if (!look) return null;
  return {
    type: "sphere",
    robot,
    look,
    target,
    radii_m: [0.45, 0.55],
    theta: { min_rad: 10 / DEG, max_rad: 35 / DEG, steps: 3 },
    phi: { min_rad: 0, max_rad: 2 * Math.PI, steps: 8 },
    roll_rad: [0],
    require: { min_visible_fraction: 1, margin_px: 2 },
    joint_margin_rad: 3 / DEG,
    min_sigma: 0.02,
    speed_scale: 0.5,
    return_home: false,
  };
}

/** `"450, 550"` → `[0.45, 0.55]` (values in `unit` per metre); `null` if a value does not parse. */
function parseList(text: string, scale: number): number[] | null {
  const parts = text
    .split(/[,\s]+/)
    .map((p) => p.trim())
    .filter((p) => p !== "");
  const values = parts.map(Number);
  return values.length > 0 && values.every(Number.isFinite) ? values.map((v) => v / scale) : null;
}

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <span className="self-center text-xs text-fg-muted">{label}</span>
      <div className="min-w-0">{children}</div>
    </>
  );
}

/** A list of numbers as text, committed on blur or Enter. */
function ListInput({
  label,
  values,
  scale,
  unit,
  onChange,
}: {
  label: string;
  values: number[];
  scale: number;
  unit: string;
  onChange: (v: number[]) => void;
}) {
  const shown = values.map((v) => +(v * scale).toFixed(3)).join(", ");
  const [draft, setDraft] = useState<{ of: string; text: string } | null>(null);
  const text = draft?.of === shown ? draft.text : shown;
  const commit = () => {
    const parsed = parseList(text, scale);
    if (parsed) onChange(parsed);
    setDraft(null);
  };
  return (
    <Row label={label}>
      <div className="flex items-center gap-1">
        <Input
          aria-label={label}
          value={text}
          className="h-7 font-mono text-xs"
          onChange={(e) => setDraft({ of: shown, text: e.target.value })}
          onBlur={commit}
          onKeyDown={(e) => e.key === "Enter" && commit()}
        />
        <span className="text-xs text-fg-muted">{unit}</span>
      </div>
    </Row>
  );
}

/** Angle range `min … max` in degrees and a step count. */
function Range({
  label,
  range,
  onChange,
}: {
  label: string;
  range: { min_rad: number; max_rad: number; steps: number };
  onChange: (r: { min_rad: number; max_rad: number; steps: number }) => void;
}) {
  return (
    <Row label={label}>
      <div className="grid grid-cols-[1fr_1fr_4.5rem] gap-1">
        <NumberInput
          aria-label={`${label} from`}
          value={range.min_rad * DEG}
          precision={1}
          unit="°"
          step="any"
          onValueChange={(v) => onChange({ ...range, min_rad: v / DEG })}
        />
        <NumberInput
          aria-label={`${label} to`}
          value={range.max_rad * DEG}
          precision={1}
          unit="°"
          step="any"
          onValueChange={(v) => onChange({ ...range, max_rad: v / DEG })}
        />
        <NumberInput
          aria-label={`${label} steps`}
          value={range.steps}
          precision={0}
          min={1}
          max={72}
          step={1}
          unit="×"
          onValueChange={(v) => onChange({ ...range, steps: Math.round(v) })}
        />
      </div>
    </Row>
  );
}

/**
 * The sphere pose generator: the sensor (a frame the robot's tool carries) on spheres around the
 * target, looking at its centre, at every radius, polar angle θ and azimuth φ. The candidates are
 * computed as the form changes and drawn in 3D; **Apply** makes the kept ones the scenario (one
 * undoable edit), each with its joint configuration stored.
 */
export function GeneratorPanel({
  session,
  scene,
  initial,
  onPreview,
  onApply,
}: {
  session: EtendueScene;
  scene: SceneSpec;
  /** The scenario's own generator, if it has one. */
  initial: GeneratorSpec | null;
  /** The candidates of the current form, or `null` while it is invalid. */
  onPreview: (generated: GeneratedPoses | null) => void;
  onApply: (generated: GeneratedPoses) => void;
}) {
  const [spec, setSpec] = useState<GeneratorSpec | null>(() => initial ?? defaultSpec(scene));
  const result = useMemo((): { ok: GeneratedPoses } | { error: string } | null => {
    if (!spec) return null;
    try {
      return { ok: session.generatePoses(spec) };
    } catch (e) {
      const f = describeFailure(e);
      return { error: f.message };
    }
  }, [session, spec]);
  useEffect(() => {
    onPreview(result && "ok" in result ? result.ok : null);
  }, [result, onPreview]);
  useEffect(() => () => onPreview(null), [onPreview]);

  if (!spec) {
    return (
      <Callout tone="info" title="Nothing to generate for">
        The generator needs a robot carrying a rig or a camera, and a target.
      </Callout>
    );
  }
  const set = (patch: Partial<GeneratorSpec>) => setSpec({ ...spec, ...patch });
  const generated = result && "ok" in result ? result.ok : null;
  const counts = generated
    ? generated.candidates.reduce<Record<string, number>>((acc, c) => {
        acc[c.status.status] = (acc[c.status.status] ?? 0) + 1;
        return acc;
      }, {})
    : {};
  const frames = [...(scene.rigs ?? []), ...(scene.cameras ?? [])].map((e) => ({ value: e.id, label: e.id }));
  return (
    <div className="space-y-3 p-3" data-testid="generator">
      <div className="grid grid-cols-[minmax(4.5rem,auto)_1fr] gap-x-3 gap-y-1.5">
        <Row label="robot">
          <Select
            aria-label="Robot"
            value={spec.robot}
            options={(scene.robots ?? []).map((r) => ({ value: r.id, label: r.id }))}
            onValueChange={(robot) => robot !== "" && set({ robot })}
          />
        </Row>
        <Row label="sensor">
          <Select
            aria-label="Sensor frame"
            value={spec.look}
            options={frames}
            onValueChange={(look) => look !== "" && set({ look })}
          />
        </Row>
        <Row label="target">
          <Select
            aria-label="Target"
            value={spec.target}
            options={(scene.targets ?? []).map((t) => ({ value: t.id, label: t.id }))}
            onValueChange={(target) => target !== "" && set({ target })}
          />
        </Row>
        <ListInput
          label="radii"
          values={spec.radii_m}
          scale={1000}
          unit="mm"
          onChange={(radii_m) => set({ radii_m })}
        />
        <Range label="θ" range={spec.theta} onChange={(theta) => set({ theta })} />
        <Range label="φ" range={spec.phi} onChange={(phi) => set({ phi })} />
        <ListInput
          label="roll"
          values={spec.roll_rad ?? [0]}
          scale={DEG}
          unit="°"
          onChange={(roll_rad) => set({ roll_rad })}
        />
        <Row label="seen">
          <NumberInput
            aria-label="Fraction of the target each camera sees"
            value={(spec.require?.min_visible_fraction ?? 1) * 100}
            precision={0}
            min={1}
            max={100}
            unit="%"
            onValueChange={(v) => set({ require: { ...spec.require, min_visible_fraction: v / 100 } })}
          />
        </Row>
        <Row label="speed">
          <NumberInput
            aria-label="Speed"
            value={(spec.speed_scale ?? 0.5) * 100}
            precision={0}
            min={1}
            max={100}
            unit="%"
            onValueChange={(v) => set({ speed_scale: v / 100 })}
          />
        </Row>
      </div>
      <Switch
        checked={spec.return_home ?? false}
        onCheckedChange={(return_home) => set({ return_home })}
        label="Return home at the end"
      />
      {result && "error" in result && (
        <Callout tone="error" title="Cannot generate">
          {result.error}
        </Callout>
      )}
      {generated && (
        <div className="space-y-2" data-testid="generator-summary">
          <ReadoutStrip
            items={[
              { label: "kept", value: counts.ok ?? 0 },
              { label: "unseen", value: counts.not_visible ?? 0 },
              { label: "unreachable", value: counts.unreachable ?? 0 },
              { label: "limits", value: (counts.near_limit ?? 0) + (counts.near_singular ?? 0) },
            ]}
          />
          <p className="text-xs text-fg-muted">
            {generated.candidates.length} candidates: green kept, amber unseen or infeasible, red unreachable.
          </p>
          <Button
            variant="primary"
            icon={<Check className="size-4" />}
            disabled={(counts.ok ?? 0) === 0}
            onClick={() => onApply(generated)}
          >
            Apply {counts.ok ?? 0} poses
          </Button>
        </div>
      )}
    </div>
  );
}
