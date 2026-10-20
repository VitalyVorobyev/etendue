/**
 * Motion: one move of the UR5e, point-to-point in joint space or linear in Cartesian space,
 * baked with derivatives → every joint's position, velocity and acceleration over time, and the
 * motion report's duration, binding limit and warnings. `bake`, `motionReport`.
 */

import type { ScenarioSpec, Step } from "@etendue/wasm";

import { robotSession, ur5eSource } from "../kernel";
import { lineChart } from "../plot";
import { card, figure, fmt, legend, perFrame, readouts, select, series, setDrawing, showError, slider } from "../ui";

const ROBOT = "ur5e";
/** The robot session's `initial_q`: every move starts there. */
const HOME = [0, -1.5708, 1.5708, -1.5708, -1.5708, 0];

interface State {
  kind: "ptp_joints" | "lin";
  d1: number;
  d2: number;
  d3: number;
  speed: number;
}

const START: State = { kind: "ptp_joints", d1: 1.2, d2: 0.5, d3: -0.6, speed: 0.5 };

export default async function motion(host: HTMLElement): Promise<void> {
  const [session, source] = await Promise.all([robotSession(), ur5eSource()]);
  const joints = source.manifest.joints;
  const c = card(host, "A jerk-limited move: joint profiles and limit use");
  const s: State = { ...START };

  const posFig = figure(c.figures, "Joint positions (rad).");
  const velFig = figure(c.figures, "Joint velocities (rad/s); dashed: the velocity limit × speed scale.");
  const accFig = figure(c.figures, "Joint accelerations (rad/s²); dashed: the acceleration limit × speed scale.");

  const scenario = (): ScenarioSpec => {
    const target = HOME.map((v, i) => v + (i === 0 ? s.d1 : i === 1 ? s.d2 : i === 2 ? s.d3 : 0));
    const step: Step =
      s.kind === "ptp_joints"
        ? { type: "ptp_joints", robot: ROBOT, q: target, speed_scale: s.speed }
        : {
            type: "lin",
            robot: ROBOT,
            base_se3_tool: session.robotTcp(ROBOT, target).base_se3_tcp,
            speed_scale: s.speed,
          };
    return { version: 1, dt: 0.01, steps: [step] };
  };

  const draw = perFrame(() => {
    try {
      const sc = scenario();
      const baked = session.bake(sc, { derivatives: true });
      const report = session.motionReport(sc);
      const t = baked.samples.map((x) => x.t);
      const column = (pick: (x: (typeof baked.samples)[number]) => number[][] | null | undefined, j: number) =>
        baked.samples.map((x) => pick(x)?.[0]?.[j] ?? NaN);
      const chart = (
        pick: (x: (typeof baked.samples)[number]) => number[][] | null | undefined,
        yLabel: string,
        limit?: number,
      ) =>
        lineChart({
          series: joints.map((jt, j) => ({ name: jt.name, colour: series(j), x: t, y: column(pick, j) })),
          xLabel: "time (s)",
          yLabel,
          height: 220,
          ...(limit !== undefined
            ? {
                hLines: [
                  { y: limit, label: "limit" },
                  { y: -limit, label: "" },
                ],
              }
            : {}),
        });
      const names = legend(joints.map((_, j): [string, string] => [`joint ${j + 1}`, series(j)]));
      setDrawing(
        posFig,
        chart((x) => x.joint_positions, "position (rad)"),
        names,
      );
      setDrawing(
        velFig,
        chart((x) => x.joint_velocities, "velocity (rad/s)", (joints[0]?.max_velocity ?? 0) * s.speed),
      );
      setDrawing(
        accFig,
        chart((x) => x.joint_accelerations, "acceleration (rad/s²)", (joints[0]?.max_acceleration ?? 0) * s.speed),
      );

      const step = report.steps[0];
      const binding = step?.binding;
      const peak = (q: "velocity" | "acceleration" | "jerk") =>
        Math.max(0, ...(step?.peaks ?? []).map((p) => (q === "jerk" ? (p.jerk ?? 0) : p[q])));
      const warnings = (step?.warnings ?? []).map((w) => w.type.replaceAll("_", " "));
      readouts(c, [
        ["duration", `${report.duration_s.toFixed(2)} s`],
        [
          "binding limit",
          binding
            ? `${binding.quantity} of ${binding.joint.replaceAll("_", " ")} (${(binding.fraction * 100).toFixed(0)} %)`
            : "—",
        ],
        ["peak velocity / limit", `${(peak("velocity") * 100).toFixed(0)} %`],
        ["peak acceleration / limit", `${(peak("acceleration") * 100).toFixed(0)} %`],
        ["peak jerk / limit", `${(peak("jerk") * 100).toFixed(0)} %`],
        ["smallest singular value", step?.min_sigma != null ? step.min_sigma.toFixed(3) : "—"],
        ["warnings", warnings.length > 0 ? warnings.join(", ") : "none"],
      ]);
      showError(c, null);
    } catch (e) {
      showError(c, e);
    }
  });

  const kind = select(c.controls, {
    label: "move",
    options: [
      ["ptp_joints", "point to point (joint space)"],
      ["lin", "linear (straight TCP path)"],
    ],
    value: s.kind,
    onChange: (v) => {
      s.kind = v;
      draw();
    },
  });
  const sliders = {
    d1: slider(c.controls, {
      label: "joint 1 travel",
      min: -2.5,
      max: 2.5,
      step: 0.05,
      value: s.d1,
      format: (v) => fmt.deg(v, 0),
      onInput: (v) => ((s.d1 = v), draw()),
    }),
    d2: slider(c.controls, {
      label: "joint 2 travel",
      min: -1,
      max: 1,
      step: 0.05,
      value: s.d2,
      format: (v) => fmt.deg(v, 0),
      onInput: (v) => ((s.d2 = v), draw()),
    }),
    d3: slider(c.controls, {
      label: "joint 3 travel",
      min: -1.2,
      max: 1.2,
      step: 0.05,
      value: s.d3,
      format: (v) => fmt.deg(v, 0),
      onInput: (v) => ((s.d3 = v), draw()),
    }),
    speed: slider(c.controls, {
      label: "speed scale",
      min: 0.1,
      max: 1,
      step: 0.05,
      value: s.speed,
      format: (v) => `${(v * 100).toFixed(0)} %`,
      onInput: (v) => ((s.speed = v), draw()),
    }),
  };
  const reset = document.createElement("button");
  reset.type = "button";
  reset.textContent = "Reset";
  reset.addEventListener("click", () => {
    Object.assign(s, START);
    kind.value = s.kind;
    sliders.d1.value = s.d1;
    sliders.d2.value = s.d2;
    sliders.d3.value = s.d3;
    sliders.speed.value = s.speed;
    draw();
  });
  c.buttons.append(reset);
  c.note.textContent =
    "A point-to-point move is time-optimal under every joint's velocity, acceleration and jerk limits, with all joints starting and stopping together: the joint that needs longest sets the pace, and the others are slowed to match. A linear move keeps the TCP on a straight line instead, so the joints follow whatever profile that path needs.";
  draw();
}
