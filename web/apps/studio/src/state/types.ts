/** The studio store's document and state types (`Studio`, `state/studio.ts`). */

import type { LibraryItem, ScenarioSpec, SceneSpec } from "@etendue/wasm";

import type { LoadedRobot } from "../io/load";
import type { Bank, Project } from "../io/project";
import type { Baked, KernelFailure } from "../kernel/etendue";
import type { Dataset } from "../kernel/native";
import type { PoseOverride } from "../kernel/posed";

/** What an edit changes: the scene, the scenario, and the robot models the scene names. */
export interface Doc {
  scene: SceneSpec;
  scenario: ScenarioSpec | null;
  robots: LoadedRobot[];
}

/**
 * An edit in progress (a slider or a gizmo being dragged): shown as it goes, one undo step when
 * it is committed.
 */
export interface Preview {
  label: string;
  /** The document as shown. */
  doc: Doc;
  /** Moves shown on top of the current bake, without baking. */
  overrides: readonly PoseOverride[];
}

/** Where Save writes, once known (a picked or saved scene); `null` until then. */
export interface SavePaths {
  scene: string;
  scenario: string | null;
}

/** A snapshot of the studio. Replaced, never mutated. */
export interface StudioState {
  current: Baked | null;
  failure: KernelFailure | null;
  loading: boolean;
  playing: boolean;
  /** The example shown in the picker (`""` after opening files). */
  example: string;
  /** The dataset generated for the current scene and scenario (Tauri shell only). */
  dataset: Dataset | null;
  /** Labels of the edits Undo and Redo would revert or apply, latest last. */
  undo: string[];
  redo: string[];
  /** The scene or scenario differs from what was opened or last saved. */
  dirty: boolean;
  saveTo: SavePaths | null;
  /** Counts opened documents (not edits): what belongs to one document resets when it changes. */
  opened: number;
  /** The edit in progress, if any. */
  preview: Preview | null;
  /** The open project: its cells, its library, its defaults (ADR 0009). */
  project: Project | null;
  /** A project folder opened in an earlier session that the browser must be allowed again. */
  reopenable: string | null;
  /** The user's bank (ADR 0010), once it has opened. */
  bank: Bank | null;
  /** The built-in library (the reference project's), once it has loaded. */
  builtin: Map<string, LibraryItem> | null;
  /** A question waiting for the user's answer (`Studio.answer`), e.g. before unsaved work is dropped. */
  question: Question | null;
}

/** A yes-or-no question the studio asks: the dialog's title, what happens on yes, and its verb. */
export interface Question {
  title: string;
  description: string;
  confirmLabel: string;
}

/** Where the Library workspace writes: the user's bank or the open project. */
export type Store = "bank" | "project";
