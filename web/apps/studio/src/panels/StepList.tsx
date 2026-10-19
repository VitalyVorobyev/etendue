import type { BakedScenario, MotionReport, MotionWarning, ScenarioSpec, Step, StepReport } from "@etendue/wasm";
import { Badge, Empty } from "@vitavision/ui";
import type { Playhead } from "@vitavision/workbench";
import { AlertTriangle, ArrowDown, ArrowUp, Camera, Clock, MoveRight, Pin, Spline, Trash2 } from "lucide-react";
import { useMemo } from "react";

import { useThrottledPlayhead } from "../kernel/useThrottled";

const ICON = {
  ptp_joints: MoveRight,
  ptp_pose: MoveRight,
  lin: Spline,
  capture: Camera,
  wait: Clock,
} as const;

const DEG = 180 / Math.PI;

function describe(step: Step): string {
  switch (step.type) {
    case "ptp_joints":
      return `PTP ${step.robot} to joints`;
    case "ptp_pose": {
      const [x, y, z] = step.base_se3_tool.translation;
      return `PTP ${step.robot} to (${[x, y, z].map((v) => (v * 1000).toFixed(0)).join(", ")}) mm`;
    }
    case "lin": {
      const [x, y, z] = step.base_se3_tool.translation;
      return `LIN ${step.robot} to (${[x, y, z].map((v) => (v * 1000).toFixed(0)).join(", ")}) mm`;
    }
    case "capture":
      return "Capture";
    case "wait":
      return `Wait ${step.duration_s} s`;
  }
}

/** A warning in words. */
export function warningText(w: MotionWarning): string {
  switch (w.type) {
    case "ik_restart_used":
      return `IK found this pose only from restart ${w.attempt}: the arm may switch branch and swing far. Resolve IK to pin it.`;
    case "near_limit":
      return `${w.joint} comes within ${(w.margin * DEG).toFixed(1)}° of its limit.`;
    case "near_singular":
      return `Passes near a singularity (smallest Jacobian singular value ${w.sigma.toFixed(3)}).`;
    case "large_joint_jump":
      return `Joints change ${(w.jump * DEG).toFixed(1)}° between LIN waypoints.`;
  }
}

/** The step's timing and binding limit, compactly. */
function timing(report: StepReport): string | null {
  if (report.robot === undefined && report.kind !== "wait") return null;
  if (report.duration_s === 0) return "no motion";
  const b = report.binding;
  const limit = b ? ` · ${b.joint} ${b.quantity.slice(0, 3)} ${Math.round(b.fraction * 100)}%` : "";
  return `${report.duration_s.toFixed(2)} s${limit}`;
}

/**
 * The scenario as a program: one row per step, with its duration, the limit that sets it, and
 * any warning of the motion report. A capture row seeks the playhead to its sample; the row of
 * the capture at or before the playhead is marked. With `onSteps`, each step can be moved up or
 * down and deleted.
 */
export function StepList({
  scenario,
  baked,
  report,
  playhead,
  onSteps,
}: {
  scenario: ScenarioSpec | null;
  baked: BakedScenario;
  report: MotionReport | null;
  playhead: Playhead;
  onSteps?: ((steps: Step[], label: string) => void) | undefined;
}) {
  const k = useThrottledPlayhead(playhead, 100);
  const captures = useMemo(
    () => baked.samples.flatMap((s, index) => (s.capture ? [{ id: s.capture.id, index }] : [])),
    [baked],
  );
  if (!scenario || scenario.steps.length === 0) {
    return <Empty>No scenario: the robots rest at their initial pose.</Empty>;
  }
  let capture = -1;
  let current = -1;
  for (const [i, c] of captures.entries()) if (c.index <= k) current = i;
  return (
    <ol className="divide-y divide-line text-xs" data-testid="step-list">
      {scenario.steps.map((step, i) => {
        const Icon = ICON[step.type];
        const at = step.type === "capture" ? captures[++capture] : undefined;
        const here = at !== undefined && capture === current;
        const speed = "speed_scale" in step && step.speed_scale !== 1 ? step.speed_scale : null;
        const r = report?.steps[i];
        const warnings = r?.warnings ?? [];
        const stored = step.type === "ptp_pose" && step.q !== undefined;
        const when = r ? timing(r) : null;
        const row = (
          <>
            <span className="w-6 shrink-0 text-right font-mono text-fg-muted">{i + 1}</span>
            <Icon className="size-3.5 shrink-0 text-fg-muted" aria-hidden />
            <span className="min-w-0 flex-1">
              <span className="block truncate">{describe(step)}</span>
              {when && <span className="block truncate font-mono text-fg-muted">{when}</span>}
            </span>
            {stored && (
              <span title="Joint configuration stored: the IK branch is pinned">
                <Pin className="size-3 text-fg-muted" aria-label="IK pinned" />
              </span>
            )}
            {speed !== null && <Badge>{Math.round(speed * 100)}%</Badge>}
            {warnings.length > 0 && (
              <span title={warnings.map(warningText).join("\n")} data-testid="step-warning">
                <AlertTriangle className="size-3.5 text-warn" aria-label={`${warnings.length} warning(s)`} />
              </span>
            )}
            {at && <span className="font-mono text-fg-muted">{at.id}</span>}
          </>
        );
        return (
          // Steps have no identity of their own: their place in the program is it.
          // eslint-disable-next-line react/no-array-index-key
          <li key={i} className="group flex items-center">
            <div className="min-w-0 flex-1">
              {at ? (
                <button
                  type="button"
                  className={`flex w-full items-center gap-2 px-2 py-1.5 text-left hover:bg-raised ${here ? "bg-raised text-signal" : ""}`}
                  onClick={() => playhead.set(at.index)}
                  aria-current={here ? "step" : undefined}
                >
                  {row}
                </button>
              ) : (
                <div className="flex items-center gap-2 px-2 py-1.5">{row}</div>
              )}
            </div>
            {onSteps && (
              <StepActions
                index={i}
                count={scenario.steps.length}
                onMove={(to) => {
                  const steps = [...scenario.steps];
                  const [moved] = steps.splice(i, 1);
                  steps.splice(to, 0, moved!);
                  onSteps(steps, `move step ${i + 1}`);
                }}
                onDelete={() =>
                  onSteps(
                    scenario.steps.filter((_, j) => j !== i),
                    `delete step ${i + 1}`,
                  )
                }
              />
            )}
          </li>
        );
      })}
    </ol>
  );
}

/** A step's edit buttons, shown on hover or focus. */
function StepActions({
  index,
  count,
  onMove,
  onDelete,
}: {
  index: number;
  count: number;
  onMove: (to: number) => void;
  onDelete: () => void;
}) {
  const button =
    "rounded p-1 text-fg-muted hover:bg-raised hover:text-fg disabled:opacity-30 focus-visible:opacity-100";
  return (
    <span className="flex shrink-0 items-center pr-1 opacity-0 group-focus-within:opacity-100 group-hover:opacity-100">
      <button
        type="button"
        className={button}
        disabled={index === 0}
        onClick={() => onMove(index - 1)}
        aria-label={`Move step ${index + 1} up`}
      >
        <ArrowUp className="size-3.5" />
      </button>
      <button
        type="button"
        className={button}
        disabled={index === count - 1}
        onClick={() => onMove(index + 1)}
        aria-label={`Move step ${index + 1} down`}
      >
        <ArrowDown className="size-3.5" />
      </button>
      <button type="button" className={button} onClick={onDelete} aria-label={`Delete step ${index + 1}`}>
        <Trash2 className="size-3.5" />
      </button>
    </span>
  );
}
