/**
 * The studio's document state — what is loaded, whether a load is running, why the last one
 * failed, whether playback runs — as an external store, so loads (async, started from events
 * or on mount) never set React state from an effect.
 */

import type { ScenarioSpec } from "@etendue/wasm";
import { type Playhead, toast } from "@vitavision/workbench";
import { useSyncExternalStore } from "react";
import examples from "virtual:etendue-examples";

import { type LoadedScene, documentKind, loadScene } from "../io/load";
import { type FileSource, filePath, filesSource, urlSource } from "../io/source";
import { fsSource, isTauri, splitAbsolute } from "../io/tauri";
import { type Baked, type KernelFailure, bakeScene, describeFailure } from "../kernel/etendue";
import { type Dataset, repoRoot } from "../kernel/native";

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
}

const served = urlSource(`${import.meta.env.BASE_URL}repo`, "repository");
let repoSource: Promise<FileSource> | null = null;

/**
 * The repository's examples and robot library: served over HTTP in a browser; in the Tauri
 * shell, read from the checkout on disk, so scenes opened from it have filesystem paths (the
 * native pipeline needs them).
 */
function repository(): Promise<FileSource> {
  repoSource ??= isTauri()
    ? repoRoot()
        .then((root) => (root === null ? served : fsSource(root, "repository")))
        .catch(() => served)
    : Promise.resolve(served);
  return repoSource;
}

/** Read files picked in the Tauri shell by absolute path: one scene, at most one scenario. */
async function loadPaths(paths: string[]): Promise<LoadedScene> {
  const docs = await Promise.all(
    paths.map(async (path) => {
      const { root, relative } = splitAbsolute(path);
      const source = fsSource(root);
      try {
        return { root, relative, kind: documentKind(JSON.parse(await source.readText(relative))) };
      } catch {
        return { root, relative, kind: null };
      }
    }),
  );
  const scenes = docs.filter((d) => d.kind === "scene");
  const scenarios = docs.filter((d) => d.kind === "scenario");
  if (scenes.length !== 1) {
    throw new Error(scenes.length === 0 ? "No scene among the picked files." : `${scenes.length} scenes picked; pick one.`);
  }
  if (scenarios.length > 1) throw new Error(`${scenarios.length} scenarios picked; pick at most one.`);
  const scene = scenes[0]!;
  const scenario = scenarios[0];
  if (scenario && scenario.root !== scene.root) throw new Error("The scene and the scenario are on different drives.");
  return loadScene(fsSource(scene.root), scene.relative, scenario?.relative ?? null, await repository());
}

/** Read dropped files: exactly one scene, at most one scenario, plus any robot assets. */
async function loadDropped(files: File[]): Promise<LoadedScene> {
  const source = filesSource(files);
  const docs = await Promise.all(
    files
      .filter((f) => f.name.endsWith(".json"))
      .map(async (f) => {
        try {
          return { path: filePath(f), kind: documentKind(JSON.parse(await f.text())) };
        } catch {
          return { path: filePath(f), kind: null };
        }
      }),
  );
  const scenes = docs.filter((d) => d.kind === "scene");
  const scenarios = docs.filter((d) => d.kind === "scenario");
  if (scenes.length !== 1) {
    throw new Error(
      scenes.length === 0 ? "No scene among the dropped files." : `${scenes.length} scenes dropped; drop one at a time.`,
    );
  }
  if (scenarios.length > 1) throw new Error(`${scenarios.length} scenarios dropped; drop at most one.`);
  return loadScene(source, scenes[0]!.path, scenarios[0]?.path ?? null, await repository());
}

/** The studio store. One per app. */
export class Studio {
  readonly playhead: Playhead;
  #state: StudioState = { current: null, failure: null, loading: false, playing: false, example: "", dataset: null };
  readonly #listeners = new Set<() => void>();
  #generation = 0;

  constructor(playhead: Playhead) {
    this.playhead = playhead;
  }

  readonly get = (): StudioState => this.#state;

  readonly subscribe = (listener: () => void): (() => void) => {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  };

  #set(patch: Partial<StudioState>): void {
    this.#state = { ...this.#state, ...patch };
    for (const l of this.#listeners) l();
  }

  /** Start or stop playback. */
  readonly setPlaying = (playing: boolean): void => this.#set({ playing });

  /** Open the example `id` (see `virtual:etendue-examples`). */
  readonly openExample = (id: string): void => {
    const ex = examples.find((e) => e.id === id);
    if (!ex) return;
    this.#set({ example: id });
    void this.#open(ex.id, async () => {
      const repo = await repository();
      return loadScene(repo, ex.scene, ex.scenario, repo);
    });
  };

  /** Open files picked by absolute path (the Tauri shell's file dialog). */
  readonly openPaths = (paths: string[]): void => {
    if (paths.length === 0) return;
    this.#set({ example: "" });
    void this.#open(`${paths.length} file(s)`, () => loadPaths(paths));
  };

  /** Replace the current scene's scenario (imported tool poses) and bake again. */
  readonly setScenario = (scenario: ScenarioSpec, what: string): void => {
    const current = this.#state.current;
    if (!current) return;
    void this.#open(what, () => Promise.resolve({ ...current.loaded, scenario }));
  };

  /** Record the dataset generated for the current scene. */
  readonly setDataset = (dataset: Dataset | null): void => this.#set({ dataset });

  /** Open dropped or picked files. */
  readonly openFiles = (files: File[]): void => {
    this.#set({ example: "" });
    void this.#open(`${files.length} file(s)`, () => loadDropped(files));
  };

  /** Open `id` unless something is loaded or loading already (safe to call on every mount). */
  start(id: string): void {
    if (this.#state.current === null && !this.#state.loading && this.#generation === 0) this.openExample(id);
  }

  /** Free the kernel session. */
  dispose(): void {
    this.#state.current?.session.free();
  }

  async #open(what: string, load: () => Promise<LoadedScene>): Promise<void> {
    const generation = ++this.#generation;
    this.#set({ loading: true, playing: false });
    try {
      const next = await bakeScene(await load());
      if (generation !== this.#generation) {
        next.session.free();
        return;
      }
      this.#state.current?.session.free();
      this.playhead.setTimeline(next.baked.samples.length, next.baked.dt);
      this.playhead.set(0);
      this.#set({ current: next, failure: null, loading: false, dataset: null });
      toast({ title: `Loaded ${what}`, tone: "success", duration: 2500 });
    } catch (e) {
      if (generation !== this.#generation) return;
      const failure = describeFailure(e);
      this.#set({ failure, loading: false });
      toast({ title: failure.title, description: failure.message, tone: "error" });
    }
  }
}

/** The studio's current state, re-rendering on change. */
export function useStudio(studio: Studio): StudioState {
  return useSyncExternalStore(studio.subscribe, studio.get, studio.get);
}

/** The example named by `?example=`, else the first. */
export function initialExample(): string {
  const wanted = new URLSearchParams(window.location.search).get("example");
  return examples.find((e) => e.id === wanted)?.id ?? examples[0]?.id ?? "";
}
