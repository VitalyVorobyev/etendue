/**
 * The Motion workspace: the program (with its motion report), jogging a robot at the playhead
 * (P12), the pose generator, and the joint chart. The 3-D view is the studio's; this gives it the
 * jogged robot's poses and handle, the generator's candidates and each robot's TCP path.
 */

import type { FrameTreeRuntime } from "@vitavision/three";
import { Button, Empty, Panel, SegmentedControl, Select } from "@vitavision/ui";
import { type Playhead, toast } from "@vitavision/workbench";
import type { GeneratedPoses } from "@etendue/wasm";
import { ChevronDown, ChevronUp, Pin } from "lucide-react";
import { type ReactNode, useMemo, useState } from "react";

import { type Baked, describeFailure } from "../kernel/etendue";
import type { PoseOverride } from "../kernel/posed";
import { useThrottledPlayhead } from "../kernel/useThrottled";
import { GeneratorPanel } from "../panels/GeneratorPanel";
import { JointChart, type JointQuantity } from "../panels/JointChart";
import { StepList } from "../panels/StepList";
import type { GizmoTarget, Viewport } from "../panels/Viewport";
import { type Jog, bakedQ, jogOverrides, motionSteps, reteach, robotFrames, teach } from "../robot/jog";
import { JogPanel } from "../robot/JogPanel";
import { stepAt, tcpPath } from "../scene/TcpPath";
import type { Studio } from "../state/studio";

const NO_OVERRIDES: readonly PoseOverride[] = [];

type ViewportProps = Parameters<typeof Viewport>[0];

type MotionPanel = "program" | "generate" | "jog";

export interface MotionWorkspace {
  /** The program, the jog or the generator. */
  right: ReactNode;
  /** The joint chart, under the 3-D view. */
  chart: ReactNode;
  /** The jogged robot's world poses, drawn over the bake. */
  overrides: readonly PoseOverride[];
  /** The jogged robot's TCP handle. */
  gizmo: GizmoTarget | null;
  /** The generator's candidates, while it shows. */
  candidates: GeneratedPoses["candidates"] | null;
  /** Each robot's TCP over the program, with the step at the playhead. */
  paths: NonNullable<ViewportProps["paths"]>;
  /** The robot whose joints the chart shows (and whose motion the exports export). */
  shownRobot: string | null;
}

export function useMotionWorkspace({
  active,
  studio,
  current,
  opened,
  playing,
  playhead,
  runtime,
  selectedRobot,
}: {
  /** Whether the workspace shows: what it computes is computed only then. */
  active: boolean;
  studio: Studio;
  current: Baked | null;
  /** The open document's number: the generator starts again for another. */
  opened: number;
  playing: boolean;
  playhead: Playhead;
  runtime: FrameTreeRuntime | null;
  /** The robot of the selection, if any. */
  selectedRobot: string | null;
}): MotionWorkspace {
  const [panel, setPanel] = useState<MotionPanel>("program");
  // A robot jogged at the playhead; it lapses when the playhead moves or the program bakes.
  const [jogState, setJog] = useState<(Jog & { baked: unknown }) | null>(null);
  const [jogRobot, setJogRobot] = useState<string | null>(null);
  const [preview, setPreview] = useState<GeneratedPoses | null>(null);
  const [chartRobot, setChartRobot] = useState<string | null>(null);
  const [chartOpen, setChartOpen] = useState(true);
  const [quantity, setQuantity] = useState<JointQuantity>("position");

  const k = useThrottledPlayhead(playhead, 50);
  const jogging = current !== null && active && panel === "jog" && !playing;
  const jogFrames = useMemo(() => {
    const robot = jogRobot ?? current?.loaded.scene.robots?.[0]?.id;
    return robot && current ? { robot, frames: robotFrames(current.loaded.robots, robot) } : null;
  }, [current, jogRobot]);
  const jog = useMemo<Jog | null>(() => {
    if (!jogging || !current || !jogFrames) return null;
    if (jogState && jogState.baked === current.baked && jogState.k === k && jogState.robot === jogFrames.robot) {
      return jogState;
    }
    const q = bakedQ(current.baked, jogFrames.robot, k);
    return q ? { robot: jogFrames.robot, q, k } : null;
  }, [jogging, current, jogFrames, jogState, k]);
  const updateJog = (j: Jog) => current && setJog({ ...j, baked: current.baked });
  const overrides = useMemo(
    () =>
      jog && runtime && current && jogFrames?.frames
        ? jogOverrides(current.session, runtime, current.loaded.scene, current.baked.frames, jogFrames.frames.base, jog)
        : NO_OVERRIDES,
    [jog, runtime, current, jogFrames],
  );
  const gizmo: GizmoTarget | null =
    jog && current && jogFrames?.frames
      ? {
          frame: jogFrames.frames.tcp,
          parent: jogFrames.frames.base,
          placement: current.session.robotTcp(jog.robot, jog.q).base_se3_tcp,
          onPreview: (pose) => {
            const answer = current.session.robotIk(jog.robot, pose, jog.q, { restarts: 0 });
            updateJog({ ...jog, q: answer.converged ? answer.q : jog.q, unreachable: !answer.converged });
          },
          onCommit: () => {},
          onCancel: () => {},
        }
      : null;

  const scenario = current?.loaded.scenario ?? null;
  // The motion report (computed when the workspace shows).
  const report = useMemo(() => {
    if (!active || !current?.loaded.scenario) return null;
    try {
      return current.session.motionReport(current.loaded.scenario);
    } catch {
      return null;
    }
  }, [active, current]);
  const unresolved = scenario?.steps.some((s) => s.type === "ptp_pose" && s.q === undefined) ?? false;
  const resolveIk = () => {
    if (!current || !scenario) return;
    try {
      studio.setScenario(current.session.resolveScenario(scenario), "resolve IK");
    } catch (e) {
      const failure = describeFailure(e);
      toast({ title: failure.title, description: failure.message, tone: "error" });
    }
  };

  // Each robot's TCP over the program, the motion at (or that led to) the playhead highlighted.
  const tcpPaths = useMemo(
    () =>
      current
        ? (current.loaded.scene.robots ?? []).flatMap((r) => {
            const frames = robotFrames(current.loaded.robots, r.id);
            const path = frames && tcpPath(current.baked, frames.tcp);
            return path ? [path] : [];
          })
        : [],
    [current],
  );
  const pathStep = active && current ? stepAt(report, current.baked.samples[k]?.t ?? 0, true) : -1;
  const paths = useMemo(
    () => ({ data: tcpPaths, report, current: pathStep, onSeek: (i: number) => playhead.set(i) }),
    [tcpPaths, report, pathStep, playhead],
  );

  // The jogged robot's steps, any of which can be re-taught at the jogged pose; the last that ran
  // by the playhead is offered first.
  const jogSteps = useMemo(() => (jog ? motionSteps(current?.loaded.scenario ?? null, jog.robot) : []), [jog, current]);
  const jogStep = jog
    ? (jogSteps.filter((s) => s.index <= stepAt(report, current?.baked.samples[jog.k]?.t ?? 0)).at(-1)?.index ?? null)
    : null;
  const reteachJog = (index: number) => {
    const step = scenario?.steps[index];
    if (!jog || !current || !scenario || !step) return;
    const next = reteach(step, jog.q, current.session.robotTcp(jog.robot, jog.q).base_se3_tcp);
    if (next) {
      studio.setScenario(
        { ...scenario, steps: scenario.steps.map((s, i) => (i === index ? next : s)) },
        `re-teach step ${index + 1}`,
      );
    }
  };
  // The jogged pose as a program step (and a capture), the playhead at its end.
  const teachJog = (capture: boolean) => {
    if (!jog || !current) return;
    const tcp = current.session.robotTcp(jog.robot, jog.q).base_se3_tcp;
    if (
      studio.setScenario(
        teach(current.loaded.scenario, jog.robot, jog.q, tcp, capture),
        capture ? "teach a capture" : "teach a pose",
      )
    ) {
      const baked = studio.get().current?.baked;
      if (baked) playhead.set(baked.samples.length - 1);
    }
  };

  const robots = current?.baked.robots ?? [];
  const shownRobot = robots.find((r) => r.id === chartRobot)?.id ?? selectedRobot ?? robots[0]?.id ?? null;

  const right = current ? (
    <div className="flex h-full min-h-0 flex-col">
      <div className="border-b border-line p-2">
        <SegmentedControl
          value={panel}
          options={[
            { value: "program", label: "Program" },
            { value: "jog", label: "Jog" },
            { value: "generate", label: "Generate poses" },
          ]}
          onValueChange={(v) => {
            if (v === "program" || v === "generate" || v === "jog") setPanel(v);
            if (v === "jog") studio.setPlaying(false);
          }}
          aria-label="Motion panel"
        />
      </div>
      <div className="min-h-0 flex-1 overflow-auto">
        {panel === "jog" ? (
          jog ? (
            <JogPanel
              session={current.session}
              robots={current.loaded.robots}
              jog={jog}
              onJog={updateJog}
              onReset={(robot) => {
                setJog(null);
                setJogRobot(robot);
              }}
              onTeach={teachJog}
              steps={jogSteps}
              defaultStep={jogStep}
              onReteach={reteachJog}
            />
          ) : (
            <Empty>{playing ? "Pause to jog." : "The cell has no robot to jog."}</Empty>
          )
        ) : panel === "generate" ? (
          <GeneratorPanel
            key={opened}
            session={current.session}
            scene={current.loaded.scene}
            initial={current.loaded.scenario?.generator ?? null}
            onPreview={setPreview}
            onApply={(generated) => {
              if (studio.setScenario(generated.scenario, "generate poses")) setPanel("program");
            }}
          />
        ) : (
          <Panel
            title="Program"
            bodyClassName="p-0"
            actions={
              <Button
                size="sm"
                variant="ghost"
                icon={<Pin className="size-3.5" />}
                disabled={!unresolved}
                title={
                  unresolved
                    ? "Store every PTP pose's joint configuration, so the IK branch can never change"
                    : "Every PTP pose's configuration is stored"
                }
                onClick={resolveIk}
              >
                Resolve IK
              </Button>
            }
          >
            {report && (
              <div
                className="border-b border-line px-3 py-2 font-mono text-xs text-fg-muted"
                data-testid="motion-summary"
              >
                {report.duration_s.toFixed(2)} s · {report.captures} captures ·{" "}
                {report.steps.reduce((n, s) => n + (s.warnings?.length ?? 0), 0)} warnings
              </div>
            )}
            <StepList
              scenario={current.loaded.scenario}
              baked={current.baked}
              report={report}
              playhead={playhead}
              onSteps={(steps, label) =>
                current.loaded.scenario && studio.setScenario({ ...current.loaded.scenario, steps }, label)
              }
            />
          </Panel>
        )}
      </div>
    </div>
  ) : null;

  const chart =
    current && robots.length > 0 ? (
      <div className={`flex shrink-0 flex-col border-t border-line bg-surface ${chartOpen ? "h-60" : ""}`}>
        <div className="flex h-8 items-center gap-2 px-2">
          <Button
            variant="ghost"
            size="sm"
            icon={chartOpen ? <ChevronDown className="size-4" /> : <ChevronUp className="size-4" />}
            onClick={() => setChartOpen(!chartOpen)}
            aria-expanded={chartOpen}
          >
            Joints
          </Button>
          {robots.length > 1 && shownRobot && (
            <Select
              aria-label="Robot"
              value={shownRobot}
              options={robots.map((r) => ({ value: r.id, label: r.id }))}
              onValueChange={(v) => v !== "" && setChartRobot(v)}
              className="w-40"
            />
          )}
          {chartOpen && (
            <SegmentedControl
              value={quantity}
              options={[
                { value: "position", label: "Position" },
                { value: "velocity", label: "Velocity" },
                { value: "acceleration", label: "Acceleration" },
              ]}
              onValueChange={(v) => v !== "" && setQuantity(v as JointQuantity)}
              aria-label="Joint quantity"
            />
          )}
        </div>
        {chartOpen && shownRobot && (
          <div className="min-h-0 flex-1 px-2 pb-2">
            <JointChart
              baked={current.baked}
              robot={shownRobot}
              joints={current.loaded.robots.find((r) => r.id === shownRobot)?.manifest.joints ?? []}
              quantity={quantity}
              playhead={playhead}
              height={150}
            />
          </div>
        )}
      </div>
    ) : null;

  return {
    right,
    chart,
    overrides,
    gizmo,
    candidates: active && panel === "generate" ? (preview?.candidates ?? null) : null,
    paths,
    shownRobot,
  };
}
