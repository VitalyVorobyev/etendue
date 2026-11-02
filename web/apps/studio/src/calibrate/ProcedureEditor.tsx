/**
 * The procedure editor (P16, ADR 0011): a calibration procedure's stages in order, each one
 * calibration-rs workflow with the parameters it holds fixed, what else it frees, how it solves
 * and what it starts from. Everything left at "default" stays absent, so calibration-rs's own
 * default holds. The kernel checks the whole procedure on every edit.
 */

import type { Freedoms, Loss, ProcedurePart, Seeds, Stage } from "@etendue/wasm";
import { validateLibraryItem } from "@etendue/wasm";
import {
  Button,
  Callout,
  Checkbox,
  Field,
  Input,
  NumberInput,
  SegmentedControl,
  Select,
  ToggleChip,
} from "@vitavision/ui";
import { ArrowDown, ArrowUp, Plus, Trash2 } from "lucide-react";
import { useMemo, useState } from "react";

import {
  FIX_PARAMS,
  FREEDOMS,
  type FixSlot,
  type FreedomSpec,
  PROBLEMS,
  type Problem,
  addStage,
  isRig,
  moveStage,
  procedureIssues,
  removeStage,
  renameStage,
  seedKinds,
  seedOf,
  seedSlots,
  seedValue,
  tidy,
  withProblem,
} from "./procedure";

const SLOT_LABEL: Record<FixSlot, string> = { intrinsics: "Intrinsics", distortion: "Distortion", sensor: "Tilt" };
const SEED_LABEL: Record<keyof Seeds, string> = {
  intrinsics: "Intrinsics",
  sensor: "Sensor tilt",
  rig: "Rig",
  handeye: "Hand-eye",
};
const LOSSES = ["none", "huber", "cauchy", "arctan"] as const;

/** The editor's whole state is the procedure; this is its one change. */
export function ProcedureEditor({
  value,
  onChange,
  cameras = [],
  tilted = true,
  disabled = false,
  showIssues = true,
}: {
  value: ProcedurePart;
  onChange: (procedure: ProcedurePart) => void;
  /** The dataset's cameras, for a single-camera stage's choice; empty where unknown. */
  cameras?: readonly string[];
  /** Whether the design's cameras tilt (a "design" sensor model calibrates a tilt). */
  tilted?: boolean;
  disabled?: boolean;
  /** List the kernel's issues under the stages (off where the host lists them itself). */
  showIssues?: boolean;
}) {
  const issues = useMemo(() => procedureIssues(value, validateLibraryItem), [value]);
  const [next, setNext] = useState<Problem>("planar_intrinsics");
  const setStage = (i: number, stage: Stage) =>
    onChange({ stages: value.stages.map((s, j) => (j === i ? tidy(stage) : s)) });

  return (
    <div className="@container flex flex-col gap-3" data-testid="procedure-editor">
      {value.stages.map((stage, i) => (
        <StageCard
          // A stage has no identity of its own while its id is being typed: by position, so the
          // id field keeps the focus.
          // eslint-disable-next-line react/no-array-index-key
          key={i}
          index={i}
          stage={stage}
          earlier={value.stages.slice(0, i).map((s) => s.id)}
          last={i === value.stages.length - 1}
          cameras={cameras}
          tilted={tilted}
          disabled={disabled}
          onChange={(s) => setStage(i, s)}
          onRename={(id) => onChange(renameStage(value, i, id))}
          onMove={(by) => onChange(moveStage(value, i, by))}
          onRemove={() => onChange(removeStage(value, i))}
        />
      ))}
      <div className="flex flex-wrap items-end gap-2">
        <Field label="Next stage">
          <Select
            aria-label="Next stage's workflow"
            value={next}
            options={PROBLEMS.map((p) => ({ value: p.value, label: p.label }))}
            onValueChange={(v) => v && setNext(v as Problem)}
            disabled={disabled}
          />
        </Field>
        <Button
          size="sm"
          variant="secondary"
          icon={<Plus className="size-3.5" />}
          onClick={() => onChange(addStage(value, next))}
          disabled={disabled}
        >
          Add stage
        </Button>
      </div>
      {showIssues && issues.length > 0 && (
        <Callout tone="warning" title="This procedure does not check">
          <ul className="font-mono text-xs" data-testid="procedure-issues">
            {issues.map((issue) => (
              <li key={issue}>{issue}</li>
            ))}
          </ul>
        </Callout>
      )}
    </div>
  );
}

function StageCard({
  index,
  stage,
  earlier,
  last,
  cameras,
  tilted: designTilted,
  disabled,
  onChange,
  onRename,
  onMove,
  onRemove,
}: {
  index: number;
  stage: Stage;
  /** The ids of the stages before it: what it can seed from. */
  earlier: string[];
  last: boolean;
  cameras: readonly string[];
  tilted: boolean;
  disabled: boolean;
  onChange: (stage: Stage) => void;
  onRename: (id: string) => void;
  onMove: (by: -1 | 1) => void;
  onRemove: () => void;
}) {
  const problem = stage.problem;
  const sensor = stage.model?.sensor ?? "design";
  const tilted = sensor === "scheimpflug" || (sensor === "design" && designTilted);
  const label = `Stage ${index + 1}`;
  return (
    <section
      className="flex flex-col gap-3 rounded-md border border-line p-3"
      aria-label={label}
      data-testid={`stage-${index}`}
    >
      <div className="flex flex-wrap items-end gap-2">
        <span className="pb-1.5 text-xs font-medium text-fg-muted">{label}</span>
        <Field label="Id">
          <Input
            aria-label={`${label} id`}
            className="w-32 font-mono"
            value={stage.id}
            onChange={(e) => onRename(e.target.value)}
            disabled={disabled}
          />
        </Field>
        <Field label="Workflow">
          <Select
            aria-label={`${label} workflow`}
            value={problem}
            options={PROBLEMS.map((p) => ({ value: p.value, label: p.label }))}
            onValueChange={(v) => v && onChange(withProblem(stage, v as Problem))}
            disabled={disabled}
          />
        </Field>
        {!isRig(problem) && cameras.length > 0 && (
          <Field label="Camera">
            <Select
              aria-label={`${label} camera`}
              value={stage.camera ?? "__each"}
              options={[
                { value: "__each", label: cameras.length > 1 ? "Each camera" : "The camera" },
                ...cameras.map((c) => ({ value: c, label: c })),
              ]}
              onValueChange={(v) => onChange({ ...stage, camera: v === "__each" ? null : v })}
              disabled={disabled}
            />
          </Field>
        )}
        <div className="ml-auto flex gap-1">
          <Button
            size="sm"
            variant="ghost"
            aria-label={`Move ${label} up`}
            icon={<ArrowUp className="size-3.5" />}
            onClick={() => onMove(-1)}
            disabled={disabled || index === 0}
          />
          <Button
            size="sm"
            variant="ghost"
            aria-label={`Move ${label} down`}
            icon={<ArrowDown className="size-3.5" />}
            onClick={() => onMove(1)}
            disabled={disabled || last}
          />
          <Button
            size="sm"
            variant="ghost"
            aria-label={`Remove ${label}`}
            icon={<Trash2 className="size-3.5" />}
            onClick={onRemove}
            disabled={disabled}
          />
        </div>
      </div>
      <p className="text-xs text-fg-muted">{PROBLEMS.find((p) => p.value === problem)?.description}</p>

      <div className="grid gap-4 @3xl:grid-cols-2">
        <div className="flex flex-col gap-2" role="group" aria-label={`${label} camera model`}>
          <h4 className="text-xs font-medium">Camera model</h4>
          <SegmentedControl
            aria-label={`${label} sensor`}
            size="sm"
            value={sensor}
            options={[
              { value: "design", label: "As designed" },
              { value: "identity", label: "Untilted" },
              { value: "scheimpflug", label: "Scheimpflug" },
            ]}
            onValueChange={(v) =>
              onChange({ ...stage, model: { ...stage.model, sensor: v as "design" | "identity" | "scheimpflug" } })
            }
            disabled={disabled}
          />
          <FixTable stage={stage} label={label} tilted={tilted} disabled={disabled} onChange={onChange} />
        </div>

        <div className="flex flex-col gap-2" role="group" aria-label={`${label} starts from`}>
          <h4 className="text-xs font-medium">Starts from</h4>
          <div className="grid grid-cols-[repeat(auto-fill,minmax(11rem,1fr))] gap-2">
            {seedSlots(problem, tilted).map((slot) => (
              <Field key={slot} label={SEED_LABEL[slot]}>
                <Select
                  aria-label={`${label} ${SEED_LABEL[slot]} seed`}
                  value={seedValue(stage.seed?.[slot]) || "__own"}
                  options={[
                    { value: "__own", label: "The workflow's own" },
                    ...seedKinds(slot).flatMap((k) =>
                      k === "stage"
                        ? earlier.map((id) => ({ value: `stage:${id}`, label: `Stage ${id}` }))
                        : [{ value: k, label: k === "nominal" ? "Datasheet values" : "The truth (simulation)" }],
                    ),
                  ]}
                  onValueChange={(v) =>
                    onChange({ ...stage, seed: { ...stage.seed, [slot]: seedOf(v === "__own" ? "" : v) } })
                  }
                  disabled={disabled}
                />
              </Field>
            ))}
          </div>
        </div>

        <Freedoms stage={stage} label={label} disabled={disabled} onChange={onChange} />
        <Solver stage={stage} label={label} disabled={disabled} onChange={onChange} />
      </div>
    </section>
  );
}

/** Which parameters stay fixed, per slot; a slot left at the default keeps calibration-rs's. */
function FixTable({
  stage,
  label,
  tilted,
  disabled,
  onChange,
}: {
  stage: Stage;
  label: string;
  tilted: boolean;
  disabled: boolean;
  onChange: (stage: Stage) => void;
}) {
  const fix = stage.model?.fix ?? {};
  const setSlot = (slot: FixSlot, list: string[] | null) =>
    onChange({ ...stage, model: { ...stage.model, fix: { ...fix, [slot]: list } } });
  const slots = (Object.keys(FIX_PARAMS) as FixSlot[]).filter((s) => s !== "sensor" || tilted);
  return (
    <table className="text-xs" aria-label={`${label} fixed parameters`}>
      <tbody>
        {slots.map((slot) => {
          const list = fix[slot] ?? null;
          return (
            <tr key={slot} className="align-middle">
              <th scope="row" className="py-1 pr-3 text-left font-normal text-fg-muted">
                {SLOT_LABEL[slot]}
              </th>
              <td className="py-1 pr-3">
                <Checkbox
                  label="Default"
                  aria-label={`${label} ${SLOT_LABEL[slot]} default`}
                  checked={list === null}
                  onCheckedChange={(c) => setSlot(slot, c ? null : [])}
                  disabled={disabled}
                />
              </td>
              <td className="flex flex-wrap gap-1 py-1">
                {FIX_PARAMS[slot].map((param) => (
                  <ToggleChip
                    key={param}
                    checked={list?.includes(param) ?? false}
                    onCheckedChange={(c) =>
                      setSlot(
                        slot,
                        c
                          ? [...(list ?? []), param].sort(
                              (a, b) => FIX_PARAMS[slot].indexOf(a as never) - FIX_PARAMS[slot].indexOf(b as never),
                            )
                          : (list ?? []).filter((p) => p !== param),
                      )
                    }
                    disabled={disabled || list === null}
                    title={list === null ? "Uncheck Default to choose" : `Hold ${param} fixed`}
                  >
                    <span className="font-mono">{param}</span>
                  </ToggleChip>
                ))}
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

function Freedoms({
  stage,
  label,
  disabled,
  onChange,
}: {
  stage: Stage;
  label: string;
  disabled: boolean;
  onChange: (stage: Stage) => void;
}) {
  const specs = FREEDOMS.filter((f) => f.applies(stage.problem));
  if (specs.length === 0) return null;
  const set = (key: keyof Freedoms, v: unknown) => onChange({ ...stage, free: { ...stage.free, [key]: v ?? null } });
  return (
    <div className="flex flex-col gap-2" role="group" aria-label={`${label} freedoms`}>
      <h4 className="text-xs font-medium">Also estimated or held</h4>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(11rem,1fr))] gap-2">
        {specs.map((f) => (
          <Freedom key={f.key} spec={f} stage={stage} label={label} disabled={disabled} set={set} />
        ))}
      </div>
    </div>
  );
}

function Freedom({
  spec,
  stage,
  label,
  disabled,
  set,
}: {
  spec: FreedomSpec;
  stage: Stage;
  label: string;
  disabled: boolean;
  set: (key: keyof Freedoms, v: unknown) => void;
}) {
  const value = stage.free?.[spec.key];
  const name = `${label} ${spec.label}`;
  if (spec.kind === "bool")
    return (
      <Field label={spec.label}>
        <SegmentedControl
          aria-label={name}
          size="sm"
          value={value == null ? "default" : value ? "yes" : "no"}
          options={[
            { value: "default", label: "Default" },
            { value: "yes", label: "Yes" },
            { value: "no", label: "No" },
          ]}
          onValueChange={(v) => set(spec.key, v === "default" ? null : v === "yes")}
          disabled={disabled}
        />
      </Field>
    );
  if (spec.kind === "number") {
    const scale = spec.scale ?? 1;
    return (
      <Field label={spec.label} annotation="default if empty">
        <NumberInput
          aria-label={name}
          value={typeof value === "number" ? value / scale : null}
          min={0}
          unit={spec.unit}
          onValueChange={(v) => set(spec.key, v * scale)}
          onClear={() => set(spec.key, null)}
          disabled={disabled}
        />
      </Field>
    );
  }
  // Indices: the views whose target pose stays at its linear estimate.
  const list = Array.isArray(value) ? value : null;
  return (
    <Field label={spec.label} annotation="view indices">
      <Input
        aria-label={name}
        className="font-mono"
        placeholder="default"
        value={list === null ? "" : list.length === 0 ? "none" : list.join(", ")}
        onChange={(e) => {
          const text = e.target.value.trim();
          if (text === "") set(spec.key, null);
          else if (text === "none") set(spec.key, []);
          else {
            const n = text.split(/[\s,]+/).map(Number);
            if (n.every((x) => Number.isInteger(x) && x >= 0)) set(spec.key, n);
          }
        }}
        disabled={disabled}
      />
    </Field>
  );
}

function Solver({
  stage,
  label,
  disabled,
  onChange,
}: {
  stage: Stage;
  label: string;
  disabled: boolean;
  onChange: (stage: Stage) => void;
}) {
  const solver = stage.solver ?? {};
  const loss = solver.robust_loss ?? null;
  const set = (patch: Partial<NonNullable<Stage["solver"]>>) => onChange({ ...stage, solver: { ...solver, ...patch } });
  const scale = loss && loss.kind !== "none" ? loss.scale : 1;
  return (
    <div className="flex flex-col gap-2" role="group" aria-label={`${label} solver`}>
      <h4 className="text-xs font-medium">Solver</h4>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(9rem,1fr))] gap-2">
        <Field label="Iterations" annotation="default if empty">
          <NumberInput
            aria-label={`${label} iterations`}
            value={solver.max_iters ?? null}
            min={1}
            step={1}
            precision={0}
            onValueChange={(v) => set({ max_iters: Math.max(1, Math.round(v)) })}
            onClear={() => set({ max_iters: null })}
            disabled={disabled}
          />
        </Field>
        <Field label="Robust loss">
          <Select
            aria-label={`${label} robust loss`}
            value={loss?.kind ?? "__default"}
            options={[
              { value: "__default", label: "Default" },
              ...LOSSES.map((k) => ({
                value: k,
                label: k === "none" ? "Least squares" : k[0]!.toUpperCase() + k.slice(1),
              })),
            ]}
            onValueChange={(v) =>
              set({
                robust_loss: v === "__default" ? null : v === "none" ? { kind: "none" } : ({ kind: v, scale } as Loss),
              })
            }
            disabled={disabled}
          />
        </Field>
        {loss && loss.kind !== "none" && (
          <Field label="Loss scale">
            <NumberInput
              aria-label={`${label} loss scale`}
              value={loss.scale}
              min={0}
              unit="px"
              onValueChange={(v) => set({ robust_loss: { ...loss, scale: v } })}
              disabled={disabled}
            />
          </Field>
        )}
      </div>
    </div>
  );
}
