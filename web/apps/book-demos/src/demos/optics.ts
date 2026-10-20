/**
 * A physical camera: focal length, f-number, focus and the lens's principal-plane gap and
 * entrance-pupil offset → the derived pinhole (`fx`), the cardinal points on the axis, and focus
 * breathing (`fx` against the focus distance). `deriveOptics`.
 */

import { type DerivedOptics, deriveOptics } from "@etendue/wasm";

import { kernel } from "../kernel";
import { frame, lineChart, svg } from "../plot";
import { card, figure, fmt, perFrame, readouts, series, setDrawing, showError, slider } from "../ui";

const PITCH = 5.86e-6;
const RES: [number, number] = [1936, 1216];

interface State {
  f: number;
  n: number;
  focus: number;
  gap: number;
  pupil: number;
}

const START: State = { f: 0.025, n: 4, focus: 0.5, gap: 0.004, pupil: 0.012 };

function derive(s: State, focus = s.focus): DerivedOptics {
  return deriveOptics(
    {
      sensor: { pixel_pitch_m: PITCH },
      lens: { focal_length_m: s.f, f_number: s.n, principal_gap_m: s.gap, entrance_pupil_m: s.pupil },
      focus_distance_m: focus,
    },
    {
      projection: { type: "pinhole" },
      distortion: { type: "none" },
      sensor: { type: "identity" },
      intrinsics: { type: "fx_fy_cx_cy_skew", fx: 1, fy: 1, cx: RES[0] / 2, cy: RES[1] / 2, skew: 0 },
    },
  );
}

const fxOf = (d: DerivedOptics): number => {
  const i = d.params.intrinsics as { fx: number };
  return i.fx;
};

/** The cardinal points on the axis, in mm from the image-plane datum. */
function axisDrawing(d: DerivedOptics): SVGSVGElement {
  const cp = d.cardinal_points;
  const marks: [string, number][] = [
    ["sensor", cp.sensor],
    ["H′", cp.rear_principal],
    ["H", cp.front_principal],
    ["E", cp.entrance_pupil],
    ["E′", cp.exit_pupil],
    ["F′", cp.rear_focal],
    ["F", cp.front_focal],
  ];
  const zs = marks.map(([, z]) => z * 1000);
  const lo = Math.min(...zs) - 4;
  const hi = Math.max(...zs) + 4;
  const fr = frame([lo, hi, -10, 10], 520, 150, 16);
  const y = fr.y(0);
  fr.root.append(
    svg("line", { x1: fr.x(lo), x2: fr.x(hi), y1: y, y2: y, stroke: "currentColor", "stroke-opacity": 0.5 }),
  );
  // The sensor, and the entrance and exit pupils as apertures.
  const ep = (d.entrance_pupil_diameter_m * 1000) / 2;
  const xp = (d.exit_pupil_diameter_m * 1000) / 2;
  const bar = (z: number, r: number, colour: string) =>
    svg("line", { x1: fr.x(z), x2: fr.x(z), y1: fr.y(r), y2: fr.y(-r), stroke: colour, "stroke-width": 3 });
  fr.root.append(bar(0, 6, "currentColor"));
  fr.root.append(bar(cp.entrance_pupil * 1000, Math.min(ep, 9), series(0)));
  fr.root.append(bar(cp.exit_pupil * 1000, Math.min(xp, 9), series(1)));
  const sorted = marks.map(([n, z], i) => ({ n, z: z * 1000, i })).sort((a, b) => a.z - b.z);
  sorted.forEach((m, k) => {
    const up = k % 2 === 0;
    const x = fr.x(m.z);
    fr.root.append(svg("circle", { cx: x, cy: y, r: 3 }));
    fr.root.append(svg("text", { x, y: up ? y - 34 : y + 44, "text-anchor": "middle" }, m.n));
    fr.root.append(
      svg(
        "text",
        { x, y: up ? y - 22 : y + 32, "text-anchor": "middle", "fill-opacity": 0.7, "font-size": 10 },
        `${m.z.toFixed(1)}`,
      ),
    );
  });
  fr.root.append(
    svg("text", { x: fr.x(hi) - 2, y: y - 6, "text-anchor": "end", "fill-opacity": 0.7 }, "z (mm) → object"),
  );
  return fr.root;
}

export default async function optics(host: HTMLElement): Promise<void> {
  await kernel();
  const c = card(host, "A physical camera: the pinhole its optics imply");
  const s: State = { ...START };
  const axisFig = figure(
    c.figures,
    "Cardinal points on the axis, mm from the image-plane datum. Bars: entrance pupil E (blue) and exit pupil E′ (orange).",
    true,
  );
  const breathFig = figure(
    c.figures,
    "Focus breathing: the derived focal length fx as the lens focuses (sensor pitch 5.86 µm).",
  );

  const draw = perFrame(() => {
    try {
      const d = derive(s);
      showError(c, null);
      setDrawing(axisFig, axisDrawing(d));
      const focus: number[] = [];
      const fx: number[] = [];
      for (let i = 0; i <= 60; i++) {
        const z = 0.15 * (3 / 0.15) ** (i / 60);
        try {
          fx.push(fxOf(derive(s, z)));
          focus.push(z);
        } catch {
          // Closer than the lens can focus.
        }
      }
      setDrawing(
        breathFig,
        lineChart({
          series: [{ name: "fx", colour: series(0), x: focus, y: fx }],
          xLabel: "focus distance (m)",
          yLabel: "fx (px)",
          vLines: [{ x: s.focus, label: "now" }],
        }),
      );
      const hfov = 2 * Math.atan(RES[0] / 2 / fxOf(d));
      readouts(c, [
        ["fx (derived)", `${fxOf(d).toFixed(1)} px`],
        ["f / pitch", `${(s.f / PITCH).toFixed(1)} px`],
        ["horizontal field of view", fmt.deg(hfov)],
        ["image distance (H′ to sensor)", fmt.mm(d.image_distance_m, 2)],
        ["object distance (H to focus)", fmt.m(d.object_distance_m)],
        ["magnification", d.magnification.toFixed(4)],
        ["entrance pupil diameter", fmt.mm(d.entrance_pupil_diameter_m, 2)],
        ["working f-number", d.working_f_number.toFixed(2)],
      ]);
    } catch (e) {
      showError(c, e);
    }
  });

  const sliders = {
    f: slider(c.controls, {
      label: "focal length",
      min: 0.008,
      max: 0.075,
      step: 0.001,
      value: s.f,
      format: (v) => fmt.mm(v, 0),
      onInput: (v) => ((s.f = v), draw()),
    }),
    n: slider(c.controls, {
      label: "f-number",
      min: 1.4,
      max: 16,
      step: 0.1,
      value: s.n,
      format: fmt.f,
      onInput: (v) => ((s.n = v), draw()),
    }),
    focus: slider(c.controls, {
      label: "focus distance",
      min: 0.2,
      max: 3,
      step: 0.01,
      value: s.focus,
      format: (v) => fmt.m(v, 2),
      onInput: (v) => ((s.focus = v), draw()),
    }),
    gap: slider(c.controls, {
      label: "principal-plane gap H′→H",
      min: -0.02,
      max: 0.02,
      step: 0.0005,
      value: s.gap,
      format: (v) => fmt.mm(v),
      onInput: (v) => ((s.gap = v), draw()),
    }),
    pupil: slider(c.controls, {
      label: "entrance pupil H→E",
      min: -0.02,
      max: 0.04,
      step: 0.0005,
      value: s.pupil,
      format: (v) => fmt.mm(v),
      onInput: (v) => ((s.pupil = v), draw()),
    }),
  };
  c.note.textContent =
    "fx is not f / pitch: focusing moves the lens away from the sensor, so the image distance and the derived focal length grow as the focus comes closer. The pinhole's centre is the entrance pupil E.";
  const reset = document.createElement("button");
  reset.type = "button";
  reset.textContent = "Reset";
  reset.addEventListener("click", () => {
    Object.assign(s, START);
    for (const [k, sl] of Object.entries(sliders)) sl.value = s[k as keyof State];
    draw();
  });
  c.buttons.append(reset);
  draw();
}
