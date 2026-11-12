/**
 * The Calibrate workspace: the dataset (generated in the desktop app, previewed in a browser), its
 * calibration against the truth, and what a calibration observes in each camera, or the residuals
 * of the last one. The 3-D view between them is the studio's; this draws the estimate in it.
 */

import type { FrameTreeRuntime } from "@vitavision/three";
import { SegmentedControl, Select } from "@vitavision/ui";
import type { Playhead } from "@vitavision/workbench";
import { type ReactNode, useCallback, useMemo, useState } from "react";

import { type Bank, type Project, itemsOf } from "../io/project";
import { isTauri } from "../io/tauri";
import type { Baked } from "../kernel/etendue";
import type { Dataset } from "../kernel/native";
import type { PoseSource } from "../kernel/posed";
import { CALIBRATE_FORM, type CalibrateForm, CalibratePanel, noiseOf } from "../panels/CalibratePanel";
import { CameraView } from "../panels/CameraView";
import { DatasetPanel } from "../panels/DatasetPanel";
import type { CalibrationRunner } from "../state/calibrationRun";
import { useCalibrationRun } from "../state/calibrationRun";
import type { DatasetRunner } from "../state/datasetRun";
import type { Studio } from "../state/studio";
import { estimateDrawing, observedAt as observedIn } from "./compare";
import { ObservationPreview } from "./ObservationPreview";
import { useObservations } from "./observations";
import type { ViewChoice } from "./ResidualViews";
import { useResiduals } from "./useResiduals";

export interface CalibrateWorkspace {
  /** The dataset and the calibration, beside the 3-D view. */
  panel: ReactNode;
  /** One camera's observations or residuals, under the 3-D view. */
  observations: ReactNode;
  /** The last calibration's estimate, to draw in the 3-D view. */
  estimate: ReturnType<typeof estimateDrawing> | null;
}

export function useCalibrateWorkspace({
  active,
  studio,
  current,
  dataset,
  project,
  bank,
  example,
  runner,
  calibration,
  runtime,
  poses,
  playhead,
  renderedAt,
}: {
  /** Whether the workspace shows: what it computes is computed only then. */
  active: boolean;
  studio: Studio;
  current: Baked | null;
  dataset: Dataset | null;
  project: Project | null;
  bank: Bank | null;
  /** The open cell's id. */
  example: string;
  runner: DatasetRunner;
  calibration: CalibrationRunner;
  runtime: FrameTreeRuntime | null;
  poses: PoseSource | null;
  playhead: Playhead;
  renderedAt: (camera: string, k: number) => string | null;
}): CalibrateWorkspace {
  // What it calibrates from, how much the 3-D view exaggerates the estimate, and the camera whose
  // observations it shows.
  const [form, setForm] = useState<CalibrateForm>(CALIBRATE_FORM);
  const [estimateScale, setEstimateScale] = useState(100);
  const [observedCamera, setObservedCamera] = useState<string | null>(null);
  const calibrationRun = useCalibrationRun(calibration);
  // A calibration's residuals in the camera view: which view was chosen, their magnification,
  // and whether the view shows them or the observations of the form.
  const [residualView, setResidualView] = useState<ViewChoice | null>(null);
  const [residualScale, setResidualScale] = useState(10);
  const [layer, setLayer] = useState<"observations" | "residuals">("residuals");

  const scene = current?.loaded.scene;
  const cameras = scene?.cameras ?? [];
  // What a calibration of the open scene observes: exact, or with the form's noise; a detector's
  // observations are not previewed.
  const observeNoise = useMemo(
    () => (!active ? null : form.source === "noisy" ? noiseOf(form.noise) : form.source === "exact" ? {} : null),
    [active, form],
  );
  const { observations, error: observeError } = useObservations(current, observeNoise);
  const observedAt = useCallback(
    (camera: string, k: number) => (current ? observedIn(observations, current.baked, camera, k) : null),
    [current, observations],
  );
  const shownCamera = cameras.find((c) => c.id === observedCamera) ?? cameras[0] ?? null;
  // A project opened for writing keeps its datasets and calibration runs (ADR 0009); otherwise a
  // dataset keeps its own runs.
  const projectFolder =
    project?.write && project.source.absolute ? project.source.absolute(project.root).replace(/[\\/]+$/, "") : null;
  const cellName = project?.cells.some((c) => c.id === example) ? example.split("/").at(-1) : undefined;
  const projectDatasets = projectFolder && cellName ? `${projectFolder}/datasets/${cellName}` : null;
  // The calibration procedures the project sees (its own, the bank's, the built-ins), and where an
  // edited one can be kept.
  const procedures = useMemo(() => new Map(project ? itemsOf(project, "procedure") : []), [project]);
  const saveProcedure = useMemo(
    () =>
      project?.write
        ? { where: "project", put: studio.saveLibraryItem }
        : bank?.write
          ? { where: "bank", put: studio.saveToBank }
          : null,
    [project, bank, studio],
  );
  // The last calibration of this dataset (its first trial), drawn over the truth in 3-D.
  const calibrated =
    dataset && calibrationRun.result?.dir === dataset.output ? (calibrationRun.result.run.reports[0] ?? null) : null;
  const estimate = useMemo(() => {
    if (!active || !calibrated || !current) return null;
    const extent = (id: string) => {
      try {
        return current.session.targetExtent(id) ?? null;
      } catch {
        return null;
      }
    };
    return estimateDrawing(calibrated, estimateScale, current.loaded.scene, current.baked, extent);
  }, [active, calibrated, current, estimateScale]);
  const residualsAt = useResiduals(calibrated, current?.baked ?? null, residualScale);
  const showResiduals = residualsAt !== undefined && layer === "residuals";
  // A view chosen in the residual table: its camera in the camera view, the playhead at its capture.
  const chooseView = (v: ViewChoice) => {
    const k = current?.baked.samples.findIndex((s) => s.capture?.id === v.capture) ?? -1;
    if (k >= 0) playhead.set(k);
    setObservedCamera(v.camera);
    setResidualView(v);
    setLayer("residuals");
  };

  if (!current) return { panel: null, observations: null, estimate: null };
  const lasers = (scene?.lasers ?? []).length > 0;
  const panel = (
    <div className="row-span-2 min-h-0 overflow-auto border-r border-line p-4">
      {isTauri() ? (
        <DatasetPanel current={current} dataset={dataset} runner={runner} projectFolder={projectDatasets} />
      ) : (
        <ObservationPreview form={form} onForm={setForm} lasers={lasers} observations={observations} />
      )}
      {dataset && (
        <section className="mt-6 border-t border-line pt-4" aria-label="Calibrate">
          <h2 className="mb-3 text-sm font-medium">Calibrate and compare</h2>
          <CalibratePanel
            dataset={dataset}
            cameras={cameras.map((c) => c.id)}
            lasers={lasers}
            runner={calibration}
            form={form}
            onForm={setForm}
            scale={estimateScale}
            onScale={setEstimateScale}
            runsFolder={projectFolder ? `${projectFolder}/runs` : `${dataset.output}/runs`}
            view={residualView}
            onView={chooseView}
            residualScale={residualScale}
            onResidualScale={setResidualScale}
            procedures={procedures}
            tilted={cameras.some((c) => c.params.sensor.type === "scheimpflug")}
            saveProcedure={saveProcedure}
          />
        </section>
      )}
    </div>
  );
  const trials = calibrationRun.result?.run.reports.length ?? 1;
  const observationsPane = (
    <section
      className="flex min-h-0 min-w-0 flex-col border-t border-line"
      aria-label="Observations"
      data-testid="observations"
    >
      <div className="flex shrink-0 flex-wrap items-center gap-2 px-2 py-1 text-xs">
        <span className="font-medium">Observations</span>
        {cameras.length > 1 && shownCamera && (
          <Select
            aria-label="Observed camera"
            value={shownCamera.id}
            options={cameras.map((c) => ({ value: c.id, label: c.id }))}
            onValueChange={setObservedCamera}
          />
        )}
        {residualsAt && (
          <SegmentedControl
            aria-label="Camera view shows"
            value={layer}
            options={[
              { value: "residuals", label: "Residuals" },
              { value: "observations", label: "Observations" },
            ]}
            onValueChange={(v) => (v === "residuals" || v === "observations") && setLayer(v)}
          />
        )}
        <span className="text-fg-muted">
          {showResiduals
            ? `The calibration's residuals${trials > 1 ? " (first trial)" : ""} at each capture: a dot is an observed point (green within the RMS, amber up to twice it, red beyond), a cross its reprojection, the line the residual ×${residualScale}.`
            : form.source === "detected"
              ? "The detector's observations are in the dataset's images."
              : (observeError ??
                "At a capture, a dot is what the calibration observes (an outlier larger, in red) and a cross the truth; a cross without a dot was dropped.")}
        </span>
      </div>
      {shownCamera && runtime && (
        <div className="min-h-0 flex-1">
          <CameraView
            session={current.session}
            runtime={runtime}
            poses={poses ?? undefined}
            camera={shownCamera}
            targets={scene?.targets ?? []}
            lasers={scene?.lasers ?? []}
            playhead={playhead}
            renderedAt={renderedAt}
            observedAt={showResiduals ? undefined : observedAt}
            residualsAt={showResiduals ? residualsAt : undefined}
          />
        </div>
      )}
    </section>
  );
  return { panel, observations: observationsPane, estimate };
}
