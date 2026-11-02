/**
 * How a run was solved: its procedure and stages, and calibration-rs's own verdict on the
 * result with the checks behind it.
 */

import { Badge, Disclosure, Table } from "@vitavision/ui";

import type {
  CalibrationReport,
  CalibrationWorkflow,
  QualityCheck,
  QualityVerdict,
  StageSummary,
} from "../kernel/native";

export const WORKFLOW: Record<CalibrationWorkflow, string> = {
  single_cam_handeye: "single-camera hand-eye",
  rig_handeye: "rig hand-eye",
  rig_handeye_laserline: "rig hand-eye laserline",
  laserline_device: "laserline device + hand-eye",
  planar_intrinsics: "planar intrinsics",
  rig_extrinsics: "rig extrinsics",
};

const VERDICT: Record<QualityCheck["status"], { text: string; tone: "normal" | "warning" | "defect" | "neutral" }> = {
  pass: { text: "Pass", tone: "normal" },
  warn: { text: "Warn", tone: "warning" },
  fail: { text: "Fail", tone: "defect" },
  not_applicable: { text: "n/a", tone: "neutral" },
};

/** calibration-rs's check kinds, for people. */
const CHECK: Record<string, string> = {
  observability: "Observability",
  convergence: "Convergence",
  variance_factor: "Noise level",
  projection_sigma: "Projection σ",
  feature_coverage: "Image coverage",
  view_outliers: "Outlier views",
  parameter_correlation: "Parameter correlation",
};

/** calibration-rs's verdict as a chip. */
export function VerdictBadge({ verdict }: { verdict: QualityVerdict }) {
  const v = VERDICT[verdict];
  return (
    <span data-testid="quality-verdict" data-verdict={verdict}>
      <Badge tone={v.tone}>calibration-rs: {v.text}</Badge>
    </span>
  );
}

/** How many trials got each verdict, e.g. "2 pass, 1 warn". */
export function verdictCounts(reports: readonly CalibrationReport[]): string {
  const counts: Partial<Record<QualityVerdict, number>> = {};
  for (const r of reports) if (r.quality) counts[r.quality.verdict] = (counts[r.quality.verdict] ?? 0) + 1;
  return (["pass", "warn", "fail"] as const)
    .filter((v) => counts[v])
    .map((v) => `${counts[v]} ${v}`)
    .join(", ");
}

function Check({ check, cameras }: { check: QualityCheck; cameras: readonly string[] }) {
  const v = VERDICT[check.status];
  const camera = check.camera !== undefined ? (cameras[check.camera] ?? `camera ${check.camera}`) : null;
  return (
    <li className="flex flex-col gap-0.5 border-t border-line py-1.5 first:border-t-0" data-status={check.status}>
      <div className="flex flex-wrap items-center gap-2">
        <Badge tone={v.tone}>{v.text}</Badge>
        <span className="font-medium">{CHECK[check.kind] ?? check.kind}</span>
        {camera && <span className="text-fg-muted">{camera}</span>}
        {check.measurement && (
          <span className="font-mono text-fg-muted">
            {Number(check.measurement.value.toPrecision(4))} (limit {check.measurement.threshold})
          </span>
        )}
      </div>
      <p className="text-fg-muted">{check.message}</p>
      {check.subjects && check.subjects.length > 0 && (
        <p className="font-mono text-fg-muted">{check.subjects.join(", ")}</p>
      )}
    </li>
  );
}

export function SolverView({ reports }: { reports: readonly CalibrationReport[] }) {
  const first = reports[0];
  if (!first) return null;
  const stages = first.stages ?? [];
  const quality = first.quality;
  return (
    <div className="flex min-w-0 flex-col gap-3 text-xs">
      <div data-testid="stages" className="flex flex-col gap-2">
        <p>
          <span className="text-fg-muted">Procedure </span>
          <span className="font-mono">{first.procedure ?? "auto"}</span>
          <span className="text-fg-muted">
            {" "}
            · {stages.length} stage{stages.length === 1 ? "" : "s"}
          </span>
        </p>
        {stages.length > 0 && (
          <Table
            columns={[
              { key: "id", header: "Stage", cell: (s: StageSummary) => <span className="font-mono">{s.id}</span> },
              { key: "workflow", header: "Workflow", cell: (s: StageSummary) => WORKFLOW[s.workflow] },
              { key: "cameras", header: "Cameras", cell: (s: StageSummary) => s.cameras.join(", ") },
              { key: "views", header: "Views", numeric: true, cell: (s: StageSummary) => s.views },
              { key: "points", header: "Points", numeric: true, cell: (s: StageSummary) => s.points },
              {
                key: "reproj",
                header: "Mean reprojection px",
                numeric: true,
                cell: (s: StageSummary) => s.mean_reproj_px.toFixed(4),
              },
            ]}
            rows={stages}
            rowKey={(s, i) => `${i}-${s.id}`}
            caption="Stages"
          />
        )}
      </div>
      {quality ? (
        <div className="flex flex-col gap-2" data-testid="quality">
          <div className="flex flex-wrap items-center gap-2">
            <VerdictBadge verdict={quality.verdict} />
            {reports.length > 1 && (
              <span className="text-fg-muted">
                first trial; over {reports.length} trials: {verdictCounts(reports)}
              </span>
            )}
          </div>
          <Disclosure
            summary="calibration-rs's checks of the result"
            count={quality.checks.length}
            defaultOpen={quality.verdict !== "pass"}
          >
            <ul className="flex flex-col" data-testid="quality-checks">
              {quality.checks.map((c) => (
                <Check key={`${c.kind}-${c.camera ?? "all"}`} check={c} cameras={first.cameras} />
              ))}
            </ul>
          </Disclosure>
        </div>
      ) : (
        <p className="text-fg-muted">calibration-rs gave no quality verdict for this run.</p>
      )}
    </div>
  );
}
