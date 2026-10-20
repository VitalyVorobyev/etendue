/**
 * The Scheimpflug construction of a laser head, in the plane through the camera's axis
 * perpendicular to the laser plane: the sensor, the principal planes, the plane of best focus, the
 * laser and its window, the depth of field, and the hinge rule. A tilt slider, and a button that
 * solves the Scheimpflug condition (`headSection`, `headScheimpflug`, `headMap`).
 */

import {
  type HeadSection,
  type SectionLine,
  type SectionPoint,
  headMap,
  headScheimpflug,
  headSection,
} from "@etendue/wasm";

import { type Head, DEFAULT_HEAD, aimDistance, toDesign } from "../head";
import { kernel } from "../kernel";
import { type Frame, frame, svg } from "../plot";
import { button, card, figure, fmt, legend, perFrame, readouts, series, setDrawing, showError, slider } from "../ui";

const START: Head = { ...DEFAULT_HEAD };

/** World `[x, y]` of a section point: `x` the camera axis `z`, `y` = −`s` (camera up). */
const xy = (p: SectionPoint): [number, number] => [p[1], -p[0]];

function line(fr: Frame, a: SectionPoint, b: SectionPoint, attrs: Record<string, string | number>): SVGLineElement {
  const [x1, y1] = xy(a);
  const [x2, y2] = xy(b);
  return svg("line", { x1: fr.x(x1), y1: fr.y(y1), x2: fr.x(x2), y2: fr.y(y2), ...attrs });
}

/** A long segment along `l`. */
function through(l: SectionLine, reach = 2): [SectionPoint, SectionPoint] {
  const [s, z] = l.point;
  const [ds, dz] = l.direction;
  return [
    [s - reach * ds, z - reach * dz],
    [s + reach * ds, z + reach * dz],
  ];
}

function polygon(fr: Frame, points: SectionPoint[], attrs: Record<string, string | number>): SVGPolygonElement {
  return svg("polygon", {
    points: points
      .map((p) => xy(p))
      .map(([x, y]) => `${fr.x(x)},${fr.y(y)}`)
      .join(" "),
    ...attrs,
  });
}

function drawSection(sec: HeadSection, box: [number, number, number, number], close: boolean): SVGSVGElement {
  const fr = frame(box, 720, close ? 260 : 300, 10);
  const clip = `ed-clip-${Math.random().toString(36).slice(2)}`;
  fr.root.append(svg("defs", {}, svg("clipPath", { id: clip }, svg("rect", { x: 0, y: 0, width: 720, height: 300 }))));
  const g = svg("g", { "clip-path": `url(#${clip})` });
  fr.root.append(g);
  const cp = sec.cardinal_points;
  const thin = { stroke: "currentColor", "stroke-opacity": 0.35, "stroke-width": 1 };
  // The camera axis.
  g.append(line(fr, [0, box[0]], [0, box[1]], { ...thin, "stroke-dasharray": "6 4" }));
  // Depth of field: the band between the near and far limits.
  if (sec.depth_of_field) {
    const [near, far] = sec.depth_of_field;
    g.append(polygon(fr, [...near, ...[...far].reverse()], { fill: series(2), "fill-opacity": 0.18, stroke: "none" }));
  }
  // The field of view.
  for (const f of sec.field)
    g.append(
      line(fr, f.point, [f.point[0] + f.direction[0] * 2, f.point[1] + f.direction[1] * 2], {
        ...thin,
        "stroke-opacity": 0.2,
      }),
    );
  // Principal planes H and H′ (lines across the axis).
  for (const z of new Set([cp.front_principal, cp.rear_principal]))
    g.append(line(fr, [-1, z], [1, z], { ...thin, "stroke-dasharray": "2 3" }));
  // The sensor plane, extended, and the plane of best focus.
  const sensorLine: SectionLine = {
    point: sec.sensor[0],
    direction: [sec.sensor[1][0] - sec.sensor[0][0], sec.sensor[1][1] - sec.sensor[0][1]],
  };
  const sn = Math.hypot(...sensorLine.direction);
  sensorLine.direction = [sensorLine.direction[0] / sn, sensorLine.direction[1] / sn];
  const [s0, s1] = through(sensorLine);
  g.append(line(fr, s0, s1, { stroke: "currentColor", "stroke-opacity": 0.45, "stroke-dasharray": "4 3" }));
  const [f0, f1] = through(sec.focus);
  g.append(line(fr, f0, f1, { stroke: series(0), "stroke-width": 1.5 }));
  // The laser: its trace and window.
  const [l0, l1] = through(sec.laser_trace);
  g.append(line(fr, l0, l1, { stroke: series(1), "stroke-opacity": 0.6 }));
  if (sec.window)
    g.append(
      line(fr, sec.window[0], sec.window[1], { stroke: series(1), "stroke-width": 5, "stroke-linecap": "round" }),
    );
  const [lx, ly] = xy(sec.laser_origin);
  g.append(svg("circle", { cx: fr.x(lx), cy: fr.y(ly), r: 4, fill: series(1) }));
  // Chief rays: object → entrance pupil, exit pupil → sensor.
  for (const r of sec.rays.filter((r) => r.kind === "chief")) {
    g.append(line(fr, r.object, r.entrance, { stroke: "currentColor", "stroke-opacity": 0.5 }));
    g.append(line(fr, r.exit, r.on_sensor, { stroke: "currentColor", "stroke-opacity": 0.5 }));
  }
  // The sensor itself.
  g.append(line(fr, sec.sensor[0], sec.sensor[1], { stroke: "currentColor", "stroke-width": close ? 4 : 6 }));
  // The lens: its entrance pupil as an aperture.
  g.append(
    line(fr, [-sec.entrance_radius, cp.entrance_pupil], [sec.entrance_radius, cp.entrance_pupil], {
      stroke: series(3),
      "stroke-width": 3,
    }),
  );
  // The hinge rule: the sensor plane meets H′, and the focus plane meets H, at one height.
  for (const p of [sec.sensor_at_rear_principal, sec.focus_at_front_principal]) {
    if (!p) continue;
    const [x, y] = xy(p);
    g.append(svg("circle", { cx: fr.x(x), cy: fr.y(y), r: 5, fill: "none", stroke: series(4), "stroke-width": 2 }));
  }
  return fr.root;
}

export default async function scheimpflug(host: HTMLElement): Promise<void> {
  await kernel();
  const c = card(host, "Scheimpflug: tilt the sensor to lay the focus on the laser plane");
  const h: Head = { ...START, tilt: [...START.tilt] };
  const whole = figure(
    c.figures,
    "The head in its section plane: camera at left looking right (+z), laser above.",
    true,
  );
  const lens = figure(
    c.figures,
    "Close up on the camera: sensor (black), principal planes (dotted), entrance pupil (purple).",
    true,
  );

  const draw = perFrame(() => {
    try {
      const design = toDesign(h);
      const d = aimDistance(h);
      const sec = headSection(design, {
        probes: [d - h.depth_m / 2, d, d + h.depth_m / 2],
        blur_limit_px: h.max_blur_px,
      });
      const map = headMap(design, null, 21, 31);
      let worst = 0;
      let seen = 0;
      for (let i = 0; i < map.blur_px.length; i++) {
        if (!map.visible[i]) continue;
        seen++;
        worst = Math.max(worst, map.blur_px[i]!);
      }
      setDrawing(
        whole,
        drawSection(sec, [-0.01, h.aim_m + h.depth_m, -0.03, h.baseline_m + 0.03], false),
        legend([
          ["plane of best focus", series(0)],
          ["laser and window", series(1)],
          ["depth of field", series(2)],
          ["hinge points", series(4)],
        ]),
      );
      const cp = sec.cardinal_points;
      setDrawing(lens, drawSection(sec, [-0.006, cp.front_focal + 0.006, -0.012, 0.012], true));
      const a = sec.sensor_at_rear_principal;
      const b = sec.focus_at_front_principal;
      showError(c, null);
      readouts(c, [
        ["sensor tilt τx", fmt.deg(h.tilt[0], 2)],
        ["focus distance", fmt.m(h.focus_distance_m)],
        ["worst blur over the window", seen ? fmt.px(worst) : "not seen"],
        ["window seen", `${((100 * seen) / map.blur_px.length).toFixed(0)} %`],
        ["hinge heights", a && b ? `${fmt.mm(-a[0])}, ${fmt.mm(-b[0])}` : "parallel planes"],
      ]);
    } catch (e) {
      showError(c, e);
    }
  });

  const tilt = slider(c.controls, {
    label: "sensor tilt τx",
    min: -0.4,
    max: 0.4,
    step: 0.002,
    value: h.tilt[0],
    format: (v) => fmt.deg(v, 1),
    onInput: (v) => ((h.tilt = [v, 0]), draw()),
  });
  const focus = slider(c.controls, {
    label: "focus distance",
    min: 0.2,
    max: 0.6,
    step: 0.002,
    value: h.focus_distance_m,
    format: (v) => fmt.m(v),
    onInput: (v) => ((h.focus_distance_m = v), draw()),
  });
  const fnum = slider(c.controls, {
    label: "f-number",
    min: 1.4,
    max: 16,
    step: 0.1,
    value: h.f_number,
    format: fmt.f,
    onInput: (v) => ((h.f_number = v), draw()),
  });
  const sync = () => {
    tilt.value = h.tilt[0];
    focus.value = h.focus_distance_m;
    fnum.value = h.f_number;
    draw();
  };
  button(c.buttons, "Solve the Scheimpflug condition", () => {
    try {
      const s = headScheimpflug(toDesign(h));
      h.tilt = [s.tilt[0], 0];
      h.focus_distance_m = s.focus_distance_m;
      sync();
    } catch (e) {
      showError(c, e);
    }
  });
  button(c.buttons, "Square sensor", () => {
    h.tilt = [0, 0];
    sync();
  });
  button(c.buttons, "Reset", () => {
    Object.assign(h, START, { tilt: [...START.tilt] });
    sync();
  });
  c.note.textContent =
    "With the sensor square to the axis, the plane of best focus is square too and crosses the laser plane at one depth. Solving the condition tilts the sensor until the plane of best focus lies on the laser plane: the hinge points coincide, and the whole window is in focus. What blur remains is diffraction.";
  draw();
}
