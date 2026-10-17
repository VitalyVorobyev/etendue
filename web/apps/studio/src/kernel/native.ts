/**
 * The native pipeline of the Tauri shell (`src-tauri`, on the `etendue_cli` library): dataset
 * generation — ground truth, Blender render, detection — with progress, Blender's status,
 * and scenarios from tool poses. The per-frame kernel stays `@etendue/wasm`
 * (`kernel/etendue.ts`). These types mirror the Rust ones in `src-tauri/src/pipeline.rs` and
 * `etendue_cli::{progress, gt, render, detect}`.
 */

import type { ScenarioSpec } from "@etendue/wasm";
import { Channel, invoke } from "@tauri-apps/api/core";

import { normalize } from "../io/tauri";

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

/** What a run produced (`Dataset`). */
export interface DatasetSummary {
  output: string;
  gt: GtSummary;
  render: RenderSummary | null;
  detection: CameraSummary[];
}

/** A generated dataset, with the run that made it (its images' URLs change per run). */
export interface Dataset extends DatasetSummary {
  run: string;
}

/** The end of a run (`Outcome`). */
export type Outcome = ({ kind: "ok" } & DatasetSummary) | { kind: "cancelled" } | { kind: "failed"; message: string };

/** Blender as the shell finds it (`BlenderStatus`). */
export interface BlenderStatus {
  found: boolean;
  exe: string;
  version: string | null;
  pin: string | null;
  message: string | null;
}

/** Where the shell finds the examples and the robot library, and where datasets go by default. */
export interface DataRoot {
  /** The checkout (a development build) or the app's bundled resources. */
  root: string;
  /** The default parent of dataset folders: the checkout's `target/studio`, else `Documents/etendue`. */
  datasets: string;
}

/** The data root, if the shell has one. */
export async function dataRoot(): Promise<DataRoot | null> {
  const found = await invoke<DataRoot | null>("data_root");
  return found === null ? null : { root: normalize(found.root), datasets: normalize(found.datasets) };
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

/**
 * Pick JSON files (a scene, optionally its scenario). The dialog runs in the shell, which then
 * lets the studio read the picked files' folders (and a scene's robot folders), and nothing
 * else outside the checkout.
 */
export async function pickJsonFiles(title: string, multiple: boolean): Promise<string[]> {
  const picked = await invoke<string[]>("pick", { title, multiple, directory: false });
  return picked.map(normalize);
}

/** Pick a directory (it becomes readable and writable for the studio). */
export async function pickDirectory(title: string): Promise<string | null> {
  const [picked] = await invoke<string[]>("pick", { title, multiple: false, directory: true });
  return picked === undefined ? null : normalize(picked);
}
