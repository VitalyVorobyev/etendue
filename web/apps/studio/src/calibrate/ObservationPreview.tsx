/**
 * The Calibrate workspace in a browser: what a calibration of the open scene would observe,
 * exactly or with noise, without the desktop app's dataset generation and calibration. The
 * observations are the kernel's, the same as a desktop run of that seed calibrates from.
 */

import type { Observations } from "@etendue/wasm";
import { Callout } from "@vitavision/ui";

import { type CalibrateForm, NoiseFields, SourceControl } from "../panels/CalibratePanel";

/** Totals over every capture and camera. */
function totals(o: Observations) {
  let views = 0;
  let points = 0;
  let outliers = 0;
  let stripe = 0;
  for (const c of o.captures) {
    for (const v of c.views) {
      if (v.points.length > 0) views++;
      points += v.points.length;
      outliers += v.points.filter((p) => p.outlier).length;
      stripe += v.laser.length;
    }
  }
  return { captures: o.captures.length, views, points, outliers, stripe };
}

export function ObservationPreview({
  form,
  onForm,
  lasers,
  observations,
}: {
  form: CalibrateForm;
  onForm: (form: CalibrateForm) => void;
  lasers: boolean;
  observations: Observations | null;
}) {
  const t = observations ? totals(observations) : null;
  return (
    <div className="flex min-w-0 flex-col gap-3" data-testid="observation-preview">
      <Callout tone="info" title="Calibration runs in the desktop app">
        Generating a dataset and calibrating it need the desktop app. Here you can see what a calibration of this scene
        observes, exactly or with noise, at each capture.
      </Callout>
      <SourceControl
        value={form.source === "detected" ? "exact" : form.source}
        onChange={(source) => onForm({ ...form, source })}
        detected={false}
      />
      {form.source === "noisy" && (
        <NoiseFields
          noise={form.noise}
          onChange={(noise) => onForm({ ...form, noise })}
          lasers={lasers}
          trials={false}
        />
      )}
      {t && (
        <p className="font-mono text-xs text-fg-muted" data-testid="observation-totals">
          {t.captures} captures · {t.views} views · {t.points} points
          {t.outliers > 0 ? ` (${t.outliers} outliers)` : ""}
          {t.stripe > 0 ? ` · ${t.stripe} stripe pixels` : ""}
        </p>
      )}
    </div>
  );
}
