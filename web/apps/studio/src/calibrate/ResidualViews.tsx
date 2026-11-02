/**
 * The first trial's residuals per view: a capture seen by a camera, its points, the RMS and the
 * largest of their residuals. Choosing a view shows it in the camera view, its residuals drawn
 * over the image.
 */

import { SegmentedControl, Table } from "@vitavision/ui";
import { useMemo } from "react";

import type { FeatureResidual } from "../kernel/native";
import { type ViewResiduals, groupViews, overallRms } from "./residuals";

/** How much the camera view magnifies the residual vectors. */
export const RESIDUAL_SCALES = [1, 10, 100, 1000] as const;

/** A chosen view: a capture and a camera. */
export interface ViewChoice {
  capture: string;
  camera: string;
}

const px = (v: number) =>
  !Number.isFinite(v) ? "—" : v !== 0 && Math.abs(v) < 1e-3 ? v.toExponential(2) : v.toFixed(3);

export function ResidualViews({
  features,
  trials,
  selected,
  onSelect,
  scale,
  onScale,
}: {
  features: readonly FeatureResidual[];
  trials: number;
  selected: ViewChoice | null;
  onSelect: (view: ViewChoice) => void;
  /** The camera view's magnification of the residuals. */
  scale: number;
  onScale: (scale: number) => void;
}) {
  const views = useMemo(() => groupViews(features), [features]);
  const rms = useMemo(() => overallRms(features), [features]);
  return (
    <div className="flex min-w-0 flex-col gap-3" data-testid="residual-views">
      <p className="text-xs text-fg-muted">
        {trials > 1 ? "The first trial's residuals" : "The residuals"}, {views.length} views; RMS {px(rms)} px over{" "}
        {features.length} points. Choose a view to see it in the camera view: each observed point is a dot, green within
        the RMS, amber up to twice it, red beyond; a cross is where the estimate projects it, and the line between them
        is the residual, magnified.
      </p>
      <div className="flex flex-wrap items-center gap-2 text-xs text-fg-muted" data-testid="residual-scale">
        <span>Residuals drawn times</span>
        <SegmentedControl
          aria-label="Residual magnification"
          value={String(scale)}
          options={RESIDUAL_SCALES.map((s) => ({ value: String(s), label: `×${s}` }))}
          onValueChange={(v) => onScale(Number(v))}
        />
      </div>
      <div className="max-h-96 overflow-auto">
        <Table
          columns={[
            {
              key: "capture",
              header: "Capture",
              cell: (v: ViewResiduals) => <span className="font-mono text-xs">{v.capture}</span>,
              sortValue: (v) => v.capture,
            },
            { key: "camera", header: "Camera", cell: (v: ViewResiduals) => v.camera, sortValue: (v) => v.camera },
            { key: "count", header: "Points", numeric: true, cell: (v: ViewResiduals) => v.count },
            {
              key: "rms",
              header: "RMS px",
              numeric: true,
              cell: (v: ViewResiduals) => px(v.rms),
              sortValue: (v) => v.rms,
              firstSort: "descending",
            },
            {
              key: "max",
              header: "Max px",
              numeric: true,
              cell: (v: ViewResiduals) => px(v.max),
              sortValue: (v) => v.max,
              firstSort: "descending",
            },
          ]}
          rows={views}
          rowKey={(v) => `${v.capture}/${v.camera}`}
          onRowClick={(v) => onSelect({ capture: v.capture, camera: v.camera })}
          isRowActive={(v) => v.capture === selected?.capture && v.camera === selected.camera}
          caption="Residuals per view"
        />
      </div>
    </div>
  );
}
