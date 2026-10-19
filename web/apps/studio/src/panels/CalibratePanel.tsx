/**
 * The Calibrate workspace's second half (P13): calibrate the generated dataset natively with
 * calibration-rs and compare every result with the ground truth. The observations are the
 * ground truth's (exact, or with controlled noise over several trials) or the detector's.
 */

import { Histogram } from "@vitavision/charts";
import {
  Badge,
  Button,
  Callout,
  Field,
  NumberInput,
  ProgressBar,
  SegmentedControl,
  Select,
  Table,
} from "@vitavision/ui";
import { Play, Square } from "lucide-react";
import { useState } from "react";

import type {
  CalibrateOptions,
  CalibrationReport,
  CalibrationRow,
  Dataset,
  ObservationNoise,
  RowStats,
} from "../kernel/native";
import { type CalibrationRunner, useCalibrationRun } from "../state/calibrationRun";

export type Source = "exact" | "noisy" | "detected";

const DEG = Math.PI / 180;

const WORKFLOW: Record<CalibrationReport["workflow"], string> = {
  single_cam_handeye: "single-camera hand-eye",
  rig_handeye: "rig hand-eye",
  rig_handeye_laserline: "rig hand-eye laserline",
  laserline_device: "laserline device + hand-eye",
};

/** The noise form, in display units (degrees and millimetres for the robot). */
export interface NoiseForm {
  pixel: number;
  outliers: number;
  outlierSigma: number;
  dropouts: number;
  laser: number;
  robotDeg: number;
  robotMm: number;
  seed: number;
  trials: number;
}

/** What the panel calibrates from; the studio keeps it so the views can preview it. */
export interface CalibrateForm {
  source: Source;
  noise: NoiseForm;
}

const NOISE: NoiseForm = {
  pixel: 0.5,
  outliers: 0,
  outlierSigma: 20,
  dropouts: 0,
  laser: 0.2,
  robotDeg: 0,
  robotMm: 0,
  seed: 1,
  trials: 1,
};

export const CALIBRATE_FORM: CalibrateForm = { source: "exact", noise: NOISE };

/** How much the 3-D view exaggerates the estimate's offsets from the truth. */
export const ESTIMATE_SCALES = [1, 10, 100, 1000] as const;

export function noiseOf(f: NoiseForm): ObservationNoise {
  return {
    pixel_sigma_px: f.pixel,
    outlier_fraction: f.outliers / 100,
    outlier_sigma_px: f.outlierSigma,
    dropout_fraction: f.dropouts / 100,
    laser_sigma_px: f.laser,
    robot_rot_sigma_rad: f.robotDeg * DEG,
    robot_trans_sigma_m: f.robotMm / 1000,
    seed: f.seed,
  };
}

const sci = (v: number) => (Number.isFinite(v) ? v.toExponential(3) : "—");
/** Pixels: fixed where readable, scientific below a thousandth. */
const px = (v: number) =>
  !Number.isFinite(v) ? "—" : v !== 0 && Math.abs(v) < 1e-3 ? v.toExponential(2) : v.toFixed(4);

function verdict(row: CalibrationRow, gated: boolean): { text: string; tone: "normal" | "defect" | "warning" } | null {
  if (row.limit === undefined) return null;
  const within = row.value <= row.limit;
  if (gated) return within ? { text: "PASS", tone: "normal" } : { text: "FAIL", tone: "defect" };
  return within ? null : { text: "outside", tone: "warning" };
}

function RowsTable({ report }: { report: CalibrationReport }) {
  return (
    <Table
      columns={[
        { key: "label", header: "Quantity", cell: (r: CalibrationRow) => r.label },
        { key: "value", header: "Error", numeric: true, cell: (r: CalibrationRow) => sci(r.value) },
        {
          key: "limit",
          header: "Limit",
          numeric: true,
          cell: (r: CalibrationRow) => (r.limit === undefined ? "" : `≤ ${r.limit}`),
        },
        {
          key: "verdict",
          header: "",
          cell: (r: CalibrationRow) => {
            const v = verdict(r, report.gated);
            return v ? <Badge tone={v.tone}>{v.text}</Badge> : null;
          },
        },
      ]}
      rows={report.rows}
      rowKey={(r) => r.label}
      caption="Calibrated against the ground truth"
    />
  );
}

function StatsTable({ stats, trials }: { stats: RowStats[]; trials: number }) {
  return (
    <Table
      columns={[
        { key: "label", header: "Quantity", cell: (r: RowStats) => r.label },
        { key: "mean", header: "Mean", numeric: true, cell: (r: RowStats) => sci(r.mean) },
        { key: "std", header: "Std", numeric: true, cell: (r: RowStats) => sci(r.std) },
        { key: "max", header: "Max", numeric: true, cell: (r: RowStats) => sci(r.max) },
        {
          key: "limit",
          header: "Limit",
          numeric: true,
          cell: (r: RowStats) => (r.limit === undefined ? "" : `≤ ${r.limit}`),
        },
      ]}
      rows={stats}
      rowKey={(r) => r.label}
      caption={`Spread over ${trials} trials`}
    />
  );
}

export function CalibratePanel({
  dataset,
  cameras,
  lasers,
  runner,
  form,
  onForm,
  scale,
  onScale,
}: {
  dataset: Dataset;
  cameras: string[];
  lasers: boolean;
  runner: CalibrationRunner;
  form: CalibrateForm;
  onForm: (form: CalibrateForm) => void;
  /** The 3-D view's exaggeration of the estimate. */
  scale: number;
  onScale: (scale: number) => void;
}) {
  const { source, noise } = form;
  const setSource = (s: Source) => onForm({ ...form, source: s });
  const [camera, setCamera] = useState<string>("");
  const { running, result, failure } = useCalibrationRun(runner);
  const detectedAvailable = dataset.render !== null && dataset.detection.length > 0;
  const set = (patch: Partial<NoiseForm>) => onForm({ ...form, noise: { ...noise, ...patch } });

  const options = (): CalibrateOptions => ({
    ...(source === "noisy" ? { noise: noiseOf(noise), trials: Math.max(1, Math.round(noise.trials)) } : {}),
    ...(source === "detected" ? { detected: true } : {}),
    ...(camera !== "" ? { camera } : {}),
  });
  const shown = result?.dir === dataset.output ? result.run : null;
  const first = shown?.reports[0] ?? null;
  const num = (label: string, value: number, onChange: (v: number) => void, unit: string, step = 0.1, min = 0) => (
    <Field label={label} annotation={unit}>
      <NumberInput value={value} min={min} step={step} onValueChange={onChange} disabled={running !== null} />
    </Field>
  );

  return (
    <div className="@container flex min-w-0 flex-col gap-3" data-testid="calibrate-panel">
      <div className="flex flex-wrap items-end gap-3">
        <Field label="Observations">
          <SegmentedControl
            aria-label="Observations"
            value={source}
            options={[
              { value: "exact", label: "Exact" },
              { value: "noisy", label: "With noise" },
              ...(detectedAvailable ? [{ value: "detected", label: "Detected" }] : []),
            ]}
            onValueChange={(v) => (v === "exact" || v === "noisy" || v === "detected") && setSource(v)}
          />
        </Field>
        {!lasers && cameras.length > 1 && (
          <Field label="Cameras">
            <Select
              aria-label="Calibrate cameras"
              value={camera === "" ? "__rig" : camera}
              options={[
                { value: "__rig", label: "The rig" },
                ...cameras.map((c) => ({ value: c, label: `${c} alone` })),
              ]}
              onValueChange={(v) => setCamera(v === "__rig" ? "" : v)}
            />
          </Field>
        )}
        {running ? (
          <Button icon={<Square className="size-4" />} variant="ghost" onClick={runner.cancel}>
            Cancel
          </Button>
        ) : (
          <Button icon={<Play className="size-4" />} onClick={() => runner.start(dataset.output, options())}>
            Calibrate
          </Button>
        )}
      </div>
      {source === "noisy" && (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(9rem,1fr))] gap-3" data-testid="noise-form">
          {num("Pixel σ", noise.pixel, (v) => set({ pixel: v }), "px")}
          {num("Outliers", noise.outliers, (v) => set({ outliers: Math.min(100, v) }), "%", 1)}
          {num("Outlier σ", noise.outlierSigma, (v) => set({ outlierSigma: v }), "px", 1)}
          {num("Dropouts", noise.dropouts, (v) => set({ dropouts: Math.min(100, v) }), "%", 1)}
          {lasers && num("Laser σ", noise.laser, (v) => set({ laser: v }), "px")}
          {num("Robot rotation σ", noise.robotDeg, (v) => set({ robotDeg: v }), "°", 0.01)}
          {num("Robot translation σ", noise.robotMm, (v) => set({ robotMm: v }), "mm", 0.01)}
          {num("Seed", noise.seed, (v) => set({ seed: Math.round(v) }), "", 1)}
          {num("Trials", noise.trials, (v) => set({ trials: Math.max(1, Math.round(v)) }), "", 1, 1)}
        </div>
      )}
      {running && (
        <ProgressBar
          fraction={running.total > 0 ? running.done / running.total : 0}
          label={`Calibrating: ${running.done} of ${running.total}`}
          aria-label="Calibration"
        />
      )}
      {failure && (
        <Callout tone="error" title="Calibration failed">
          <p className="font-mono text-xs whitespace-pre-wrap">{failure}</p>
        </Callout>
      )}
      {first && shown && (
        <div className="flex flex-col gap-3" data-testid="calibration-result">
          <div className="flex flex-wrap items-center gap-2 text-sm">
            <span className="font-medium">{WORKFLOW[first.workflow]}</span>
            <span className="text-fg-muted">
              {first.cameras.join(", ")} · {first.views} views · {first.points} points
              {first.laser_pixels > 0 ? ` · ${first.laser_pixels} laser pixels` : ""}
            </span>
            {first.gated ? (
              <Badge tone={first.pass ? "normal" : "defect"}>
                {first.gate} {first.pass ? "PASS" : "FAIL"}
              </Badge>
            ) : (
              <Badge tone="neutral">{first.gate} limits for reference</Badge>
            )}
          </div>
          <p className="font-mono text-xs text-fg-muted" data-testid="calibration-residuals">
            reprojection: mean {px(first.residuals.mean_px)} px · rms per axis {px(first.residuals.rms_axis_px)} px ·
            max {px(first.residuals.max_px)} px over {first.residuals.count} points
          </p>
          {first.estimate.offsets && (
            <div className="flex flex-wrap items-center gap-2 text-xs text-fg-muted" data-testid="estimate-scale">
              <span>The 3-D view draws the estimate over the truth in the warning colour, its errors times</span>
              <SegmentedControl
                aria-label="Exaggeration"
                value={String(scale)}
                options={ESTIMATE_SCALES.map((s) => ({ value: String(s), label: `×${s}` }))}
                onValueChange={(v) => onScale(Number(v))}
              />
            </div>
          )}
          <div className="grid gap-4 @4xl:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]">
            <div className="min-w-0">
              {shown.row_stats ? (
                <StatsTable stats={shown.row_stats} trials={shown.reports.length} />
              ) : (
                <RowsTable report={first} />
              )}
            </div>
            <Histogram
              values={first.residuals.errors_px}
              bins={40}
              label="Reprojection errors"
              xLabel="error"
              unit="px"
              yLabel="points"
              variant="fluid"
              height={220}
              footer={`${shown.reports.length > 1 ? "First trial. " : ""}Mean per camera: ${first.residuals.per_camera_mean_px
                .map((m, i) => `${first.cameras[i] ?? i} ${px(m)} px`)
                .join(", ")}`}
            />
          </div>
        </div>
      )}
      {!first && !running && !failure && (
        <p className="text-xs text-fg-muted">
          Calibrates this dataset with calibration-rs, natively, and compares every intrinsic, rig transform, hand-eye
          and laser plane with the ground truth. On exact observations the closed-loop gate applies (G5.1, G5.3 with
          lasers).
        </p>
      )}
    </div>
  );
}
