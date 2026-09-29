/**
 * The native pipeline of the Tauri shell (`src-tauri`, on the `etendue_cli` library): dataset
 * generation — ground truth, Blender render, detection — with progress, Blender's status,
 * and scenarios from tool poses. The per-frame kernel stays `@etendue/wasm`
 * (`kernel/etendue.ts`). These types mirror the Rust ones in `src-tauri/src/pipeline.rs` and
 * `etendue_cli::{progress, gt, render, detect}`.
 */

import type { ScenarioSpec } from "@etendue/wasm";
import { Channel, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

/** A pipeline step (`etendue_cli::progress::Stage`). */
export type Stage = "ground_truth" | "render" | "resample" | "detect";

/** A progress event (`etendue_cli::progress::Progress`). */
export type Progress =
  | { kind: "step"; stage: Stage; done: number; total: number }
  | { kind: "log"; line: string };

/** What to generate (`DatasetRequest`). */
export interface DatasetRequest {
  scene: string;
  scenario: ScenarioSpec;
  output: string;
  render: boolean;
  samples: number;
  supersample: number;
  sensor: string | null;
  cameras: string[];
  cpu: boolean;
  blender: string | null;
  allowBlenderVersion: boolean;
}

/** `etendue_cli::gt::GtSummary`. */
export interface GtSummary {
  captures: number;
  views: number;
  visible: number;
  points: number;
}

/** `etendue_cli::render::RenderSummary`. */
export interface RenderSummary {
  images: number;
  cameras: string[];
}

/** `etendue_cli::detect::CameraSummary`. */
export interface CameraSummary {
  camera: string;
  views: number;
  ok: number;
  no_board: number;
  partial: number;
  ambiguous: number;
  points: number;
  mislabelled: number;
  unmatched: number;
  rms_px: number;
  max_px: number;
}

/** A generated dataset (`Dataset`). */
export interface Dataset {
  output: string;
  gt: GtSummary;
  render: RenderSummary | null;
  detection: CameraSummary[];
}

/** The end of a run (`Outcome`). */
export type Outcome = ({ kind: "ok" } & Dataset) | { kind: "cancelled" } | { kind: "failed"; message: string };

/** Blender as the shell finds it (`BlenderStatus`). */
export interface BlenderStatus {
  found: boolean;
  exe: string;
  version: string | null;
  pin: string | null;
  message: string | null;
}

/** The checkout the shell was built from (examples, robot library), if it still exists. */
export function repoRoot(): Promise<string | null> {
  return invoke<string | null>("repo_root");
}

/** Whether `path` is an existing file. */
export function pathExists(path: string): Promise<boolean> {
  return invoke<boolean>("path_exists", { path });
}

/** Blender's status for a scene in `sceneDir`. */
export function blenderStatus(sceneDir: string | null, exe: string | null = null): Promise<BlenderStatus> {
  return invoke<BlenderStatus>("blender_status", { exe, sceneDir });
}

/** Run `request` as `runId`, calling `onProgress` for every event. */
export function generateDataset(
  runId: string,
  request: DatasetRequest,
  onProgress: (p: Progress) => void,
): Promise<Outcome> {
  const channel = new Channel<Progress>();
  channel.onmessage = onProgress;
  return invoke<Outcome>("generate_dataset", { runId, request, onProgress: channel });
}

/** Ask run `runId` to stop; `true` if it was still running. */
export function cancelRun(runId: string): Promise<boolean> {
  return invoke<boolean>("cancel_run", { runId });
}

/** A stop-and-shoot scenario for `robot` from a poses file. */
export function scenarioFromPoses(path: string, robot: string, speedScale = 0.5): Promise<ScenarioSpec> {
  return invoke<ScenarioSpec>("scenario_from_poses", { path, robot, speedScale });
}

const JSON_FILTER = [{ name: "JSON", extensions: ["json"] }];

/** Pick JSON files (a scene, optionally its scenario). */
export async function pickJsonFiles(title: string, multiple: boolean): Promise<string[]> {
  const picked = await open({ title, multiple, filters: JSON_FILTER });
  if (picked === null) return [];
  return Array.isArray(picked) ? picked : [picked];
}

/** Pick a directory. */
export async function pickDirectory(title: string): Promise<string | null> {
  const picked = await open({ title, directory: true });
  return typeof picked === "string" ? picked : null;
}
