import { type HeadOptimum, headOptimise } from "@etendue/wasm";
import { Button, Callout, Input, Switch } from "@vitavision/ui";
import { Crosshair, PackagePlus, Sparkles } from "lucide-react";
import { useState } from "react";

import type { Project } from "../io/project";
import { Fold, RowLabel, Rows } from "../panels/controls/Fold";
import { Pick, Row, SaveButton } from "../panels/controls/Library";
import { type Live, NumberRow, SliderRow } from "../panels/controls/SliderRow";
import type { HeadAnalysis } from "./analysis";
import { type HeadParams, fromItem, toDesign, withSetting } from "./design";

const DEG = 180 / Math.PI;

/** A live edit of one number of the design: shown as it moves (there is no bake to wait for). */
function live(
  params: HeadParams,
  onChange: (p: HeadParams) => void,
  set: (p: HeadParams, v: number) => HeadParams,
): Live<number> {
  return { preview: (v) => onChange(set(params, v)), commit: () => {}, cancel: () => {} };
}

/**
 * The head's design: its parts from the project library, the camera's setting, where the laser
 * sits, the window, the optimiser and saving to the library.
 */
export function OpticsForm({
  params,
  onChange,
  analysis,
  project,
  onSave,
  onPlace,
}: {
  params: HeadParams;
  onChange: (p: HeadParams) => void;
  analysis: HeadAnalysis | string;
  project: Project | null;
  /** Save the design to the project's library; `null` when the project is read-only. */
  onSave: ((p: HeadParams) => Promise<boolean>) | null;
  /** Put the head into the open cell; `null` when no cell is open. */
  onPlace: ((p: HeadParams) => void) | null;
}) {
  const [tiltFree, setTiltFree] = useState(true);
  const [optimum, setOptimum] = useState<{ for: HeadParams; result: HeadOptimum } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const p = params;
  const f = p.lens.focal_length_m;
  const lib = (ref: string) => project?.library.get(ref);
  const hinge = typeof analysis === "string" ? null : analysis.hinge;

  const optimise = () => {
    try {
      const result = headOptimise(toDesign(p), {
        grid: [9, 9],
        f_number_range: [p.lens.min_f_number, 22],
        tilt_free: tiltFree,
      });
      setOptimum({ for: p, result });
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message.split("\n")[0]! : String(e));
    }
  };
  const result = optimum?.for === p ? optimum.result : null;

  return (
    <div className="pb-3" data-testid="optics-form">
      <div className="flex items-center gap-2 px-3 py-2">
        <Input
          aria-label="Design name"
          value={p.name}
          className="h-7 min-w-0 flex-1 text-sm"
          onChange={(e) => onChange({ ...p, name: e.target.value })}
        />
      </div>
      <Fold title="Parts" summary={`${(f * 1000).toFixed(0)} mm · ${p.sensor.name}`}>
        <Rows>
          <Pick
            label="head"
            project={project}
            kind="head"
            current={p.name}
            onPick={(ref) => {
              const item = lib(ref);
              const next = item && fromItem(item);
              if (next) onChange(next);
              else
                setError(
                  "This head's laser is not placed as this editor places lasers (its line along camera x, aimed at the axis).",
                );
            }}
          />
          <Pick
            label="sensor"
            project={project}
            kind="sensor"
            current={p.sensor.name}
            onPick={(ref) => {
              const s = lib(ref);
              if (s?.sensor)
                onChange({
                  ...p,
                  sensor: { name: s.name, pixel_pitch_m: s.sensor.pixel_pitch_m, resolution: s.sensor.resolution },
                });
            }}
          />
          <Pick
            label="lens"
            project={project}
            kind="lens"
            current={p.lens.name}
            onPick={(ref) => {
              const l = lib(ref)?.lens;
              if (l) {
                onChange({
                  ...p,
                  lens: {
                    name: lib(ref)!.name,
                    focal_length_m: l.focal_length_m,
                    min_f_number: l.min_f_number,
                    principal_gap_m: l.principal_gap_m ?? 0,
                    entrance_pupil_m: l.entrance_pupil_m ?? 0,
                  },
                  f_number: Math.max(p.f_number, l.min_f_number),
                });
              }
            }}
          />
          <Pick
            label="laser"
            project={project}
            kind="laser"
            current={p.laser.name}
            onPick={(ref) => {
              const l = lib(ref)?.laser;
              if (l)
                onChange({
                  ...p,
                  laser: {
                    name: lib(ref)!.name,
                    wavelength_nm: l.wavelength_nm,
                    fan_half_angle: l.fan_half_angle,
                    beam_waist_m: l.beam_waist_m ?? null,
                  },
                });
            }}
          />
        </Rows>
      </Fold>
      <Fold
        title="Camera"
        summary={`f/${p.f_number.toFixed(1)} · ${(p.focus_distance_m * 1000).toFixed(0)} mm · ${(p.tilt[0] * DEG).toFixed(2)}°`}
      >
        <Rows>
          <SliderRow
            label="f-number"
            value={p.f_number}
            precision={1}
            range={[p.lens.min_f_number, 22]}
            limits={[p.lens.min_f_number, 64]}
            step={0.1}
            live={live(p, onChange, (q, v) => ({ ...q, f_number: v }))}
          />
          <SliderRow
            label="focus"
            value={p.focus_distance_m}
            scale={1000}
            unit="mm"
            precision={1}
            range={[Math.ceil((4.2 * f + p.lens.principal_gap_m) * 1000), 2000]}
            limits={[1, 100_000]}
            step={1}
            live={live(p, onChange, (q, v) => ({ ...q, focus_distance_m: v }))}
            testId="focus"
          />
          <SliderRow
            label="tilt x"
            value={p.tilt[0]}
            scale={DEG}
            unit="°"
            precision={2}
            range={[-25, 25]}
            step={0.05}
            live={live(p, onChange, (q, v) => ({ ...q, tilt: [v, q.tilt[1]] }))}
            testId="tilt-x"
          />
          <SliderRow
            label="tilt y"
            value={p.tilt[1]}
            scale={DEG}
            unit="°"
            precision={2}
            range={[-25, 25]}
            step={0.05}
            live={live(p, onChange, (q, v) => ({ ...q, tilt: [q.tilt[0], v] }))}
          />
        </Rows>
        <div className="flex flex-wrap gap-1.5">
          <Button
            size="sm"
            icon={<Crosshair className="size-3.5" />}
            disabled={!hinge}
            onClick={() => hinge && onChange(withSetting(p, { ...hinge, f_number: p.f_number }))}
            title="Tilt the sensor and focus so the plane of best focus is the laser plane"
          >
            Scheimpflug
          </Button>
          <Button size="sm" variant="ghost" onClick={() => onChange({ ...p, tilt: [0, 0] })}>
            Square sensor
          </Button>
        </div>
      </Fold>
      <Fold
        title="Lens"
        summary={`g ${(p.lens.principal_gap_m * 1000).toFixed(1)} · e ${(p.lens.entrance_pupil_m * 1000).toFixed(1)} mm`}
        defaultOpen={false}
      >
        <Rows>
          <NumberRow
            label="focal length"
            value={f}
            scale={1000}
            unit="mm"
            precision={2}
            limits={[1, 1000]}
            live={live(p, onChange, (q, v) => ({ ...q, lens: { ...q.lens, focal_length_m: v } }))}
          />
          <SliderRow
            label="gap H−H′"
            value={p.lens.principal_gap_m}
            scale={1000}
            unit="mm"
            precision={1}
            range={[-20, 20]}
            step={0.1}
            live={live(p, onChange, (q, v) => ({ ...q, lens: { ...q.lens, principal_gap_m: v } }))}
          />
          <SliderRow
            label="pupil E−H"
            value={p.lens.entrance_pupil_m}
            scale={1000}
            unit="mm"
            precision={1}
            range={[-40, Math.floor(f * 1000 - 1)]}
            step={0.1}
            live={live(p, onChange, (q, v) => ({ ...q, lens: { ...q.lens, entrance_pupil_m: v } }))}
          />
        </Rows>
      </Fold>
      <Fold
        title="Laser"
        summary={`${(p.baseline_m * 1000).toFixed(0)} mm off axis · aimed at ${(p.aim_m * 1000).toFixed(0)} mm`}
      >
        <Rows>
          <SliderRow
            label="baseline"
            value={p.baseline_m}
            scale={1000}
            unit="mm"
            precision={1}
            range={[5, 400]}
            step={0.5}
            live={live(p, onChange, (q, v) => ({ ...q, baseline_m: v }))}
            testId="baseline"
          />
          <SliderRow
            label="height"
            value={p.height_m}
            scale={1000}
            unit="mm"
            precision={1}
            range={[-100, 300]}
            step={0.5}
            live={live(p, onChange, (q, v) => ({ ...q, height_m: Math.min(v, q.aim_m - 0.01) }))}
          />
          <SliderRow
            label="aim"
            value={p.aim_m}
            scale={1000}
            unit="mm"
            precision={1}
            range={[50, 2000]}
            step={1}
            live={live(p, onChange, (q, v) => ({ ...q, aim_m: Math.max(v, q.height_m + 0.01) }))}
          />
        </Rows>
      </Fold>
      <Fold
        title="Window"
        summary={`${(p.depth_m * 1000).toFixed(0)} × ${(p.width_m * 1000).toFixed(0)} mm · ≤ ${p.max_blur_px} px`}
      >
        <Rows>
          <SliderRow
            label="depth"
            value={p.depth_m}
            scale={1000}
            unit="mm"
            precision={1}
            range={[1, 500]}
            step={0.5}
            live={live(p, onChange, (q, v) => ({ ...q, depth_m: v }))}
          />
          <SliderRow
            label="width"
            value={p.width_m}
            scale={1000}
            unit="mm"
            precision={1}
            range={[1, 400]}
            step={0.5}
            live={live(p, onChange, (q, v) => ({ ...q, width_m: v }))}
          />
          <SliderRow
            label="max blur"
            value={p.max_blur_px}
            unit="px"
            precision={1}
            range={[0.5, 10]}
            step={0.1}
            live={live(p, onChange, (q, v) => ({ ...q, max_blur_px: v }))}
          />
        </Rows>
      </Fold>
      <Fold
        title="Optimise"
        summary={result ? `${result.worst_blur_px.toFixed(2)} px worst` : "smallest worst blur over the window"}
      >
        <Switch
          checked={tiltFree}
          onCheckedChange={setTiltFree}
          label="Tilt the sensor"
          description="Without, the sensor stays square to the axis: focus and f-number balance defocus against diffraction."
        />
        <Button
          size="sm"
          icon={<Sparkles className="size-3.5" />}
          onClick={optimise}
          disabled={typeof analysis === "string"}
        >
          Optimise
        </Button>
        {result && (
          <div className="space-y-2 rounded-md border border-line p-2 text-xs" data-testid="optimum">
            <Rows>
              <RowLabel>worst blur</RowLabel>
              <span className="font-mono tabular-nums">
                now {typeof analysis === "string" ? "—" : analysis.stats.worstBlurPx.toFixed(2)} · start{" "}
                {result.start_worst_blur_px.toFixed(2)} → <b>{result.worst_blur_px.toFixed(2)}</b> px
              </span>
              <RowLabel>setting</RowLabel>
              <span className="font-mono tabular-nums">
                {(result.best.tilt[0] * DEG).toFixed(2)}°, {(result.best.tilt[1] * DEG).toFixed(2)}° ·{" "}
                {(result.best.focus_distance_m * 1000).toFixed(1)} mm · f/{result.best.f_number.toFixed(1)}
              </span>
            </Rows>
            {typeof analysis !== "string" && result.worst_blur_px > analysis.stats.worstBlurPx + 1e-6 && (
              <p className="text-fg-muted">
                The current design is better than this optimum{tiltFree ? "" : ": let the sensor tilt"}.
              </p>
            )}
            <div className="flex gap-1.5">
              <Button
                size="sm"
                onClick={() => {
                  onChange(withSetting(p, result.best));
                  setOptimum(null);
                }}
              >
                Apply
              </Button>
              <Button size="sm" variant="ghost" onClick={() => setOptimum(null)}>
                Discard
              </Button>
            </div>
          </div>
        )}
        {error && <p className="text-xs text-defect">{error}</p>}
      </Fold>
      <Fold title="Library" summary={project?.manifest.name ?? ""}>
        <Rows>
          <RowLabel>id</RowLabel>
          <Input
            aria-label="Library id"
            value={p.id}
            className="h-7 font-mono text-xs"
            onChange={(e) => onChange({ ...p, id: e.target.value.replace(/[^a-z0-9_-]/g, "_").toLowerCase() })}
          />
        </Rows>
        <div className="flex flex-wrap gap-1.5">
          <SaveButton onSave={onSave && (() => onSave(p))} disabled={typeof analysis === "string" || p.id === ""} />
          {onPlace && (
            <Button
              size="sm"
              variant="ghost"
              icon={<PackagePlus className="size-3.5" />}
              disabled={typeof analysis === "string"}
              onClick={() => onPlace(p)}
              title="Add this head's camera and laser to the open cell"
            >
              Place in cell
            </Button>
          )}
        </div>
      </Fold>
    </div>
  );
}

const mm = (m: number, digits = 1) => `${(m * 1000).toFixed(digits)} mm`;
const um = (m: number) => `${(m * 1e6).toFixed(1)} µm`;

/** What the design gives: the camera's derived optics and the window's worst figures. */
export function OpticsReadouts({ params, analysis }: { params: HeadParams; analysis: HeadAnalysis | string }) {
  if (typeof analysis === "string") {
    return (
      <div className="p-3">
        <Callout tone="warning" title="The kernel refused this head">
          <p>{analysis}</p>
        </Callout>
      </div>
    );
  }
  const { derived, stats, hinge } = analysis;
  const fx = (derived.params.intrinsics as { fx: number }).fx;
  const fxInf = params.lens.focal_length_m / params.sensor.pixel_pitch_m;
  const airy = 2.44 * params.laser.wavelength_nm * 1e-9 * derived.working_f_number;
  const off = hinge
    ? Math.max(Math.abs(hinge.tilt[0] - params.tilt[0]), Math.abs(hinge.tilt[1] - params.tilt[1])) * DEG
    : null;
  const focusOff = hinge ? Math.abs(hinge.focus_distance_m - params.focus_distance_m) : null;
  const scheimpflug = off !== null && focusOff !== null && off < 0.01 && focusOff < 1e-4;
  return (
    <div className="pb-3" data-testid="optics-readouts">
      <Fold title="Window" summary={`${(stats.coverage * 100).toFixed(0)} % seen · ${stats.worstBlurPx.toFixed(2)} px`}>
        <div className="grid grid-cols-[minmax(0,1fr)_auto] gap-x-3 gap-y-1">
          <Row label="seen">{(stats.coverage * 100).toFixed(1)} %</Row>
          <Row label="worst blur">
            <span className={stats.worstBlurPx > params.max_blur_px ? "text-defect" : "text-normal"}>
              {stats.worstBlurPx.toFixed(2)} px
            </span>
          </Row>
          <Row label="depth / px">
            {um(stats.depthPerPx[0])} – {um(stats.depthPerPx[1])}
          </Row>
          <Row label="lateral / px">
            {um(stats.lateralPerPx[0])} – {um(stats.lateralPerPx[1])}
          </Row>
          <Row label="triangulation">
            {(stats.triangulationRad[0] * DEG).toFixed(1)}° – {(stats.triangulationRad[1] * DEG).toFixed(1)}°
          </Row>
        </div>
      </Fold>
      <Fold
        title="Scheimpflug"
        summary={scheimpflug ? "laser plane in focus" : off === null ? "none" : `${off.toFixed(2)}° off`}
      >
        <div className="grid grid-cols-[minmax(0,1fr)_auto] gap-x-3 gap-y-1">
          <Row label="hinge tilt">
            {hinge ? `${(hinge.tilt[0] * DEG).toFixed(2)}°, ${(hinge.tilt[1] * DEG).toFixed(2)}°` : "—"}
          </Row>
          <Row label="hinge focus">{hinge ? mm(hinge.focus_distance_m) : "—"}</Row>
        </div>
      </Fold>
      <Fold title="Camera" summary={`fx ${fx.toFixed(0)} px`}>
        <div className="grid grid-cols-[minmax(0,1fr)_auto] gap-x-3 gap-y-1">
          <Row label="fx">{fx.toFixed(1)} px</Row>
          <Row label="breathing">
            {fx >= fxInf ? "+" : ""}
            {((fx / fxInf - 1) * 100).toFixed(2)} %
          </Row>
          <Row label="magnification">{derived.magnification.toFixed(4)}×</Row>
          <Row label="object pixel">{um(params.sensor.pixel_pitch_m / derived.magnification)}</Row>
          <Row label="working f/#">{derived.working_f_number.toFixed(2)}</Row>
          <Row label="Airy ⌀">
            {um(airy)} · {(airy / params.sensor.pixel_pitch_m).toFixed(2)} px
          </Row>
          <Row label="entrance pupil ⌀">{mm(derived.entrance_pupil_diameter_m, 2)}</Row>
          <Row label="pupil magnification">{derived.pupil_magnification.toFixed(3)}</Row>
          <Row label="H′ to sensor">{mm(derived.image_distance_m, 2)}</Row>
        </div>
      </Fold>
    </div>
  );
}
