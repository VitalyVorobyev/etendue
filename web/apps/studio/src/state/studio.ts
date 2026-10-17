/**
 * The studio's document state — what is loaded, whether a load is running, why the last one
 * failed, whether playback runs — as an external store, so loads (async, started from events
 * or on mount) never set React state from an effect.
 */

import type { ScenarioSpec, SceneSpec } from "@etendue/wasm";
import { type Playhead, toast } from "@vitavision/workbench";
import { useSyncExternalStore } from "react";
import examples from "virtual:etendue-examples";

import { download, jsonText } from "../io/export";
import { type LoadedRobot, type LoadedScene, documentKind, loadRobot, loadScene } from "../io/load";
import { type FileSource, filePath, filesSource, urlSource } from "../io/source";
import { fsSource, isTauri, normalize, parentDir, splitAbsolute } from "../io/tauri";
import { type Baked, type KernelFailure, bakeScene, describeFailure, rebake } from "../kernel/etendue";
import { type Dataset, dataRoot, saveDialog, saveScene } from "../kernel/native";

/** What an edit changes: the scene, the scenario, and the robot models the scene names. */
export interface Doc {
  scene: SceneSpec;
  scenario: ScenarioSpec | null;
  robots: LoadedRobot[];
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
}

/** One entry of the undo or redo stack: the document before (or after) an edit. */
interface Step {
  label: string;
  doc: Doc;
  /** When the entry was made (ms), to fold a burst of the same edit into one. */
  at: number;
}

/** Undo entries kept. */
const HISTORY = 100;
/** Edits with the same label within this window (ms) are undone together (typing into a field). */
const COALESCE_MS = 1000;

/** The scenario file that goes with a scene file: `scenario.json` next to `scene.json`, else `<stem>.scenario.json`. */
export function scenarioPathFor(scenePath: string): string {
  const dir = parentDir(scenePath);
  const name = normalize(scenePath).slice(dir.length + 1);
  return name === "scene.json" ? `${dir}/scenario.json` : `${dir}/${name.replace(/\.json$/, "")}.scenario.json`;
}

const served = urlSource(`${import.meta.env.BASE_URL}repo`, "repository");
let repoSource: Promise<FileSource> | null = null;

/**
 * The repository's examples and robot library: served over HTTP in a browser; in the Tauri
 * shell, read from disk (the checkout, or the app's bundled copy), so scenes opened from it
 * have filesystem paths (the native pipeline needs them).
 */
function repository(): Promise<FileSource> {
  repoSource ??= isTauri()
    ? dataRoot()
        .then((data) => (data === null ? served : fsSource(data.root, "repository")))
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
    throw new Error(
      scenes.length === 0 ? "No scene among the picked files." : `${scenes.length} scenes picked; pick one.`,
    );
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
      scenes.length === 0
        ? "No scene among the dropped files."
        : `${scenes.length} scenes dropped; drop one at a time.`,
    );
  }
  if (scenarios.length > 1) throw new Error(`${scenarios.length} scenarios dropped; drop at most one.`);
  return loadScene(source, scenes[0]!.path, scenarios[0]?.path ?? null, await repository());
}

/** The studio store. One per app. */
export class Studio {
  readonly playhead: Playhead;
  #state: StudioState = {
    current: null,
    failure: null,
    loading: false,
    playing: false,
    example: "",
    dataset: null,
    undo: [],
    redo: [],
    dirty: false,
    saveTo: null,
    opened: 0,
  };
  readonly #listeners = new Set<() => void>();
  #generation = 0;
  #undo: Step[] = [];
  #redo: Step[] = [];
  /** The document as opened or last saved. */
  #saved: Doc | null = null;

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
    void this.#open(`${paths.length} file(s)`, () => loadPaths(paths)).then(() => {
      // Save writes back to the files that were picked.
      const loaded = this.#state.current?.loaded;
      if (loaded?.origin && paths.map(normalize).includes(loaded.origin)) {
        this.#set({ saveTo: { scene: loaded.origin, scenario: loaded.scenarioOrigin } });
      }
    });
  };

  /** Replace the current scene's scenario (imported or generated) as one undoable edit. */
  readonly setScenario = (scenario: ScenarioSpec, what: string): boolean =>
    this.edit(what, (d) => ({ ...d, scenario }));

  /**
   * Edit the scene or scenario: `change` returns the new pair (new objects for what changed). The
   * result is validated and baked; if that fails nothing changes and the reason is shown.
   * Returns whether the edit was applied. Edits with the same `label` in quick succession undo
   * together.
   */
  readonly edit = (label: string, change: (doc: Doc) => Doc): boolean => {
    const current = this.#state.current;
    if (!current) return false;
    const before = docOf(current);
    const after = change(before);
    if (after.scene === before.scene && after.scenario === before.scenario && after.robots === before.robots) {
      return true;
    }
    if (!this.#apply(current, after)) return false;
    const now = Date.now();
    const top = this.#undo.at(-1);
    if (!(top && top.label === label && now - top.at < COALESCE_MS)) {
      this.#undo = [...this.#undo, { label, doc: before, at: now }].slice(-HISTORY);
    } else {
      top.at = now;
    }
    this.#redo = [];
    this.#publishHistory();
    return true;
  };

  /** Revert the last edit. */
  readonly undo = (): void => this.#travel(this.#undo, this.#redo);

  /** Apply the last undone edit again. */
  readonly redo = (): void => this.#travel(this.#redo, this.#undo);

  #travel(from: Step[], to: Step[]): void {
    const current = this.#state.current;
    const step = from.at(-1);
    if (!current || !step) return;
    const here = docOf(current);
    if (!this.#apply(current, step.doc)) return;
    from.pop();
    to.push({ label: step.label, doc: here, at: 0 });
    this.#publishHistory();
  }

  /**
   * Add the library robot whose manifest is `manifest` (repository-relative) to the scene, as
   * `id`, attached to `parent`, by `change` (which places the new `RobotSpec`). One undoable edit.
   */
  readonly addRobot = async (
    manifest: string,
    id: string,
    change: (doc: Doc, robot: LoadedRobot) => Doc,
  ): Promise<boolean> => {
    let robot: LoadedRobot;
    try {
      robot = await loadRobot(await repository(), manifest, id);
    } catch (e) {
      toast({
        title: "Could not load the robot",
        description: e instanceof Error ? e.message : String(e),
        tone: "error",
      });
      return false;
    }
    return this.edit(`add ${id}`, (d) => change({ ...d, robots: [...d.robots, robot] }, robot));
  };

  /** Bake `doc` in place of the current document. */
  #apply(current: Baked, doc: Doc): boolean {
    let next: Baked;
    try {
      next = rebake(current, { ...current.loaded, scene: doc.scene, scenario: doc.scenario, robots: doc.robots });
    } catch (e) {
      const failure = describeFailure(e);
      const detail = failure.issues.map((i) => `${i.path}: ${i.message}`).join("\n");
      toast({ title: `Not applied: ${failure.title}`, description: detail || failure.message, tone: "error" });
      return false;
    }
    // The edit invalidates any dataset of the old document, and a run in flight for it.
    this.#generation++;
    this.playhead.setTimeline(next.baked.samples.length, next.baked.dt);
    this.#set({ current: next, failure: null, dataset: null });
    return true;
  }

  #publishHistory(): void {
    const current = this.#state.current;
    const doc = current ? docOf(current) : null;
    const saved = this.#saved;
    this.#set({
      undo: this.#undo.map((s) => s.label),
      redo: this.#redo.map((s) => s.label),
      dirty: doc !== null && (saved === null || doc.scene !== saved.scene || doc.scenario !== saved.scenario),
    });
  }

  /**
   * Save the scene and its scenario. In the Tauri shell to where they were opened from or last
   * saved (asking where when `as`, or when there is no such place yet: an example is never
   * overwritten); in a browser as downloads. Resolves to whether it saved.
   */
  readonly save = async (as = false): Promise<boolean> => {
    const current = this.#state.current;
    if (!current) return false;
    const doc = docOf(current);
    if (!isTauri()) {
      download({ name: "scene.json", text: jsonText(doc.scene), type: "application/json" });
      if (doc.scenario) download({ name: "scenario.json", text: jsonText(doc.scenario), type: "application/json" });
      this.#saved = doc;
      this.#publishHistory();
      return true;
    }
    let target = as ? null : this.#state.saveTo;
    if (target === null) {
      const dir = current.loaded.origin === null ? null : parentDir(current.loaded.origin);
      const scene = await saveDialog("Save the scene", "scene.json", ["json"], dir);
      if (scene === null) return false;
      target = { scene, scenario: doc.scenario ? scenarioPathFor(scene) : null };
    } else if (doc.scenario && target.scenario === null) {
      target = { ...target, scenario: scenarioPathFor(target.scene) };
    }
    try {
      const written = await saveScene({
        scenePath: target.scene,
        scene: doc.scene,
        scenarioPath: doc.scenario ? target.scenario : null,
        scenario: doc.scenario,
        from: current.loaded.origin === null ? null : parentDir(current.loaded.origin),
      });
      this.#rebased(written, target);
      toast({ title: "Saved", description: target.scene, tone: "success", duration: 2500 });
      return true;
    } catch (e) {
      toast({ title: "Could not save", description: e instanceof Error ? e.message : String(e), tone: "error" });
      return false;
    }
  };

  /**
   * After a save: the scene as written (its robot manifest paths relative to its new folder) is
   * the current one, and the history follows, so an undo does not bring back paths relative to
   * the old folder.
   */
  #rebased(written: SceneSpec, target: SavePaths): void {
    const current = this.#state.current;
    if (!current) return;
    const manifests = new Map((written.robots ?? []).map((r) => [r.id, r.manifest]));
    const rebase = (scene: SceneSpec): SceneSpec =>
      scene.robots?.some((r) => manifests.has(r.id) && manifests.get(r.id) !== r.manifest)
        ? { ...scene, robots: scene.robots.map((r) => ({ ...r, manifest: manifests.get(r.id) ?? r.manifest })) }
        : scene;
    const scene = rebase(current.loaded.scene);
    const loaded = {
      ...current.loaded,
      scene,
      origin: target.scene,
      scenarioOrigin: current.loaded.scenario ? target.scenario : current.loaded.scenarioOrigin,
      label: parentDir(target.scene).split("/").at(-1) ?? current.loaded.label,
    };
    // Only manifest strings changed: the session needs no update.
    const next: Baked = { ...current, loaded };
    for (const step of [...this.#undo, ...this.#redo]) step.doc = { ...step.doc, scene: rebase(step.doc.scene) };
    this.#saved = docOf(next);
    this.#set({ current: next, saveTo: target, example: "" });
    this.#publishHistory();
  }

  /** Record the dataset generated for the current scene. */
  readonly setDataset = (dataset: Dataset | null): void => this.#set({ dataset });

  /** Which load is current: it changes whenever a scene or scenario opens. */
  readonly generation = (): number => this.#generation;

  /** Record `dataset` if load `generation` is still the current one; whether it was. */
  readonly setDatasetFor = (generation: number, dataset: Dataset): boolean => {
    if (generation !== this.#generation) return false;
    this.#set({ dataset });
    return true;
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

  async #open(what: string, load: () => Promise<LoadedScene>, saveTo: SavePaths | null = null): Promise<void> {
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
      this.#undo = [];
      this.#redo = [];
      this.#saved = docOf(next);
      this.#set({
        current: next,
        failure: null,
        loading: false,
        dataset: null,
        saveTo,
        opened: this.#state.opened + 1,
      });
      this.#publishHistory();
      toast({ title: `Loaded ${what}`, tone: "success", duration: 2500 });
    } catch (e) {
      if (generation !== this.#generation) return;
      const failure = describeFailure(e);
      this.#set({ failure, loading: false });
      toast({ title: failure.title, description: failure.message, tone: "error" });
    }
  }
}

function docOf(baked: Baked): Doc {
  return { scene: baked.loaded.scene, scenario: baked.loaded.scenario, robots: baked.loaded.robots };
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
