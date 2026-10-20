/**
 * Robot kinematics on the UR5e: joint sliders → the TCP pose and manipulability (forward
 * kinematics), the arm drawn as its link frames in a side and a top view; and inverse kinematics
 * from spread seeds, which finds the other configurations (branches) that reach the same TCP.
 * `robotTcp`, `robotLinkPoses`, `robotIk`.
 */

import type { EtendueScene, IkAnswer, RobotTcp } from "@etendue/wasm";

import { robotSession, ur5eSource } from "../kernel";
import { frame, svg } from "../plot";
import { card, figure, fmt, legend, perFrame, readouts, series, setDrawing, showError, slider } from "../ui";
import { maxJointDiff, rpy } from "./geometry";

const ROBOT = "ur5e";
const START = [0, -2.0, 1.6, -1.2, -1.5708, 0];
/** Branch `i`'s colour: the palette after the first colour, which is the user's arm. */
const branchColour = (i: number): string => series(1 + (i % 5));

/** Whole degrees, without a negative zero. */
const deg0 = (rad: number): string => `${Math.round((rad * 180) / Math.PI) || 0}°`;

const NAMES = ["base", "shoulder", "elbow", "wrist 1", "wrist 2", "wrist 3"];

/** Link origins, `[x, y, z]` per link, from `robotLinkPoses`' seven values per link. */
function origins(session: EtendueScene, q: readonly number[]): [number, number, number][] {
  const poses = session.robotLinkPoses(ROBOT, q);
  const out: [number, number, number][] = [];
  for (let i = 0; i + 6 < poses.length; i += 7) out.push([poses[i + 4]!, poses[i + 5]!, poses[i + 6]!]);
  return out;
}

/** The arm in one orthographic view: axes `a` and `b` of each link origin. */
function view(
  arms: { points: [number, number, number][]; colour: string; dashed: boolean }[],
  a: 0 | 1,
  b: 1 | 2,
  box: [number, number, number, number],
  labels: [string, string],
): SVGSVGElement {
  const fr = frame(box, 300, 260, 12);
  const ink = { stroke: "currentColor", "stroke-opacity": 0.25 };
  fr.root.append(svg("line", { x1: fr.x(box[0]), x2: fr.x(box[1]), y1: fr.y(0), y2: fr.y(0), ...ink }));
  fr.root.append(svg("line", { x1: fr.x(0), x2: fr.x(0), y1: fr.y(box[2]), y2: fr.y(box[3]), ...ink }));
  fr.root.append(
    svg("text", { x: fr.x(box[1]) - 4, y: fr.y(0) - 4, "text-anchor": "end", "fill-opacity": 0.7 }, labels[0]),
  );
  fr.root.append(svg("text", { x: fr.x(0) + 4, y: fr.y(box[3]) + 12, "fill-opacity": 0.7 }, labels[1]));
  for (const arm of arms) {
    const d = arm.points.map((p, i) => `${i ? "L" : "M"}${fr.x(p[a]).toFixed(1)},${fr.y(p[b]).toFixed(1)}`).join("");
    fr.root.append(
      svg("path", {
        d,
        fill: "none",
        stroke: arm.colour,
        "stroke-width": arm.dashed ? 2 : 3,
        "stroke-linejoin": "round",
        ...(arm.dashed ? { "stroke-dasharray": "5 3" } : {}),
      }),
    );
    for (const p of arm.points)
      fr.root.append(svg("circle", { cx: fr.x(p[a]), cy: fr.y(p[b]), r: 2.5, fill: arm.colour }));
    const tcp = arm.points[arm.points.length - 1];
    if (tcp && !arm.dashed)
      fr.root.append(svg("circle", { cx: fr.x(tcp[a]), cy: fr.y(tcp[b]), r: 5, fill: "none", stroke: arm.colour }));
  }
  return fr.root;
}

/** Seeds on the other branches: shoulder turned half a turn, elbow and wrist mirrored. */
function seeds(q: readonly number[], lower: number[], upper: number[]): number[][] {
  const out: number[][] = [];
  for (let mask = 1; mask < 8; mask++) {
    const s = [...q];
    if (mask & 1) s[0] = s[0]! + (s[0]! > 0 ? -Math.PI : Math.PI);
    if (mask & 2) {
      s[2] = -s[2]!;
      s[1] = s[1]! + q[2]!;
    }
    if (mask & 4) {
      s[4] = -s[4]!;
      s[3] = s[3]! + Math.PI;
      s[5] = s[5]! + Math.PI;
    }
    out.push(s.map((v, i) => Math.min(upper[i]!, Math.max(lower[i]!, v))));
  }
  return out;
}

export default async function kinematics(host: HTMLElement): Promise<void> {
  const [session, source] = await Promise.all([robotSession(), ur5eSource()]);
  const lower = source.manifest.joints.map((j) => Math.max(j.lower, -Math.PI));
  const upper = source.manifest.joints.map((j) => Math.min(j.upper, Math.PI));
  const c = card(host, "Forward and inverse kinematics of a UR5e");
  const q = [...START];
  let branches: { q: number[]; answer: IkAnswer }[] = [];
  let ikNote = "";

  const side = figure(c.figures, "Side view (x–z), metres. Dots are the link frames' origins; the ring is the TCP.");
  const top = figure(c.figures, "Top view (x–y), metres.");

  const draw = perFrame(() => {
    try {
      const tcp: RobotTcp = session.robotTcp(ROBOT, q);
      const arms = [{ points: origins(session, q), colour: series(0), dashed: false }];
      branches.forEach((b, i) => arms.push({ points: origins(session, b.q), colour: branchColour(i), dashed: true }));
      const lg =
        branches.length > 0
          ? legend([
              ["your configuration", series(0)],
              ...branches.map((_, i): [string, string] => [`branch ${i + 1}`, branchColour(i)]),
            ])
          : undefined;
      setDrawing(side, view(arms, 0, 2, [-0.9, 0.9, -0.2, 1.1], ["x", "z"]), lg);
      setDrawing(top, view(arms, 0, 1, [-0.9, 0.9, -0.9, 0.9], ["x", "y"]));
      const [x, y, z] = tcp.base_se3_tcp.translation;
      const [r, p, w] = rpy(tcp.base_se3_tcp.rotation);
      readouts(c, [
        ["TCP x, y, z", `${(x * 1000).toFixed(0)}, ${(y * 1000).toFixed(0)}, ${(z * 1000).toFixed(0)} mm`],
        ["roll, pitch, yaw", [r, p, w].map(deg0).join(", ")],
        ["manipulability √det(J Jᵀ)", tcp.manipulability.toFixed(4)],
        ["IK", ikNote || "press “Find other branches”"],
      ]);
      showError(c, null);
    } catch (e) {
      showError(c, e);
    }
  });

  const sliders = NAMES.map((name, i) =>
    slider(c.controls, {
      label: `joint ${i + 1} (${name})`,
      min: lower[i]!,
      max: upper[i]!,
      step: 0.01,
      value: q[i]!,
      format: (v) => fmt.deg(v, 0),
      onInput: (v) => {
        q[i] = v;
        branches = [];
        ikNote = "";
        draw();
      },
    }),
  );

  const find = document.createElement("button");
  find.type = "button";
  find.textContent = "Find other branches (IK)";
  find.addEventListener("click", () => {
    try {
      const target = session.robotTcp(ROBOT, q).base_se3_tcp;
      const found: { q: number[]; answer: IkAnswer }[] = [];
      let worst = 0;
      let iterations = 0;
      for (const seed of seeds(q, lower, upper)) {
        const answer = session.robotIk(ROBOT, target, seed, { restarts: 0 });
        if (!answer.converged) continue;
        iterations += answer.iterations;
        worst = Math.max(worst, answer.translation_error ?? 0);
        const distinct = [q, ...found.map((f) => f.q)].every((other) => maxJointDiff(answer.q, other) > 0.05);
        if (distinct) found.push({ q: answer.q, answer });
      }
      branches = found;
      ikNote =
        found.length === 0
          ? "no other branch converged from these seeds"
          : `${found.length + 1} configurations, worst position error ${(worst * 1e9).toFixed(2)} nm, ${iterations} iterations in all`;
      draw();
    } catch (e) {
      showError(c, e);
    }
  });
  const reset = document.createElement("button");
  reset.type = "button";
  reset.textContent = "Reset";
  reset.addEventListener("click", () => {
    START.forEach((v, i) => {
      q[i] = v;
      sliders[i]!.value = v;
    });
    branches = [];
    ikNote = "";
    draw();
  });
  c.buttons.append(find, reset);
  c.note.textContent =
    "Forward kinematics chains the joint transforms from the base to the TCP. Inverse kinematics runs the other way and is not unique: started from seeds on other branches (shoulder turned, elbow or wrist flipped), damped least squares lands on other configurations that put the TCP at exactly the same pose.";
  draw();
}
