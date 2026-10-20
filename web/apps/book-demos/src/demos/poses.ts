/**
 * Pose generation: the closed-loop example's sphere generator (a stereo rig on a UR5e looking at
 * a chessboard) with its radius, cone, azimuth steps and roll range → every candidate sensor
 * position around the board, coloured by whether it was kept and why not. `generatePoses`.
 */

import type { Candidate, GeneratorSpec } from "@etendue/wasm";

import { closedLoopCell } from "../kernel";
import { frame, svg } from "../plot";
import { card, figure, fmt, legend, perFrame, readouts, series, setDrawing, showError, slider } from "../ui";
import { rotate } from "./geometry";

type Status = Candidate["status"]["status"];

const STATUS: [Status, string, string][] = [
  ["ok", "kept", series(2)],
  ["not_visible", "board not fully seen", series(4)],
  ["near_limit", "near a joint limit", series(3)],
  ["near_singular", "near a singularity", series(1)],
  ["unreachable", "unreachable", series(5)],
];
const colourOf = (s: Status): string => STATUS.find(([k]) => k === s)?.[2] ?? "currentColor";

interface State {
  radius: number;
  theta: number;
  phiSteps: number;
  roll: number;
}

export default async function poses(host: HTMLElement): Promise<void> {
  const cell = await closedLoopCell();
  const base = cell.scenario.generator;
  if (!base) throw new Error("the example scenario has no generator");
  const target = cell.scene.targets?.find((t) => t.id === base.target);
  const [bx, by, bz] = target?.parent_se3_self.translation ?? [0, 0, 0];
  const [ex, ey] = cell.session.targetExtent(base.target) ?? [0.18, 0.12];
  const START: State = {
    radius: base.radii_m[base.radii_m.length - 1] ?? 0.55,
    theta: base.theta.max_rad,
    phiSteps: base.phi.steps,
    roll: Math.max(...(base.roll_rad ?? [0]).map(Math.abs)),
  };
  const s: State = { ...START };
  const c = card(host, "Generating viewpoints on a sphere around the board");
  const topFig = figure(
    c.figures,
    "Top view (x–y, metres): candidate sensor positions around the board; kept ones show their viewing direction.",
  );
  const sideFig = figure(c.figures, "Side view (x–z, metres); the robot's base is at the origin.");

  const spec = (): GeneratorSpec => {
    const inner = base.radii_m.length > 1 ? Math.min(base.radii_m[0]!, s.radius - 0.02) : s.radius;
    return {
      ...base,
      radii_m: base.radii_m.length > 1 ? [inner, s.radius] : [s.radius],
      theta: { ...base.theta, max_rad: Math.max(s.theta, base.theta.min_rad) },
      phi: { ...base.phi, steps: s.phiSteps },
      roll_rad: s.roll > 0 ? [-s.roll, s.roll] : [0],
    };
  };

  const plot = (
    cands: Candidate[],
    a: 0 | 1,
    b: 1 | 2,
    box: [number, number, number, number],
    board: [number, number, number, number],
  ) => {
    const fr = frame(box, 300, 260, 12);
    const ink = { stroke: "currentColor", "stroke-opacity": 0.25 };
    fr.root.append(svg("line", { x1: fr.x(box[0]), x2: fr.x(box[1]), y1: fr.y(0), y2: fr.y(0), ...ink }));
    fr.root.append(svg("line", { x1: fr.x(0), x2: fr.x(0), y1: fr.y(box[2]), y2: fr.y(box[3]), ...ink }));
    fr.root.append(svg("circle", { cx: fr.x(0), cy: fr.y(0), r: 4, fill: "none", stroke: "currentColor" }));
    fr.root.append(svg("text", { x: fr.x(0) + 6, y: fr.y(0) + 14, "fill-opacity": 0.7 }, "base"));
    // The board, a world-aligned rectangle `[x0, y0, x1, y1]` in this view's axes.
    const [x0, y0, x1, y1] = board;
    fr.root.append(
      svg("rect", {
        x: Math.min(fr.x(x0), fr.x(x1)),
        y: Math.min(fr.y(y0), fr.y(y1)),
        width: Math.max(2, Math.abs(fr.x(x1) - fr.x(x0))),
        height: Math.max(2, Math.abs(fr.y(y1) - fr.y(y0))),
        fill: "currentColor",
        "fill-opacity": 0.35,
        stroke: "currentColor",
      }),
    );
    // Rejected first, so the kept ones draw on top.
    const ordered = [...cands].sort((p, q) => Number(p.status.status === "ok") - Number(q.status.status === "ok"));
    for (const cand of ordered) {
      const p = cand.world_se3_sensor.translation;
      const colour = colourOf(cand.status.status);
      if (cand.status.status === "ok") {
        const dir = rotate(cand.world_se3_sensor.rotation, [0, 0, 1]);
        const L = 0.06;
        fr.root.append(
          svg("line", {
            x1: fr.x(p[a]),
            y1: fr.y(p[b]),
            x2: fr.x(p[a] + L * dir[a]),
            y2: fr.y(p[b] + L * dir[b]),
            stroke: colour,
            "stroke-width": 1.5,
          }),
        );
      }
      fr.root.append(svg("circle", { cx: fr.x(p[a]), cy: fr.y(p[b]), r: 3.2, fill: colour, "fill-opacity": 0.9 }));
    }
    return fr.root;
  };

  const draw = perFrame(() => {
    try {
      const t0 = performance.now();
      const out = cell.session.generatePoses(spec());
      const ms = performance.now() - t0;
      const cands = out.candidates;
      const counts = new Map<Status, number>();
      for (const cand of cands) counts.set(cand.status.status, (counts.get(cand.status.status) ?? 0) + 1);
      const top = plot(
        cands,
        0,
        1,
        [bx - 0.55, bx + 0.55, by - 0.5, by + 0.5],
        [bx - ex / 2, by - ey / 2, bx + ex / 2, by + ey / 2],
      );
      const side = plot(
        cands,
        0,
        2,
        [bx - 0.55, bx + 0.55, bz - 0.15, bz + 0.8],
        [bx - ex / 2, bz - 0.004, bx + ex / 2, bz + 0.004],
      );
      setDrawing(
        topFig,
        top,
        legend(
          STATUS.filter(([k]) => counts.has(k)).map(([k, label, colour]): [string, string] => [
            `${label} (${counts.get(k)})`,
            colour,
          ]),
        ),
      );
      setDrawing(sideFig, side);
      readouts(c, [
        ["candidates", String(cands.length)],
        ["kept (captures)", String(counts.get("ok") ?? 0)],
        ["rejected", String(cands.length - (counts.get("ok") ?? 0))],
        ["generated in", `${ms.toFixed(0)} ms`],
      ]);
      showError(c, null);
    } catch (e) {
      showError(c, e);
    }
  });

  const sliders = {
    radius: slider(c.controls, {
      label: "outer radius",
      min: 0.35,
      max: 0.9,
      step: 0.01,
      value: s.radius,
      format: (v) => fmt.m(v, 2),
      onInput: (v) => ((s.radius = v), draw()),
    }),
    theta: slider(c.controls, {
      label: "cone half-angle θ max",
      min: 0.2,
      max: 1.0,
      step: 0.01,
      value: s.theta,
      format: (v) => fmt.deg(v, 0),
      onInput: (v) => ((s.theta = v), draw()),
    }),
    phiSteps: slider(c.controls, {
      label: "azimuth steps",
      min: 2,
      max: 12,
      step: 1,
      value: s.phiSteps,
      format: (v) => String(v),
      onInput: (v) => ((s.phiSteps = v), draw()),
    }),
    roll: slider(c.controls, {
      label: "roll ±",
      min: 0,
      max: 0.8,
      step: 0.01,
      value: s.roll,
      format: (v) => fmt.deg(v, 0),
      onInput: (v) => ((s.roll = v), draw()),
    }),
  };
  const reset = document.createElement("button");
  reset.type = "button";
  reset.textContent = "Reset";
  reset.addEventListener("click", () => {
    Object.assign(s, START);
    for (const [k, sl] of Object.entries(sliders)) sl.value = s[k as keyof State];
    draw();
  });
  c.buttons.append(reset);
  c.note.textContent =
    "Each candidate puts the rig on a sphere around the board, looking at its centre: radii × polar angles θ × azimuths φ × rolls. It is kept when the robot reaches it by inverse kinematics, away from joint limits and singularities, and every camera sees the whole board. Widen the cone or the radius and watch candidates fall out of reach.";
  draw();
}
