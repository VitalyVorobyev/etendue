/**
 * A run's parameters over its trials: each one's truth, mean, bias and spread beside
 * calibration-rs's predicted σ, and for a camera parameter the histogram of its trials' values
 * with the truth and the ±σ band around it.
 */

import { Histogram } from "@vitavision/charts";
import { Table } from "@vitavision/ui";
import { useState } from "react";

import type { CalibrationReport, ParamStats } from "../kernel/native";
import { SIGMA_RATIO, binValues, sigmaAgreement, trialValues } from "./params";

/** A value at a readable precision: fixed where it reads, scientific when very small or large. */
export function formatValue(v: number | undefined): string {
  if (v === undefined || !Number.isFinite(v)) return "—";
  if (v === 0) return "0";
  const a = Math.abs(v);
  return a < 1e-3 || a >= 1e5 ? v.toExponential(3) : Number(v.toPrecision(5)).toString();
}

function Ratio({ stat }: { stat: ParamStats }) {
  const sigma = stat.predicted_sigma ?? undefined;
  const agreement = sigmaAgreement(stat.std, sigma);
  if (agreement === null || sigma === undefined) return <span className="text-fg-muted">—</span>;
  const ratio = (stat.std / sigma).toFixed(2);
  return agreement === "ok" ? (
    <span data-agreement="ok">{ratio}</span>
  ) : (
    <span
      className="rounded-sm bg-warn/15 px-1 text-warn"
      data-agreement={agreement}
      title={
        agreement === "low"
          ? "The trials scatter less than calibration-rs predicts"
          : "The trials scatter more than calibration-rs predicts"
      }
    >
      {ratio}
    </span>
  );
}

function ParamHistogram({ stat, values }: { stat: ParamStats; values: number[] }) {
  const sigma = stat.predicted_sigma ?? undefined;
  const band = sigma !== undefined ? [stat.truth - sigma, stat.truth + sigma] : [];
  const bins = binValues(values, Math.max(5, Math.min(24, 2 * Math.ceil(Math.sqrt(values.length)))), [
    stat.truth,
    ...band,
  ]);
  const unit = stat.unit === "" ? undefined : stat.unit;
  return (
    <div data-testid="param-histogram" data-param={stat.label}>
      <Histogram
        counts={bins.counts}
        domain={[bins.lo, bins.hi]}
        label={`${stat.label} over trials`}
        xLabel={stat.label}
        unit={unit}
        yLabel="trials"
        threshold={stat.truth}
        bands={sigma !== undefined ? [{ from: band[0]!, to: band[1]!, label: "truth ± predicted σ" }] : []}
        markers={[{ position: stat.mean, tone: "warn", label: `mean ${formatValue(stat.mean)}` }]}
        variant="fluid"
        height={200}
        footer={`${values.length} trials. Dashed: the truth, ${formatValue(stat.truth)}; tick: the mean, ${formatValue(stat.mean)}${
          sigma !== undefined ? `; band: the truth ± calibration-rs's predicted σ, ${formatValue(sigma)}` : ""
        }.`}
      />
    </div>
  );
}

export function ParamStatsView({ stats, reports }: { stats: ParamStats[]; reports: readonly CalibrationReport[] }) {
  const [selected, setSelected] = useState<string | null>(null);
  const shown = stats.find((s) => s.label === selected) ?? stats.find((s) => trialValues(reports, s.label)) ?? null;
  const values = shown ? trialValues(reports, shown.label) : null;
  const trials = reports.length;
  const first = reports[0];
  // calibration-rs names cameras by index (`cam0.k3`); the report's cameras give their ids.
  const fixed = (first?.uncertainty?.fixed ?? []).map((l) =>
    l.replace(/^cam(\d+)\./, (m, i: string) => `${first?.cameras[Number(i)] ?? m.slice(0, -1)} `),
  );
  return (
    <div className="flex min-w-0 flex-col gap-3" data-testid="param-stats">
      <p className="text-xs text-fg-muted">
        Each parameter over {trials} trials, beside its truth and calibration-rs's predicted σ. With noise like the
        model's, std/σ is near 1; outside {SIGMA_RATIO.low}–{SIGMA_RATIO.high} it is tinted. With {trials} trials the
        ratio itself is uncertain by about {Math.round(100 / Math.sqrt(2 * Math.max(1, trials - 1)))} %. Hand-eye values
        are errors from the truth, in the transform's own frame.
      </p>
      <div className="max-h-96 overflow-auto">
        <Table
          columns={[
            { key: "label", header: "Parameter", cell: (s: ParamStats) => s.label, sortValue: (s) => s.label },
            { key: "unit", header: "Unit", cell: (s: ParamStats) => s.unit },
            { key: "truth", header: "Truth", numeric: true, cell: (s: ParamStats) => formatValue(s.truth) },
            { key: "mean", header: "Mean", numeric: true, cell: (s: ParamStats) => formatValue(s.mean) },
            {
              key: "bias",
              header: "Bias",
              numeric: true,
              cell: (s: ParamStats) => formatValue(s.bias),
              sortValue: (s) => Math.abs(s.bias),
              firstSort: "descending",
            },
            {
              key: "std",
              header: "Std",
              numeric: true,
              cell: (s: ParamStats) => formatValue(s.std),
            },
            {
              key: "sigma",
              header: "Predicted σ",
              numeric: true,
              cell: (s: ParamStats) => formatValue(s.predicted_sigma ?? undefined),
            },
            {
              key: "ratio",
              header: "Std/σ",
              numeric: true,
              cell: (s: ParamStats) => <Ratio stat={s} />,
              sortValue: (s) => (s.predicted_sigma ? s.std / s.predicted_sigma : null),
              firstSort: "descending",
            },
          ]}
          rows={stats}
          rowKey={(s) => s.label}
          onRowClick={(s) => setSelected(s.label)}
          isRowActive={(s) => s.label === shown?.label}
          caption="Parameters over trials"
        />
      </div>
      {fixed.length > 0 && (
        <p className="text-xs text-fg-muted" data-testid="fixed-params">
          The predicted σ are conditional on what the solve held fixed:{" "}
          <span className="font-mono">{fixed.join(", ")}</span>.
        </p>
      )}
      {shown && values ? (
        <ParamHistogram stat={shown} values={values} />
      ) : (
        shown && (
          <p className="text-xs text-fg-muted">
            {shown.label}: the histogram covers the cameras' intrinsics, distortion and tilt.
          </p>
        )
      )}
    </div>
  );
}
