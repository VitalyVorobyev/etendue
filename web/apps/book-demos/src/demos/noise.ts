/**
 * Observation noise: the closed-loop example's ground truth observed as a detector would report
 * it — pixel noise, outliers and dropped corners — in one camera at one capture, against the
 * exact projections. `observe`.
 */

import type { ObservationNoise, Observations, ObservedView } from "@etendue/wasm";

import { closedLoopCell } from "../kernel";
import { frame, svg } from "../plot";
import { card, figure, legend, perFrame, readouts, select, series, setDrawing, showError, slider } from "../ui";

/** Outliers: the palette's red. */
const WARN = series(5);

interface State {
  sigma: number;
  outliers: number;
  outlierSigma: number;
  dropout: number;
  seed: number;
  capture: string;
  camera: string;
  magnify: "1" | "20";
}

export default async function noise(host: HTMLElement): Promise<void> {
  const cell = await closedLoopCell();
  const truth: Observations = cell.session.observe(cell.scenario, {});
  const captures = truth.captures.slice(0, 8).map((x) => x.id);
  const cameras = truth.captures[0]?.views.map((v) => v.camera) ?? [];
  const resolution = new Map((cell.scene.cameras ?? []).map((cam) => [cam.id, cam.resolution]));
  const START: State = {
    sigma: 0.5,
    outliers: 0.02,
    outlierSigma: 8,
    dropout: 0.1,
    seed: 1,
    capture: captures[0] ?? "",
    camera: cameras[0] ?? "",
    magnify: "20",
  };
  const s: State = { ...START };
  const c = card(host, "What a detector reports: noise, outliers and dropouts");
  const fig = figure(c.figures, "");

  const viewOf = (obs: Observations): ObservedView | undefined =>
    obs.captures.find((x) => x.id === s.capture)?.views.find((v) => v.camera === s.camera);

  const draw = perFrame(() => {
    try {
      const n: ObservationNoise = {
        pixel_sigma_px: s.sigma,
        outlier_fraction: s.outliers,
        outlier_sigma_px: s.outlierSigma,
        dropout_fraction: s.dropout,
        seed: s.seed,
      };
      const observed = viewOf(cell.session.observe(cell.scenario, n));
      const exact = viewOf(truth);
      const [W, H] = resolution.get(s.camera) ?? [1280, 1024];
      const mag = Number(s.magnify);
      const fr = frame([0, W, -H, 0], 520, Math.round((520 * H) / W), 6);
      const u = (px: number) => fr.x(px);
      const v = (py: number) => fr.y(-py);
      fr.root.append(
        svg("rect", {
          x: u(0),
          y: v(0),
          width: u(W) - u(0),
          height: v(H) - v(0),
          fill: "none",
          stroke: "currentColor",
          "stroke-opacity": 0.4,
        }),
      );
      const seen = new Map((observed?.points ?? []).map((p) => [p.point, p]));
      let dropped = 0;
      let outliers = 0;
      let sum = 0;
      let inliers = 0;
      let worst = 0;
      for (const t of exact?.points ?? []) {
        const [tu, tv] = t.pixel;
        const o = seen.get(t.point);
        if (!o) {
          dropped++;
          const r = 4;
          fr.root.append(
            svg("path", {
              d: `M${u(tu) - r},${v(tv) - r}L${u(tu) + r},${v(tv) + r}M${u(tu) - r},${v(tv) + r}L${u(tu) + r},${v(tv) - r}`,
              stroke: "currentColor",
              "stroke-opacity": 0.6,
            }),
          );
          continue;
        }
        const du = o.pixel[0] - tu;
        const dv = o.pixel[1] - tv;
        const d = Math.hypot(du, dv);
        if (o.outlier) outliers++;
        else {
          sum += d * d;
          inliers++;
          worst = Math.max(worst, d);
        }
        const colour = o.outlier ? WARN : series(0);
        // Outliers are drawn at their true displacement: they are large already.
        const k = o.outlier ? 1 : mag;
        const ou = tu + k * du;
        const ov = tv + k * dv;
        fr.root.append(
          svg("circle", { cx: u(tu), cy: v(tv), r: 3.5, fill: "none", stroke: "currentColor", "stroke-opacity": 0.5 }),
        );
        fr.root.append(svg("line", { x1: u(tu), y1: v(tv), x2: u(ou), y2: v(ov), stroke: colour, "stroke-width": 1 }));
        fr.root.append(svg("circle", { cx: u(ou), cy: v(ov), r: 2, fill: colour }));
      }
      fig.querySelector("figcaption")!.textContent =
        `${s.camera}'s image (${W} × ${H} px) at capture ${s.capture}: rings are the exact corners, dots what the detector reports${mag > 1 ? `, noise displacements magnified ×${mag} (outliers true size)` : ""}; crosses are corners it missed.`;
      setDrawing(
        fig,
        fr.root,
        legend([
          ["exact corner", "currentColor"],
          ["observed", series(0)],
          ["outlier", WARN],
        ]),
      );
      const total = exact?.points.length ?? 0;
      readouts(c, [
        ["corners in view", String(total)],
        ["reported", String(total - dropped)],
        ["dropped", String(dropped)],
        ["outliers", String(outliers)],
        ["RMS error (inliers)", `${inliers > 0 ? Math.sqrt(sum / inliers).toFixed(3) : "—"} px`],
        ["largest inlier error", `${worst.toFixed(3)} px`],
      ]);
      showError(
        c,
        observed && observed.points.length === 0 ? "Fewer than four corners left: this view is dropped." : null,
      );
    } catch (e) {
      showError(c, e);
    }
  });

  const selects = {
    capture: select(c.controls, {
      label: "capture",
      options: captures.map((id): [string, string] => [id, id]),
      value: s.capture,
      onChange: (x) => ((s.capture = x), draw()),
    }),
    camera: select(c.controls, {
      label: "camera",
      options: cameras.map((id): [string, string] => [id, id]),
      value: s.camera,
      onChange: (x) => ((s.camera = x), draw()),
    }),
    magnify: select(c.controls, {
      label: "displacements",
      options: [
        ["20", "magnified ×20"],
        ["1", "true size"],
      ],
      value: s.magnify,
      onChange: (x) => ((s.magnify = x), draw()),
    }),
  };
  const sliders = {
    sigma: slider(c.controls, {
      label: "pixel noise σ",
      min: 0,
      max: 2,
      step: 0.05,
      value: s.sigma,
      format: (x) => `${x.toFixed(2)} px`,
      onInput: (x) => ((s.sigma = x), draw()),
    }),
    outliers: slider(c.controls, {
      label: "outlier fraction",
      min: 0,
      max: 0.2,
      step: 0.01,
      value: s.outliers,
      format: (x) => `${(x * 100).toFixed(0)} %`,
      onInput: (x) => ((s.outliers = x), draw()),
    }),
    outlierSigma: slider(c.controls, {
      label: "outlier σ",
      min: 1,
      max: 20,
      step: 0.5,
      value: s.outlierSigma,
      format: (x) => `${x.toFixed(1)} px`,
      onInput: (x) => ((s.outlierSigma = x), draw()),
    }),
    dropout: slider(c.controls, {
      label: "dropout fraction",
      min: 0,
      max: 0.5,
      step: 0.01,
      value: s.dropout,
      format: (x) => `${(x * 100).toFixed(0)} %`,
      onInput: (x) => ((s.dropout = x), draw()),
    }),
    seed: slider(c.controls, {
      label: "seed",
      min: 0,
      max: 20,
      step: 1,
      value: s.seed,
      format: (x) => String(x),
      onInput: (x) => ((s.seed = x), draw()),
    }),
  };
  const reset = document.createElement("button");
  reset.type = "button";
  reset.textContent = "Reset";
  reset.addEventListener("click", () => {
    Object.assign(s, START);
    selects.capture.value = s.capture;
    selects.camera.value = s.camera;
    selects.magnify.value = s.magnify;
    for (const k of ["sigma", "outliers", "outlierSigma", "dropout", "seed"] as const) sliders[k].value = s[k];
    draw();
  });
  c.buttons.append(reset);
  c.note.textContent =
    "Ground truth is exact; a calibration never sees it. The observations are what a detector and a robot controller would report: every corner moved by Gaussian pixel noise, a share replaced by outliers, a share missed. The same seed gives the same observations as etendue calibrate --noise.";
  draw();
}
