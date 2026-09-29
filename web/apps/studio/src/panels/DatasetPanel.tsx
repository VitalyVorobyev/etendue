import { Button, Callout, Checkbox, Field, Input, ProgressBar, StatusDot, Switch, Table } from "@vitavision/ui";
import { NumberInput } from "@vitavision/ui-next";
import { toast } from "@vitavision/workbench";
import { FolderOpen, Play, Square } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { parentDir } from "../io/tauri";
import type { Baked } from "../kernel/etendue";
import {
  type BlenderStatus,
  type CameraSummary,
  type Dataset,
  type Progress,
  type Stage,
  blenderStatus,
  cancelRun,
  generateDataset,
  pathExists,
  pickDirectory,
  pickJsonFiles,
  repoRoot,
} from "../kernel/native";

const STAGES: Record<Stage, string> = {
  ground_truth: "Ground truth",
  render: "Rendering",
  resample: "Resampling",
  detect: "Detecting",
};

/** Lines of the run log kept on screen. */
const LOG_LINES = 8;

/** Where a scene's dataset goes by default: `target/studio/<example>` inside the checkout for
 * its examples (never into `examples/`), else `dataset/` beside the scene. */
function defaultOutput(origin: string, root: string | null): string {
  const dir = parentDir(origin);
  const examples = root === null ? null : `${root.replace(/\/+$/, "")}/examples/`;
  if (examples !== null && dir.startsWith(examples)) {
    return `${root!.replace(/\/+$/, "")}/target/studio/${dir.slice(examples.length)}`;
  }
  return `${dir}/dataset`;
}

interface Defaults {
  output: string;
  sensor: string | null;
  blender: BlenderStatus | null;
}

/** The defaults for a scene, looked up once per scene. */
function useDefaults(origin: string | null): Defaults | null {
  const [found, setFound] = useState<{ origin: string; value: Defaults } | null>(null);
  useEffect(() => {
    if (origin === null) return;
    let live = true;
    const dir = parentDir(origin);
    void Promise.all([
      repoRoot().catch(() => null),
      pathExists(`${dir}/sensor_linear.json`).catch(() => false),
      blenderStatus(dir).catch(() => null),
    ]).then(([root, sensor, blender]) => {
      if (live) {
        setFound({
          origin,
          value: { output: defaultOutput(origin, root), sensor: sensor ? `${dir}/sensor_linear.json` : null, blender },
        });
      }
    });
    return () => {
      live = false;
    };
  }, [origin]);
  return found !== null && found.origin === origin ? found.value : null;
}

/** Everything the form edits, all optional until the user touches it (defaults fill in). */
interface Form {
  output?: string;
  sensor?: string | null;
  render: boolean;
  samples: number;
  supersample: number;
  cameras: string[] | null;
  cpu: boolean;
}

interface Run {
  id: string;
  step: { stage: Stage; done: number; total: number } | null;
  log: string[];
}

const COLUMNS = [
  { key: "camera", header: "Camera", cell: (r: CameraSummary) => r.camera },
  { key: "ok", header: "Views ok", numeric: true, cell: (r: CameraSummary) => `${r.ok}/${r.views}` },
  { key: "points", header: "Corners", numeric: true, cell: (r: CameraSummary) => r.points },
  {
    key: "wrong",
    header: "Mislabelled",
    numeric: true,
    cell: (r: CameraSummary) => r.mislabelled + r.unmatched,
  },
  { key: "rms", header: "RMS vs GT", numeric: true, cell: (r: CameraSummary) => `${r.rms_px.toFixed(4)} px` },
  { key: "max", header: "Max", numeric: true, cell: (r: CameraSummary) => `${r.max_px.toFixed(3)} px` },
];

/**
 * Generate a synthetic dataset of the current scene and scenario with the native pipeline
 * (Tauri shell only): analytic ground truth, a Blender render of every capture, and corner
 * detection checked against the ground truth — `etendue gt`, `render` and `detect` in one run.
 */
export function DatasetPanel({
  current,
  dataset,
  onDataset,
}: {
  current: Baked;
  dataset: Dataset | null;
  onDataset: (d: Dataset | null) => void;
}) {
  const { origin, scenario, scene } = current.loaded;
  const defaults = useDefaults(origin);
  const allCameras = (scene.cameras ?? []).map((c) => c.id);
  const [form, setForm] = useState<Form>({
    render: true,
    samples: 16,
    supersample: 4,
    cameras: null,
    cpu: false,
  });
  const [run, setRun] = useState<Run | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const runRef = useRef<Run | null>(null);

  if (origin === null) {
    return (
      <Callout tone="info" title="Open the scene from disk">
        Dataset generation needs the scene's files: open it with “Open scene…” (dropped files have no path).
      </Callout>
    );
  }
  if (scenario === null) {
    return (
      <Callout tone="info" title="No scenario">
        Open a scenario with the scene, or import tool poses, to generate a dataset of its captures.
      </Callout>
    );
  }

  const output = form.output ?? defaults?.output ?? "";
  const sensor = form.sensor === undefined ? (defaults?.sensor ?? null) : form.sensor;
  const cameras = form.cameras ?? allCameras;
  const blender = defaults?.blender ?? null;
  const running = run !== null;

  const onProgress = (p: Progress) => {
    const r = runRef.current;
    if (!r) return;
    const next: Run =
      p.kind === "step"
        ? { ...r, step: { stage: p.stage, done: p.done, total: p.total } }
        : { ...r, log: [...r.log, p.line].slice(-200) };
    runRef.current = next;
    setRun(next);
  };

  const start = () => {
    const id = `run-${Date.now()}`;
    const r: Run = { id, step: null, log: [] };
    runRef.current = r;
    setRun(r);
    setFailure(null);
    onDataset(null);
    void generateDataset(
      id,
      {
        scene: origin,
        scenario,
        output,
        render: form.render,
        samples: form.samples,
        supersample: form.supersample,
        sensor,
        cameras: cameras.length === allCameras.length ? [] : cameras,
        cpu: form.cpu,
        blender: null,
        allowBlenderVersion: false,
      },
      onProgress,
    )
      .then((outcome) => {
        if (outcome.kind === "ok") {
          const d: Dataset = {
            output: outcome.output,
            gt: outcome.gt,
            render: outcome.render,
            detection: outcome.detection,
          };
          onDataset(d);
          toast({ title: "Dataset generated", description: d.output, tone: "success", duration: 4000 });
        } else if (outcome.kind === "cancelled") {
          toast({ title: "Dataset generation cancelled", tone: "info", duration: 2500 });
        } else {
          setFailure(outcome.message);
        }
      })
      .catch((e: unknown) => setFailure(e instanceof Error ? e.message : String(e)))
      .finally(() => {
        runRef.current = null;
        setRun(null);
      });
  };

  const step = run?.step ?? null;
  const set = (patch: Partial<Form>) => setForm((f) => ({ ...f, ...patch }));

  return (
    <div className="flex h-full min-h-0 gap-4 overflow-auto" data-testid="dataset-panel">
      <div className="flex w-96 shrink-0 flex-col gap-3">
        <div className="flex items-center gap-2 text-xs" data-testid="blender-status">
          {blender === null ? (
            <StatusDot tone="neutral">Looking for Blender…</StatusDot>
          ) : blender.found ? (
            <StatusDot tone={blender.message === null ? "normal" : "warning"}>
              Blender {blender.version}
              {blender.pin !== null && blender.pin !== blender.version ? ` (pinned ${blender.pin})` : ""}
            </StatusDot>
          ) : (
            <StatusDot tone="defect">Blender not found</StatusDot>
          )}
          {blender?.message && <span className="truncate text-fg-muted" title={blender.message}>{blender.message}</span>}
        </div>
        <Field label="Output folder">
          <div className="flex gap-2">
            <Input value={output} onChange={(e) => set({ output: e.target.value })} className="font-mono text-xs" />
            <Button
              icon={<FolderOpen className="size-4" />}
              onClick={() => void pickDirectory("Dataset folder").then((d) => d !== null && set({ output: d }))}
              disabled={running}
            >
              Choose…
            </Button>
          </div>
        </Field>
        <Switch
          checked={form.render}
          onCheckedChange={(render) => set({ render })}
          label="Render with Blender"
          description="Off: the analytic ground truth only (dataset.json, gt.json)."
          disabled={running}
        />
        {form.render && (
          <>
            <div className="grid grid-cols-2 gap-3">
              <Field label="Samples" annotation="per canonical pixel">
                <NumberInput
                  value={form.samples}
                  min={1}
                  step={1}
                  onChange={(e) => set({ samples: Math.max(1, Math.round(Number(e.target.value)) || 1) })}
                  disabled={running}
                />
              </Field>
              <Field label="Supersampling" annotation="s">
                <NumberInput
                  value={form.supersample}
                  min={1}
                  step={1}
                  onChange={(e) => set({ supersample: Math.max(1, Number(e.target.value) || 1) })}
                  disabled={running}
                />
              </Field>
            </div>
            <Field label="Sensor model" description="None: sRGB images. A sensor gives linear raw images (G4.2).">
              <div className="flex gap-2">
                <Input value={sensor ?? ""} placeholder="none (sRGB)" readOnly className="font-mono text-xs" />
                <Button
                  onClick={() => void pickJsonFiles("Sensor model", false).then(([p]) => p !== undefined && set({ sensor: p }))}
                  disabled={running}
                >
                  Choose…
                </Button>
                <Button variant="ghost" onClick={() => set({ sensor: null })} disabled={running || sensor === null}>
                  Clear
                </Button>
              </div>
            </Field>
            <Field label="Cameras" as="group">
              <div className="flex flex-wrap gap-3">
                {allCameras.map((id) => (
                  <Checkbox
                    key={id}
                    label={id}
                    checked={cameras.includes(id)}
                    onCheckedChange={(on) =>
                      set({ cameras: on ? [...cameras, id] : cameras.filter((c) => c !== id) })
                    }
                    disabled={running}
                  />
                ))}
              </div>
            </Field>
            <Switch
              checked={form.cpu}
              onCheckedChange={(cpu) => set({ cpu })}
              label="Render on the CPU"
              description="Bit-exact between runs, and slower (P4-5)."
              disabled={running}
            />
          </>
        )}
        <div className="flex gap-2">
          <Button
            variant="primary"
            icon={<Play className="size-4" />}
            loading={running}
            disabled={running || output === "" || (form.render && (cameras.length === 0 || blender?.found === false))}
            onClick={start}
          >
            Generate
          </Button>
          {running && (
            <Button variant="danger" icon={<Square className="size-4" />} onClick={() => void cancelRun(run.id)}>
              Cancel
            </Button>
          )}
        </div>
      </div>
      <div className="flex min-w-0 flex-1 flex-col gap-3">
        {running && (
          <ProgressBar
            fraction={step && step.total > 0 ? step.done / step.total : 0}
            label={step ? `${STAGES[step.stage]}: ${step.done} of ${step.total}` : "Starting…"}
            aria-label="Dataset generation"
          />
        )}
        {run && run.log.length > 0 && (
          <pre className="max-h-40 overflow-auto rounded border border-line bg-canvas p-2 font-mono text-[11px] text-fg-muted" data-testid="dataset-log">
            {run.log.slice(-LOG_LINES).join("\n")}
          </pre>
        )}
        {failure && (
          <Callout tone="error" title="Dataset generation failed">
            <span className="whitespace-pre-wrap font-mono text-xs">{failure}</span>
          </Callout>
        )}
        {dataset && !running && (
          <div className="flex flex-col gap-2" data-testid="dataset-result">
            <div className="text-xs text-fg-muted">
              <span className="font-mono text-fg">{dataset.output}</span> · {dataset.gt.captures} captures,{" "}
              {dataset.gt.views} views, {dataset.gt.visible} visible points
              {dataset.render && ` · ${dataset.render.images} images`}
            </div>
            {dataset.detection.length > 0 && (
              <Table columns={COLUMNS} rows={dataset.detection} rowKey={(r) => r.camera} caption="Detection against the ground truth" />
            )}
          </div>
        )}
      </div>
    </div>
  );
}
