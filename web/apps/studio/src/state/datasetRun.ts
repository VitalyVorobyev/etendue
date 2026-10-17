/**
 * The studio's dataset run (Tauri shell only): at most one at a time, owned here and not by
 * the Dataset panel, so switching tabs neither loses its progress and its Cancel button nor
 * lets a second run start into the same folder. It is its own store, so a stream of Blender
 * log lines re-renders the panel only, and at most every `PUBLISH_MS`.
 */

import { toast } from "@vitavision/workbench";
import { useSyncExternalStore } from "react";

import {
  type Dataset,
  type DatasetRequest,
  type Progress,
  type Stage,
  cancelRun,
  generateDataset,
} from "../kernel/native";
import type { Studio } from "./studio";

/** A run in flight. */
export interface Run {
  id: string;
  step: { stage: Stage; done: number; total: number } | null;
  log: string[];
}

/** A snapshot of the runner. Replaced, never mutated. */
export interface RunnerState {
  run: Run | null;
  /** Why the last run failed. */
  failure: string | null;
}

/** Log lines kept per run. */
const LOG_KEEP = 200;
/** Progress reaches the UI at most this often (ms). */
const PUBLISH_MS = 100;

export class DatasetRunner {
  readonly #studio: Studio;
  #state: RunnerState = { run: null, failure: null };
  readonly #listeners = new Set<() => void>();
  /** The run as last reported, ahead of what is published. */
  #latest: Run | null = null;
  #timer: ReturnType<typeof setTimeout> | null = null;

  constructor(studio: Studio) {
    this.#studio = studio;
  }

  readonly get = (): RunnerState => this.#state;

  readonly subscribe = (listener: () => void): (() => void) => {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  };

  #publish(patch: Partial<RunnerState>): void {
    if (this.#timer !== null) clearTimeout(this.#timer);
    this.#timer = null;
    this.#state = { ...this.#state, ...patch };
    for (const l of this.#listeners) l();
  }

  #progress(p: Progress): void {
    const r = this.#latest;
    if (r === null) return;
    this.#latest =
      p.kind === "step"
        ? { ...r, step: { stage: p.stage, done: p.done, total: p.total } }
        : { ...r, log: [...r.log, p.line].slice(-LOG_KEEP) };
    this.#timer ??= setTimeout(() => this.#publish({ run: this.#latest }), PUBLISH_MS);
  }

  /** Start a run of `request` for the scene and scenario loaded now; ignored while one runs. */
  readonly start = (request: DatasetRequest): void => {
    if (this.#latest !== null) return;
    const generation = this.#studio.generation();
    const id = `run-${Date.now()}`;
    this.#latest = { id, step: null, log: [] };
    this.#publish({ run: this.#latest, failure: null });
    this.#studio.setDataset(null);
    let failure: string | null = null;
    void generateDataset(id, request, (p) => this.#progress(p))
      .then((outcome) => {
        if (outcome.kind === "ok") {
          const dataset: Dataset = {
            output: outcome.output,
            gt: outcome.gt,
            render: outcome.render,
            detection: outcome.detection,
            run: id,
          };
          // A scene or scenario opened since the run started is not the one it rendered.
          if (this.#studio.setDatasetFor(generation, dataset)) {
            toast({ title: "Dataset generated", description: dataset.output, tone: "success", duration: 4000 });
          } else {
            toast({ title: "Dataset generated for a scene no longer open", description: dataset.output, tone: "info" });
          }
        } else if (outcome.kind === "cancelled") {
          toast({ title: "Dataset generation cancelled", tone: "info", duration: 2500 });
        } else {
          failure = outcome.message;
        }
      })
      .catch((e: unknown) => {
        failure = e instanceof Error ? e.message : String(e);
      })
      .finally(() => {
        this.#latest = null;
        this.#publish({ run: null, failure });
      });
  };

  /** Ask the run in flight to stop. */
  readonly cancel = (): void => {
    if (this.#latest !== null) void cancelRun(this.#latest.id);
  };
}

/** The runner's current state, re-rendering on change. */
export function useDatasetRun(runner: DatasetRunner): RunnerState {
  return useSyncExternalStore(runner.subscribe, runner.get, runner.get);
}
