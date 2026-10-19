/**
 * The Motion workspace's Jog panel (P12): move a robot by its joints, or its TCP in Cartesian
 * steps (the gizmo on the TCP drags it in 3D), and teach the pose as a program step.
 */

import type { EtendueScene, RobotSource } from "@etendue/wasm";
import type { Iso3 } from "@vitavision/three";
import { Button, Callout, SegmentedControl, Select } from "@vitavision/ui";
import { Camera, MoveRight, RefreshCw, RotateCcw } from "lucide-react";
import { useMemo, useState } from "react";
import { Euler, Quaternion } from "three";

import { Fold, RowLabel, Rows } from "../panels/controls/Fold";
import { Row } from "../panels/controls/Library";
import { PoseReadout, SliderRow } from "../panels/controls/SliderRow";
import { type Jog, jogged } from "./jog";

const DEG = 180 / Math.PI;
/** A joint this close to a limit is flagged (rad). */
const NEAR_LIMIT = 5 / DEG;
/** Below this manipulability the arm is close to a singularity. */
const NEAR_SINGULAR = 0.01;

const AXES = ["x", "y", "z"] as const;

/** A step of `size` along or about one axis, as a pose. */
function delta(kind: "move" | "turn", axis: number, size: number): Iso3 {
  if (kind === "move") {
    const t: [number, number, number] = [0, 0, 0];
    t[axis] = size;
    return { rotation: [0, 0, 0, 1], translation: t };
  }
  const e = new Euler(axis === 0 ? size : 0, axis === 1 ? size : 0, axis === 2 ? size : 0);
  const q = new Quaternion().setFromEuler(e);
  return { rotation: [q.x, q.y, q.z, q.w], translation: [0, 0, 0] };
}

export function JogPanel({
  session,
  robots,
  jog,
  onJog,
  onReset,
  onTeach,
  steps = [],
  defaultStep = null,
  onReteach,
}: {
  session: EtendueScene;
  robots: readonly RobotSource[];
  jog: Jog;
  onJog: (jog: Jog) => void;
  /** Back to the robot's joints at the playhead, or another robot's. */
  onReset: (robot: string) => void;
  /** Append a PTP move to the jogged pose, and a capture if asked. */
  onTeach: (capture: boolean) => void;
  /** The program's steps that move this robot, which can be re-taught at the jogged pose. */
  steps?: readonly { index: number; label: string }[];
  /** The one offered first: the last that ran by the playhead. */
  defaultStep?: number | null;
  onReteach?: ((index: number) => void) | undefined;
}) {
  const [picked, setPicked] = useState<number | null>(null);
  const [frame, setFrame] = useState<"base" | "tool">("tool");
  const [move, setMove] = useState(0.01);
  const [turn, setTurn] = useState(5 / DEG);
  const robot = robots.find((r) => r.id === jog.robot);
  const tcp = useMemo(() => session.robotTcp(jog.robot, jog.q), [session, jog.robot, jog.q]);
  if (!robot) return null;
  const joints = robot.manifest.joints;
  const chosen =
    picked !== null && steps.some((s) => s.index === picked) ? picked : (defaultStep ?? steps.at(-1)?.index ?? 0);

  const step = (kind: "move" | "turn", axis: number, sign: number) => {
    const target = jogged(tcp.base_se3_tcp, delta(kind, axis, sign * (kind === "move" ? move : turn)), frame);
    const answer = session.robotIk(jog.robot, target, jog.q, { restarts: 0 });
    onJog({ ...jog, q: answer.converged ? answer.q : jog.q, unreachable: !answer.converged });
  };
  const near = joints.filter((j, i) => {
    const q = jog.q[i]!;
    return q - j.lower < NEAR_LIMIT || j.upper - q < NEAR_LIMIT;
  });

  return (
    <div className="pb-3" data-testid="jog-panel">
      <div className="flex items-center gap-2 px-3 py-2">
        {robots.length > 1 && (
          <Select
            aria-label="Robot"
            className="min-w-0 flex-1"
            value={jog.robot}
            options={robots.map((r) => ({ value: r.id, label: r.id }))}
            onValueChange={(id) => id !== "" && onReset(id)}
          />
        )}
        <Button
          size="sm"
          variant="ghost"
          icon={<RotateCcw className="size-3.5" />}
          onClick={() => onReset(jog.robot)}
          title="Back to the joints the program has at the playhead"
        >
          At the playhead
        </Button>
      </div>
      {jog.unreachable && (
        <div className="px-3 pb-2">
          <Callout tone="warning" title="Out of reach">
            <p>The arm cannot reach that pose from here; it stays where it was.</p>
          </Callout>
        </div>
      )}
      <Fold title="Joints" summary={near.length > 0 ? `${near.length} near a limit` : `${joints.length} joints`}>
        <Rows>
          {joints.map((j, i) => (
            <SliderRow
              key={j.name}
              label={`J${i + 1}`}
              title={j.name}
              value={jog.q[i]!}
              scale={DEG}
              unit="°"
              precision={1}
              range={[j.lower * DEG, j.upper * DEG]}
              limits={[j.lower * DEG, j.upper * DEG]}
              step={0.1}
              live={{
                preview: (v) => onJog({ ...jog, q: jog.q.map((x, k) => (k === i ? v : x)), unreachable: false }),
                commit: () => {},
                cancel: () => {},
              }}
              testId={`joint-${i}`}
            />
          ))}
        </Rows>
        {near.length > 0 && (
          <p className="text-xs text-warn">Within 5° of a limit: {near.map((j) => j.name).join(", ")}.</p>
        )}
      </Fold>
      <Fold title="TCP" summary={`in ${jog.robot}'s base`}>
        <PoseReadout pose={tcp.base_se3_tcp} testId="jog-tcp" />
        <div className="grid grid-cols-[minmax(0,1fr)_auto] gap-x-3 gap-y-1">
          <Row label="manipulability">
            <span
              className={tcp.manipulability < NEAR_SINGULAR ? "text-warn" : undefined}
              data-testid="jog-manipulability"
            >
              {tcp.manipulability.toFixed(4)}
            </span>
          </Row>
        </div>
        {tcp.manipulability < NEAR_SINGULAR && (
          <p className="text-xs text-warn">Near a singularity: small TCP motions need large joint motions.</p>
        )}
      </Fold>
      <Fold title="Cartesian" summary={`${frame} axes`}>
        <Rows>
          <RowLabel>axes</RowLabel>
          <SegmentedControl
            aria-label="Jog axes"
            size="sm"
            value={frame}
            options={[
              { value: "tool", label: "Tool" },
              { value: "base", label: "Base" },
            ]}
            onValueChange={(v) => (v === "tool" || v === "base") && setFrame(v)}
          />
          <RowLabel>step</RowLabel>
          <div className="flex gap-1.5">
            <Select
              aria-label="Move step"
              className="min-w-0 flex-1"
              value={String(move)}
              options={[0.001, 0.01, 0.05].map((v) => ({ value: String(v), label: `${v * 1000} mm` }))}
              onValueChange={(v) => v !== "" && setMove(Number(v))}
            />
            <Select
              aria-label="Turn step"
              className="min-w-0 flex-1"
              value={String(turn)}
              options={[1, 5, 15].map((v) => ({ value: String(v / DEG), label: `${v}°` }))}
              onValueChange={(v) => v !== "" && setTurn(Number(v))}
            />
          </div>
        </Rows>
        <div className="grid grid-cols-6 gap-1" role="group" aria-label="Cartesian jog">
          {(["move", "turn"] as const).flatMap((kind) =>
            AXES.flatMap((a, axis) =>
              [-1, 1].map((sign) => {
                const label = `${sign < 0 ? "−" : "+"}${kind === "turn" ? "R" : ""}${a}`;
                return (
                  <Button
                    key={label}
                    size="sm"
                    variant="ghost"
                    onClick={() => step(kind, axis, sign)}
                    aria-label={`Jog ${label}`}
                  >
                    {label}
                  </Button>
                );
              }),
            ),
          )}
        </div>
      </Fold>
      <div className="flex flex-wrap gap-1.5 px-3 pt-2">
        <Button size="sm" icon={<MoveRight className="size-3.5" />} onClick={() => onTeach(false)}>
          Add PTP step
        </Button>
        <Button size="sm" variant="ghost" icon={<Camera className="size-3.5" />} onClick={() => onTeach(true)}>
          Add PTP + capture
        </Button>
      </div>
      {onReteach && steps.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5 px-3 pt-2" data-testid="reteach">
          <Select
            aria-label="Step to re-teach"
            value={String(chosen)}
            options={steps.map((s) => ({ value: String(s.index), label: `Step ${s.label}` }))}
            onValueChange={(v) => setPicked(Number(v))}
          />
          <Button size="sm" variant="ghost" icon={<RefreshCw className="size-3.5" />} onClick={() => onReteach(chosen)}>
            Re-teach here
          </Button>
        </div>
      )}
    </div>
  );
}
