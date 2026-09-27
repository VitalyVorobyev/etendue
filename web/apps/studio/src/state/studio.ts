/**
 * The studio's document state — what is loaded, whether a load is running, why the last one
 * failed, whether playback runs — as an external store, so loads (async, started from events
 * or on mount) never set React state from an effect.
 */

import { type Playhead, toast } from "@vitavision/workbench";
import { useSyncExternalStore } from "react";
import examples from "virtual:etendue-examples";

import { type LoadedScene, documentKind, loadScene } from "../io/load";
import { filePath, filesSource, urlSource } from "../io/source";
import { type Baked, type KernelFailure, bakeScene, describeFailure } from "../kernel/etendue";

/** A snapshot of the studio. Replaced, never mutated. */
export interface StudioState {
  current: Baked | null;
  failure: KernelFailure | null;
  loading: boolean;
  playing: boolean;
  /** The example shown in the picker (`""` after opening files). */
  example: string;
}

const repo = urlSource(`${import.meta.env.BASE_URL}repo`, "repository");

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
  return loadScene(source, scenes[0]!.path, scenarios[0]?.path ?? null, repo);
}

/** The studio store. One per app. */
export class Studio {
  readonly playhead: Playhead;
  #state: StudioState = { current: null, failure: null, loading: false, playing: false, example: "" };
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
    void this.#open(ex.id, () => loadScene(repo, ex.scene, ex.scenario, repo));
  };

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
      this.#set({ current: next, failure: null, loading: false });
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
