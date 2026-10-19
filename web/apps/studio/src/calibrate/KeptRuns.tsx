/**
 * The runs kept for a dataset (ADR 0009 `runs/<name>/`), newest first: what each calibrated
 * from and how it came out. Showing one puts it in the result view, the 3-D estimate included.
 */

import { Badge, Button, Table } from "@vitavision/ui";
import { useEffect, useState } from "react";

import { type StoredRun, calibrationRuns } from "../kernel/native";
import { describeRun, samePath } from "./runs";

function outcome(r: StoredRun): { text: string; tone: "normal" | "defect" | "neutral" } {
  const first = r.run.reports[0];
  if (!first) return { text: "empty", tone: "neutral" };
  if (first.gated)
    return { text: `${first.gate} ${first.pass ? "PASS" : "FAIL"}`, tone: first.pass ? "normal" : "defect" };
  return { text: `${first.residuals.rms_axis_px.toPrecision(3)} px rms`, tone: "neutral" };
}

export function KeptRuns({
  folder,
  dataset,
  kept,
  shown,
  onShow,
}: {
  /** Where runs are kept. */
  folder: string;
  /** The dataset whose runs are listed. */
  dataset: string;
  /** Runs kept this session: a change looks again. */
  kept: number;
  /** The name of the run shown, if it is one of these. */
  shown: string | null;
  onShow: (run: StoredRun) => void;
}) {
  const [listed, setListed] = useState<{ key: string; runs: StoredRun[] } | null>(null);
  const key = `${folder}\n${kept}`;
  useEffect(() => {
    let live = true;
    void calibrationRuns(folder)
      .catch(() => [])
      .then((runs) => live && setListed({ key, runs }));
    return () => {
      live = false;
    };
  }, [folder, key]);
  const runs = (listed?.runs ?? []).filter((r) => samePath(r.config.dataset, dataset)).reverse();
  if (runs.length === 0) return null;
  return (
    <div className="flex flex-col gap-1" data-testid="kept-runs">
      <Table
        columns={[
          { key: "name", header: "Run", cell: (r: StoredRun) => <span className="font-mono text-xs">{r.name}</span> },
          { key: "from", header: "Observations", cell: (r: StoredRun) => describeRun(r.config.options) },
          {
            key: "outcome",
            header: "Outcome",
            cell: (r: StoredRun) => {
              const o = outcome(r);
              return <Badge tone={o.tone}>{o.text}</Badge>;
            },
          },
          {
            key: "show",
            header: "",
            cell: (r: StoredRun) =>
              r.name === shown ? (
                <span className="text-xs text-fg-muted">shown</span>
              ) : (
                <Button size="sm" variant="ghost" onClick={() => onShow(r)}>
                  Show
                </Button>
              ),
          },
        ]}
        rows={runs}
        rowKey={(r) => r.name}
        caption={`Kept in ${folder}`}
      />
    </div>
  );
}
