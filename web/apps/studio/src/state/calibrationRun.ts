/**
 * The studio's calibration run (Tauri shell only): at most one at a time, its result kept for
 * the dataset it calibrated, so switching workspaces does not lose it. Each run is also kept in
 * a folder (ADR 0009 `runs/<name>`), and a kept run can be shown again.
 */

import { useSyncExternalStore } from "react";

import {
  type CalibrateOptions,
  type CalibrationRun,
  type Progress,
  type StoredRun,
  calibrateDataset,
  cancelRun,
} from "../kernel/native";

export interface CalibrationState {
  /** A run in flight: its id and trials done. */
  running: { id: string; done: number; total: number } | null;
  /** The result shown, for the dataset in `dir`: the last run, or a kept one; `name` is its folder's. */
  result: { dir: string; options: CalibrateOptions; run: CalibrationRun; name: string } | null;
  failure: string | null;
  /** Runs kept so far in this session, so a list of them knows to look again. */
  kept: number;
}

export class CalibrationRunner {
  #state: CalibrationState = { running: null, result: null, failure: null, kept: 0 };
  readonly #listeners = new Set<() => void>();

  readonly get = (): CalibrationState => this.#state;

  readonly subscribe = (listener: () => void): (() => void) => {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  };

  #set(patch: Partial<CalibrationState>): void {
    this.#state = { ...this.#state, ...patch };
    for (const l of this.#listeners) l();
  }

  #progress(id: string, p: Progress): void {
    const r = this.#state.running;
    if (p.kind === "step" && r?.id === id) this.#set({ running: { ...r, done: p.done, total: p.total } });
  }

  /** Calibrate the dataset in `dir`, keeping the run in `folder/name`; ignored while a run is in flight. */
  readonly start = (dir: string, options: CalibrateOptions, folder: string, name: string): void => {
    if (this.#state.running !== null) return;
    const id = `calibrate-${Date.now()}`;
    this.#set({ running: { id, done: 0, total: options.trials ?? 1 }, failure: null });
    let next: Partial<CalibrationState> = {};
    void calibrateDataset(id, dir, options, `${folder}/${name}`, (p) => this.#progress(id, p))
      .then((outcome) => {
        if (outcome.kind === "ok") {
          const run: CalibrationRun = { reports: outcome.reports };
          if (outcome.row_stats) run.row_stats = outcome.row_stats;
          next = { result: { dir, options, run, name }, kept: this.#state.kept + 1 };
        } else if (outcome.kind === "failed") {
          next = { failure: outcome.message };
        }
      })
      .catch((e: unknown) => {
        next = { failure: e instanceof Error ? e.message : String(e) };
      })
      .finally(() => this.#set({ running: null, ...next }));
  };

  /** Show kept run `stored` of the dataset in `dir`. */
  readonly show = (dir: string, stored: StoredRun): void => {
    if (this.#state.running !== null) return;
    this.#set({ result: { dir, options: stored.config.options, run: stored.run, name: stored.name }, failure: null });
  };

  readonly cancel = (): void => {
    const r = this.#state.running;
    if (r !== null) void cancelRun(r.id);
  };
}

export function useCalibrationRun(runner: CalibrationRunner): CalibrationState {
  return useSyncExternalStore(runner.subscribe, runner.get, runner.get);
}
