import { LineChart, type Series } from "@vitavision/charts";
import { Button, Callout, Input, SegmentedControl, Select, Switch } from "@vitavision/ui";
import { PackagePlus } from "lucide-react";
import { useDeferredValue, useMemo } from "react";

import type { Project } from "../io/project";
import type { HeadParams } from "../optics/design";
import { Fold, RowLabel, Rows } from "../panels/controls/Fold";
import { Pick, Row, SaveButton } from "../panels/controls/Library";
import { type Live, SliderRow } from "../panels/controls/SliderRow";
import { type RigAnalysis, tiltSweep } from "./analysis";
import { type RigParams, fromItem, tiltOf, unitOfHead, unitOfItem, withWorkingDistance } from "./design";
import type { VoxelShow } from "./RigView";

const DEG = 180 / Math.PI;
const OPTICS_HEAD = "__optics";

function live(
  params: RigParams,
  onChange: (p: RigParams) => void,
  set: (p: RigParams, v: number) => RigParams,
): Live<number> {
  return { preview: (v) => onChange(set(params, v)), commit: () => {}, cancel: () => {} };
}

const mm = (m: number) => `${(m * 1000).toFixed(0)} mm`;

/** The rig's design: its unit, how the units are arranged, and the box it is judged over. */
export function RigForm({
  params,
  onChange,
  analysis,
  project,
  head,
  onSave,
  onPlace,
}: {
  params: RigParams;
  onChange: (p: RigParams) => void;
  analysis: RigAnalysis | string;
  project: Project | null;
  /** The Optics workspace's head, offered as a unit. */
  head: HeadParams | null;
  onSave: ((p: RigParams) => Promise<boolean>) | null;
  onPlace: ((p: RigParams) => void) | null;
}) {
  const p = params;
  const stereo = p.arrangement === "stereo";
  const tilt = tiltOf(p);
  const refused = typeof analysis === "string";
  return (
    <div className="pb-3" data-testid="rig-form">
      <div className="flex items-center gap-2 px-3 py-2">
        <Input
          aria-label="Rig name"
          value={p.name}
          className="h-7 min-w-0 flex-1 text-sm"
          onChange={(e) => onChange({ ...p, name: e.target.value })}
        />
      </div>
      <Fold title="Unit" summary={`${p.unit.kind === "head" ? "head" : "camera"} · ${p.unit.name}`}>
        <Rows>
          <Pick
            label="rig"
            project={project}
            kind="rig"
            current={p.name}
            onPick={(ref) => {
              const item = project?.library.get(ref);
              const next = item && fromItem(item);
              if (next) onChange(next);
            }}
          />
          <Pick
            label="unit"
            project={project}
            kind={["camera", "head"]}
            current={p.unit.name}
            extra={head ? [{ value: OPTICS_HEAD, label: `${head.name} (Optics)` }] : []}
            onPick={(ref) => {
              if (ref === OPTICS_HEAD) {
                if (head) onChange({ ...p, unit: unitOfHead(head) });
                return;
              }
              const item = project?.library.get(ref);
              const unit = item && unitOfItem(item);
              if (unit) onChange({ ...p, unit });
            }}
          />
        </Rows>
        {p.unit.kind === "camera" && p.unit.camera.optics && (
          <Switch
            checked={p.refocus}
            onCheckedChange={(on) => onChange({ ...p, refocus: on })}
            label="Focus on the working distance"
            description={
              p.refocus
                ? "Each camera focuses where its axis crosses the working distance."
                : `As the unit is: ${mm(p.unit.camera.optics.focus_distance_m)}.`
            }
          />
        )}
        <p className="text-xs text-fg-muted">
          {p.unit.kind === "head"
            ? "Every camera carries its own laser, fixed to its body as the head was designed."
            : "Cameras only: coverage counts the views of each point."}
        </p>
      </Fold>
      <Fold
        title="Arrangement"
        summary={
          stereo
            ? `stereo · ${mm(2 * p.radius_m)} · ${p.converge ? `converging at ${mm(p.working_distance_m)}` : "parallel"}`
            : `ring of ${p.count} · ρ ${mm(p.radius_m)} · φ ${(tilt * DEG).toFixed(1)}°`
        }
      >
        <SegmentedControl
          aria-label="Arrangement"
          size="sm"
          value={p.arrangement}
          options={[
            { value: "stereo", label: "Stereo" },
            { value: "ring", label: "Ring" },
          ]}
          onValueChange={(v) => (v === "stereo" || v === "ring") && onChange({ ...p, arrangement: v })}
        />
        <Rows>
          {stereo ? (
            <SliderRow
              label="baseline"
              value={2 * p.radius_m}
              scale={1000}
              unit="mm"
              precision={1}
              range={[10, 600]}
              step={1}
              live={live(p, onChange, (q, v) => ({ ...q, radius_m: v / 2 }))}
              testId="baseline"
            />
          ) : (
            <>
              <SliderRow
                label="k cameras"
                value={p.count}
                precision={0}
                range={[1, 8]}
                limits={[1, 16]}
                step={1}
                live={live(p, onChange, (q, v) => ({ ...q, count: Math.round(v) }))}
                testId="count"
              />
              <SliderRow
                label="ρ radius"
                value={p.radius_m}
                scale={1000}
                unit="mm"
                precision={1}
                range={[0, 300]}
                step={1}
                live={live(p, onChange, (q, v) => ({ ...q, radius_m: v }))}
                testId="radius"
              />
            </>
          )}
          <SliderRow
            label="working dist."
            value={p.working_distance_m}
            scale={1000}
            unit="mm"
            precision={0}
            range={[50, 2000]}
            step={5}
            live={live(p, onChange, withWorkingDistance)}
            testId="working-distance"
          />
          {!p.converge && (
            <SliderRow
              label="φ tilt"
              value={p.tilt_rad}
              scale={DEG}
              unit="°"
              precision={2}
              range={[0, 60]}
              step={0.1}
              live={live(p, onChange, (q, v) => ({ ...q, tilt_rad: v }))}
              testId="tilt"
            />
          )}
          {!stereo && (
            <SliderRow
              label="phase"
              value={p.phase_rad}
              scale={DEG}
              unit="°"
              precision={1}
              range={[-180, 180]}
              step={1}
              live={live(p, onChange, (q, v) => ({ ...q, phase_rad: v }))}
            />
          )}
          {!stereo && (
            <>
              <RowLabel>image x</RowLabel>
              <Select
                aria-label="Image x direction"
                value={p.image_x}
                options={[
                  { value: "tangential", label: "Along the ring" },
                  { value: "radial", label: "Away from the axis" },
                ]}
                onValueChange={(v) => (v === "tangential" || v === "radial") && onChange({ ...p, image_x: v })}
              />
            </>
          )}
        </Rows>
        <Switch
          checked={p.converge}
          onCheckedChange={(on) => onChange({ ...p, converge: on, tilt_rad: tilt })}
          label={stereo ? "Converge at the working distance" : "Axes meet at the working distance"}
          description={
            p.converge
              ? `φ = atan(ρ / d) = ${(tilt * DEG).toFixed(2)}°`
              : stereo
                ? "Off with φ = 0 for a parallel pair."
                : "φ set by hand."
          }
        />
      </Fold>
      <Fold title="Coverage" summary={`${mm(2 * p.box.half_m)} box · ≤ ${p.max_blur_px} px · ${p.resolution}³`}>
        <Rows>
          <SliderRow
            label="width"
            value={2 * p.box.half_m}
            scale={1000}
            unit="mm"
            precision={0}
            range={[20, 1500]}
            step={5}
            live={live(p, onChange, (q, v) => ({ ...q, box: { ...q.box, half_m: v / 2 } }))}
          />
          <SliderRow
            label="near"
            value={p.box.near_m}
            scale={1000}
            unit="mm"
            precision={0}
            range={[10, 2000]}
            step={5}
            live={live(p, onChange, (q, v) => ({ ...q, box: { ...q.box, near_m: Math.min(v, q.box.far_m - 0.01) } }))}
          />
          <SliderRow
            label="far"
            value={p.box.far_m}
            scale={1000}
            unit="mm"
            precision={0}
            range={[20, 3000]}
            step={5}
            live={live(p, onChange, (q, v) => ({ ...q, box: { ...q.box, far_m: Math.max(v, q.box.near_m + 0.01) } }))}
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
          <RowLabel>voxels</RowLabel>
          <SegmentedControl
            aria-label="Voxels per axis"
            size="sm"
            value={String(p.resolution)}
            options={[32, 48, 64].map((n) => ({ value: String(n), label: `${n}³` }))}
            onValueChange={(v) => v !== "" && onChange({ ...p, resolution: Number(v) })}
          />
        </Rows>
      </Fold>
      <div className="flex flex-wrap gap-1.5 px-3 pt-2">
        <SaveButton onSave={onSave && (() => onSave(p))} disabled={refused || p.id === ""} />
        {onPlace && (
          <Button
            size="sm"
            variant="ghost"
            icon={<PackagePlus className="size-3.5" />}
            disabled={refused}
            onClick={() => onPlace(p)}
            title="Add this rig, its cameras and lasers to the open cell"
          >
            Place in cell
          </Button>
        )}
      </div>
    </div>
  );
}

const cm3 = (m3: number) => `${(m3 * 1e6).toFixed(m3 * 1e6 < 10 ? 2 : 0)} cm³`;

/** The rig's figures: its geometry and what it covers. */
export function RigReadouts({
  params,
  analysis,
  show,
  onShow,
}: {
  params: RigParams;
  analysis: RigAnalysis | string;
  show: VoxelShow;
  onShow: (s: VoxelShow) => void;
}) {
  if (typeof analysis === "string") {
    return (
      <div className="p-3">
        <Callout tone="warning" title="The kernel refused this rig">
          <p>{analysis}</p>
        </Callout>
      </div>
    );
  }
  const { volumes, cameras, lasers, grid } = analysis;
  const k = cameras.length;
  const voxel = (grid.max[0] - grid.min[0]) / grid.n[0];
  const showValue = show.kind === "measured" ? "measured" : String(show.min);
  return (
    <div className="pb-3" data-testid="rig-readouts">
      <Fold
        title="Geometry"
        summary={`${k} camera${k === 1 ? "" : "s"}${lasers.length ? ` · ${lasers.length} lasers` : ""}`}
      >
        <div className="grid grid-cols-[minmax(0,1fr)_auto] gap-x-3 gap-y-1">
          <Row label={params.arrangement === "stereo" ? "baseline" : "neighbours"}>{mm(analysis.neighbour_m)}</Row>
          <Row label="φ to the axis">{(tiltOf(params) * DEG).toFixed(2)}°</Row>
          <Row label="between axes">{(analysis.axisAngle * DEG).toFixed(2)}°</Row>
          <Row label="axes meet">
            {tiltOf(params) > 1e-9 ? mm(params.radius_m / Math.tan(tiltOf(params))) : "never (parallel)"}
          </Row>
        </div>
      </Fold>
      <Fold title="Coverage" summary={`${cm3(volumes.seen[1])} by 2+`}>
        <div className="grid grid-cols-[minmax(0,1fr)_auto] gap-x-3 gap-y-1">
          <Row label="seen by 1+">{cm3(volumes.seen[0])}</Row>
          <Row label="seen by 2+">
            <span data-testid="rig-seen-2">{cm3(volumes.seen[1])}</span>
          </Row>
          <Row label={`seen by all ${k}`}>{cm3(volumes.seen[2])}</Row>
          {lasers.length > 0 && (
            <>
              <Row label="measured">
                <span data-testid="rig-measured">{cm3(volumes.measured[0])}</span>
              </Row>
              <Row label="by 2+ pairs">{cm3(volumes.measured[1])}</Row>
            </>
          )}
          <Row label="voxel">{(voxel * 1000).toFixed(1)} mm</Row>
        </div>
        <RowLabel>show</RowLabel>
        <SegmentedControl
          aria-label="Voxels shown"
          size="sm"
          value={showValue}
          options={[
            { value: "1", label: "1+" },
            { value: "2", label: "2+" },
            { value: String(Math.max(1, k)), label: `all ${k}` },
            ...(lasers.length > 0 ? [{ value: "measured", label: "measured" }] : []),
          ].filter((o, i, all) => all.findIndex((x) => x.value === o.value) === i)}
          onValueChange={(v) =>
            v !== "" && onShow(v === "measured" ? { kind: "measured" } : { kind: "views", min: Number(v) })
          }
        />
      </Fold>
    </div>
  );
}

/** Volume seen by two or more cameras (and measured, with lasers) against the tilt φ. */
export function TiltChart({ params }: { params: RigParams }) {
  const deferred = useDeferredValue(params);
  const sweep = useMemo(() => tiltSweep(deferred, [0, Math.PI / 4]), [deferred]);
  const withLasers = deferred.unit.laser !== null;
  const series = useMemo<Series[]>(
    () => [
      { name: "seen by 2+", points: sweep.map((s) => ({ x: s.tilt * DEG, y: s.seen2 * 1e6 })) },
      ...(withLasers
        ? [{ name: "measured", points: sweep.map((s) => ({ x: s.tilt * DEG, y: s.measured * 1e6 })) }]
        : []),
    ],
    [sweep, withLasers],
  );
  const now = tiltOf(params) * DEG;
  return (
    <LineChart
      series={series}
      label="Coverage against the tilt φ"
      xLabel="φ (°)"
      yLabel="cm³"
      variant="fluid"
      className="h-full"
      underlay={(x, y) => (
        <line
          x1={x.project(now)}
          x2={x.project(now)}
          y1={y.project(y.domain[0])}
          y2={y.project(y.domain[1])}
          className="stroke-signal"
          strokeWidth={1.5}
        />
      )}
    />
  );
}
