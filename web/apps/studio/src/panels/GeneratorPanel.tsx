import type { EtendueScene, GeneratedPoses, GeneratorSpec, PlannedPoses, SceneSpec } from "@etendue/wasm";
import { LineChart, type Series } from "@vitavision/charts";
import { Button, Callout, Input, NumberInput, ReadoutStrip, Select, Switch } from "@vitavision/ui";
import { Check, Sparkles } from "lucide-react";
import { type ReactNode, useDeferredValue, useEffect, useMemo, useState } from "react";

import { toast } from "@vitavision/workbench";

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

/** The most poses the σ curve is drawn for. */
const CURVE_POSES = 40;

/**
 * Suggest poses (P18): the planner's picks among the kept candidates, the predicted system σ
 * against the number of poses beside the evenly thinned grid's, and **Apply** for the chosen
 * budget. The picks for a budget are the first picks of a larger one, so one plan draws the curve.
 */
function SuggestPoses({
  session,
  spec,
  generated,
  kept,
  onApply,
}: {
  session: EtendueScene;
  spec: GeneratorSpec;
  generated: GeneratedPoses;
  kept: number;
  onApply: (generated: GeneratedPoses) => void;
}) {
  const [budget, setBudget] = useState(12);
  const [noise, setNoise] = useState(0.5);
  const cap = Math.min(kept, CURVE_POSES);
  const shown = Math.max(1, Math.min(budget, cap));
  const input = useDeferredValue({ spec, noise, cap });
  const curve = useMemo((): { ok: PlannedPoses } | { error: string } | null => {
    if (input.cap === 0) return null;
    try {
      return { ok: session.planPoses(input.spec, { budget: input.cap, noise_px: input.noise }) };
    } catch (e) {
      return { error: describeFailure(e).message };
    }
  }, [session, input]);
  const plan = curve && "ok" in curve ? curve.ok : null;
  const series = useMemo<Series[]>(() => {
    if (!plan) return [];
    // From three poses on: fewer barely determine the calibration, and their σ would flatten the rest.
    const at = (values: (number | null)[]) =>
      values.map((y, k) => ({ x: k + 1, y: y ?? Number.NaN })).filter((p) => p.x >= 3);
    return [
      { name: "suggested", points: at(plan.sigma_mm) },
      { name: "grid", points: at(plan.grid_sigma_curve_mm) },
    ];
  }, [plan]);
  if (kept === 0) return null;
  if (curve && "error" in curve) {
    return (
      <Callout tone="warning" title="Cannot suggest poses">
        {curve.error}
      </Callout>
    );
  }
  const planned = plan?.sigma_mm[shown - 1];
  const grid = plan?.grid_sigma_curve_mm[shown - 1];
  const apply = () => {
    try {
      const chosen = session.planPoses(spec, { budget: shown, noise_px: noise });
      onApply({ candidates: generated.candidates, scenario: chosen.scenario });
    } catch (e) {
      toast({ title: "Cannot suggest poses", description: describeFailure(e).message, tone: "error" });
    }
  };
  return (
    <div className="space-y-2 border-t border-border pt-3" data-testid="suggest-poses">
      <h3 className="text-xs font-medium">Suggest poses</h3>
      <p className="text-xs text-fg-muted">
        The poses, among the kept ones, that calibrate the cell best: the smallest predicted system σ, how far the
        calibrated cell would misplace a point it knows in the robot&apos;s base (a rig hand-eye with the rig refined,
        robot poses exact).
      </p>
      <div className="grid grid-cols-[minmax(4.5rem,auto)_1fr] gap-x-3 gap-y-1.5">
        <Row label="poses">
          <NumberInput
            aria-label="Pose budget"
            value={shown}
            precision={0}
            min={1}
            max={cap}
            step={1}
            unit="×"
            onValueChange={(v) => setBudget(Math.round(v))}
          />
        </Row>
        <Row label="noise">
          <NumberInput
            aria-label="Pixel noise"
            value={noise}
            precision={2}
            min={0.01}
            step="any"
            unit="px"
            onValueChange={(v) => v > 0 && setNoise(v)}
          />
        </Row>
      </div>
      {plan && (
        <>
          <ReadoutStrip
            items={[
              { label: "suggested σ", value: planned === undefined ? "–" : `${planned.toFixed(4)} mm` },
              { label: "grid σ", value: grid == null ? "–" : `${grid.toFixed(4)} mm` },
            ]}
          />
          <LineChart
            series={series}
            label="Predicted system σ against the number of poses"
            xLabel="poses"
            yLabel="σ (mm)"
            variant="fluid"
            height={190}
            underlay={(x, y) => (
              <line
                x1={x.project(shown)}
                x2={x.project(shown)}
                y1={y.project(y.domain[0])}
                y2={y.project(y.domain[1])}
                className="stroke-signal"
                strokeWidth={1.5}
              />
            )}
          />
          <Button variant="primary" icon={<Sparkles className="size-4" />} onClick={apply}>
            Apply {shown} suggested poses
          </Button>
        </>
      )}
    </div>
  );
}

/**
 * The sphere pose generator: the sensor on spheres around the target, looking at its centre, at
 * every radius, polar angle θ and azimuth φ: eye-in-hand the robot's tool carries the sensor
 * there, eye-to-hand it carries the target so the fixed sensor sees it from there. The candidates
 * are computed as the form changes and drawn in 3D; **Apply** makes the kept ones the scenario
 * (one undoable edit), each with its joint configuration stored; **Suggest poses** picks fewer.
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
        The generator needs a robot, a rig or a camera, and a target, one of them on the robot&apos;s tool.
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
          <SuggestPoses session={session} spec={spec} generated={generated} kept={counts.ok ?? 0} onApply={onApply} />
        </div>
      )}
    </div>
  );
}
