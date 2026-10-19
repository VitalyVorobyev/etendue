/**
 * The studio's document state — what is loaded, whether a load is running, why the last one
 * failed, whether playback runs — as an external store, so loads (async, started from events
 * or on mount) never set React state from an effect.
 */

import type { LibraryItem, ScenarioSpec, SceneSpec } from "@etendue/wasm";
import { type Playhead, toast } from "@vitavision/workbench";
import { useSyncExternalStore } from "react";

import { download, jsonText } from "../io/export";
import { type LoadedRobot, type LoadedScene, documentKind, loadRobot, loadScene } from "../io/load";
import {
  PROJECT_FILE,
  type Project,
  type ProjectWriter,
  addCell,
  createProject,
  loadProject,
  saveLibraryItem as writeLibraryItem,
} from "../io/project";
import { canOpenFolders, folderSource, pickFolder, regrant, rememberFolder, rememberedFolder } from "../io/folder";
import { type FileSource, filePath, filesSource, urlSource } from "../io/source";
import { absolute, fsSource, isTauri, normalize, parentDir, splitAbsolute } from "../io/tauri";
import { type Baked, type KernelFailure, bakeScene, describeFailure, initKernel, rebake } from "../kernel/etendue";
import {
  type Dataset,
  dataRoot,
  openProjectDir,
  pickDirectory,
  saveDialog,
  saveScene,
  writeText,
} from "../kernel/native";
import { type PoseOverride, overridesOf } from "../kernel/posed";
import { followers, sceneEffect } from "../scene/effect";

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
    preview: null,
    project: null,
    reopenable: null,
  };
  readonly #listeners = new Set<() => void>();
  #generation = 0;
  #undo: Step[] = [];
  #redo: Step[] = [];
  /** The document as opened or last saved. */
  #saved: Doc | null = null;
  #starting = false;
  /** The reference project (the bundled examples): what new projects start from. */
  #starter: Project | null = null;
  /** The browser folder to ask for again (`reopenable`). */
  #remembered: Parameters<typeof regrant>[0] | null = null;
  /** The open transaction: its label and the document before it. */
  #tx: { label: string; before: Doc } | null = null;
  /** A previewed document waiting for its bake (at most one bake per animation frame). */
  #pending: { doc: Doc; frame: number } | null = null;

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

  /** Open cell `id` of the open project (one of `project.cells`). */
  readonly openExample = (id: string): void => {
    const project = this.#state.project;
    if (project) this.#openCell(project, id);
  };

  /** Open cell `id` of `project`; it is the open project's cell only if `project` is open. */
  #openCell(project: Project, id: string): void {
    const cell = project.cells.find((c) => c.id === id);
    if (!cell) return;
    this.#set({ example: project === this.#state.project ? id : "" });
    void this.#open(cell.id, async () => loadScene(project.source, cell.scene, cell.scenario, await repository()));
  }

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
    this.commit();
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

  /**
   * Show an edit in progress: `change` gets the document as currently shown. Nothing enters the
   * history until {@link Studio.commit}; another label commits the open edit first.
   *
   * - A move of placed things is drawn over the current bake (no kernel call at all).
   * - An edit that moves nothing (intrinsics, a laser's fan) replaces the kernel's scene and
   *   keeps the bake.
   * - Anything else is baked, at most once per animation frame.
   *
   * A preview the kernel rejects is shown as the failure, without a toast; the commit reports it.
   */
  readonly preview = (label: string, change: (doc: Doc) => Doc): void => {
    if (this.#tx && this.#tx.label !== label) this.commit();
    const current = this.#state.current;
    if (!current) return;
    const shown = this.#state.preview?.doc ?? docOf(current);
    const doc = change(shown);
    if (doc.scene === shown.scene && doc.scenario === shown.scenario && doc.robots === shown.robots) return;
    this.#tx ??= { label, before: docOf(current) };
    this.#show(label, current, doc);
  };

  #show(label: string, current: Baked, doc: Doc): void {
    this.#unschedule();
    const applied = docOf(current);
    const effect =
      doc.robots === applied.robots && doc.scenario === applied.scenario
        ? sceneEffect(applied.scene, doc.scene)
        : ({ kind: "bake" } as const);
    if (effect.kind === "none") {
      this.#set({ preview: { label, doc, overrides: [] } });
    } else if (effect.kind === "poses") {
      const frames = current.baked.frames;
      const overrides = overridesOf(effect.moved, (id) => followers(doc.scene, frames, id));
      this.#set({ preview: { label, doc, overrides } });
    } else if (effect.kind === "params") {
      this.#apply(current, doc, true);
      this.#set({ preview: { label, doc, overrides: [] } });
    } else {
      this.#set({ preview: { label, doc, overrides: [] } });
      this.#pending = {
        doc,
        frame: requestAnimationFrame(() => {
          const pending = this.#pending;
          const now = this.#state.current;
          this.#pending = null;
          if (pending && now) this.#apply(now, pending.doc, true);
        }),
      };
    }
  }

  #unschedule(): void {
    if (this.#pending) cancelAnimationFrame(this.#pending.frame);
    this.#pending = null;
  }

  /**
   * End the edit in progress: bake what it shows and record it as one undo step. If the kernel
   * rejects it, the document goes back to what it was before the edit. Returns whether the edit
   * was applied (`true` when none was open).
   */
  readonly commit = (): boolean => {
    const tx = this.#tx;
    if (!tx) return true;
    this.#tx = null;
    this.#unschedule();
    const current = this.#state.current;
    const preview = this.#state.preview;
    if (!current) return false;
    const target = preview?.doc ?? docOf(current);
    const ok = sameDoc(target, docOf(current)) || this.#apply(current, target);
    this.#set({ preview: null });
    if (!ok) {
      const now = this.#state.current;
      if (now && !sameDoc(docOf(now), tx.before)) this.#apply(now, tx.before, true);
      this.#set({ failure: null });
      return false;
    }
    if (!sameDoc(target, tx.before)) {
      this.#undo = [...this.#undo, { label: tx.label, doc: tx.before, at: 0 }].slice(-HISTORY);
      this.#redo = [];
    }
    this.#publishHistory();
    return true;
  };

  /** Drop the edit in progress: the document goes back to what it was before it. */
  readonly cancel = (): void => {
    const tx = this.#tx;
    if (!tx) return;
    this.#tx = null;
    this.#unschedule();
    const current = this.#state.current;
    if (current && !sameDoc(docOf(current), tx.before)) this.#apply(current, tx.before, true);
    this.#set({ preview: null, failure: null });
  };

  /** Revert the last edit. */
  readonly undo = (): void => {
    this.commit();
    this.#travel(this.#undo, this.#redo);
  };

  /** Apply the last undone edit again. */
  readonly redo = (): void => {
    this.commit();
    this.#travel(this.#redo, this.#undo);
  };

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

  /** The library robot whose manifest is `manifest` (repository-relative), as `id`. */
  readonly loadLibraryRobot = async (manifest: string, id: string): Promise<LoadedRobot> =>
    loadRobot(await repository(), manifest, id);

  /**
   * Make robot `id` of the scene the library robot whose manifest is `manifest`, by `change`
   * (which updates its `RobotSpec`). One undoable edit; refused, like any edit, when the scene or
   * the program no longer validates (a program written for other joints).
   */
  readonly replaceRobot = async (
    manifest: string,
    id: string,
    change: (doc: Doc, robot: LoadedRobot) => Doc,
  ): Promise<boolean> => {
    let robot: LoadedRobot;
    try {
      robot = await this.loadLibraryRobot(manifest, id);
    } catch (e) {
      toast({
        title: "Could not load the robot",
        description: e instanceof Error ? e.message : String(e),
        tone: "error",
      });
      return false;
    }
    return this.edit(`replace ${id}`, (d) =>
      change({ ...d, robots: d.robots.map((r) => (r.id === id ? robot : r)) }, robot),
    );
  };

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

  /**
   * Bake `doc` in place of the current document. A failure is shown as a toast, or `quiet`ly as
   * the state's failure (a preview in progress).
   */
  #apply(current: Baked, doc: Doc, quiet = false): boolean {
    let next: Baked;
    try {
      next = rebake(current, { ...current.loaded, scene: doc.scene, scenario: doc.scenario, robots: doc.robots });
    } catch (e) {
      const failure = describeFailure(e);
      if (quiet) {
        this.#set({ failure });
      } else {
        const detail = failure.issues.map((i) => `${i.path}: ${i.message}`).join("\n");
        toast({ title: `Not applied: ${failure.title}`, description: detail || failure.message, tone: "error" });
      }
      return false;
    }
    // The edit invalidates any dataset of the old document, and a run in flight for it.
    this.#generation++;
    if (next.baked !== current.baked) this.playhead.setTimeline(next.baked.samples.length, next.baked.dt);
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
    this.commit();
    const current = this.#state.current;
    if (!current) return false;
    const doc = docOf(current);
    // A cell of a project opened for writing is saved where it is.
    const project = this.#state.project;
    const cell = project?.cells.find((c) => c.id === this.#state.example);
    if (!as && project?.write && cell) {
      try {
        await project.write(`${cell.id}/scene.json`, jsonText(doc.scene));
        if (doc.scenario) await project.write(`${cell.id}/scenario.json`, jsonText(doc.scenario));
        this.#saved = doc;
        this.#publishHistory();
        toast({
          title: "Saved",
          description: `${project.manifest.name} / ${cell.id}`,
          tone: "success",
          duration: 2500,
        });
        return true;
      } catch (e) {
        toast({ title: "Could not save", description: e instanceof Error ? e.message : String(e), tone: "error" });
        return false;
      }
    }
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

  /**
   * Open the reference project (the repository's `examples/`), then its cell `id` (or its first),
   * unless something is loaded or loading already (safe to call on every mount).
   */
  start(id: string): void {
    if (this.#state.current !== null || this.#state.loading || this.#generation !== 0 || this.#starting) return;
    this.#starting = true;
    this.#set({ loading: true });
    // Library items are checked by the kernel.
    void Promise.all([repository(), initKernel()])
      .then(([repo]) => loadProject(repo, "examples"))
      .then(async (starter) => {
        this.#starter = starter;
        // An example asked for by name wins; otherwise the project last opened, if it still opens.
        const project = (id === "" ? await this.#reopen() : null) ?? starter;
        for (const p of project.problems) console.warn(`project: ${p.path}: ${p.message}`);
        this.#set({ project, loading: false });
        if (this.#state.current === null && this.#generation === 0) {
          // A project without cells yet starts from an example, to save into it as a cell.
          const from = project.cells.length > 0 ? project : starter;
          this.#openCell(from, from.cells.some((c) => c.id === id) ? id : (from.cells[0]?.id ?? ""));
        }
      })
      .catch((e: unknown) => {
        const failure = describeFailure(e);
        this.#set({ failure: { ...failure, title: "The examples project did not open" }, loading: false });
      });
  }

  /** The project of the last session, if it opens without asking (else `reopenable` is set). */
  async #reopen(): Promise<Project | null> {
    try {
      if (isTauri()) {
        const dir = localStorage.getItem(PROJECT_KEY);
        return dir ? await tauriProject(dir) : null;
      }
      const remembered = canOpenFolders() ? await rememberedFolder() : null;
      if (!remembered) return null;
      if (remembered.granted) return await browserProject(remembered.handle);
      this.#remembered = remembered.handle;
      this.#set({ reopenable: remembered.handle.name });
      return null;
    } catch {
      return null;
    }
  }

  /** Whether this studio can open and create project folders here. */
  readonly canOpenProjects = (): boolean => isTauri() || canOpenFolders();

  /** Ask for a project folder and open it, or (`create`) make a new project there first. */
  readonly openProject = async (create = false): Promise<void> => {
    try {
      let project: Project | null;
      if (isTauri()) {
        const dir = await pickDirectory(create ? "Choose a folder for the new project" : "Open a project folder");
        if (dir === null) return;
        if (create) await this.#create((path, text) => writeText(absolute(dir, path), text), folderName(dir), dir);
        project = await tauriProject(dir);
      } else {
        const handle = await pickFolder();
        if (handle === null) return;
        const source = folderSource(handle);
        if (create) await this.#create(source.write, handle.name, handle.name, await source.has(PROJECT_FILE));
        project = await browserProject(handle);
      }
      this.#use(project);
    } catch (e) {
      toast({
        title: create ? "Could not create the project" : "Could not open the project",
        description: e instanceof Error ? e.message : String(e),
        tone: "error",
      });
    }
  };

  /** Allow the remembered browser folder again (a click) and open it. */
  readonly reopenProject = async (): Promise<void> => {
    const handle = this.#remembered;
    if (!handle || !(await regrant(handle))) return;
    this.#remembered = null;
    this.#set({ reopenable: null });
    this.#use(await browserProject(handle));
  };

  async #create(write: ProjectWriter, name: string, where: string, exists = false): Promise<void> {
    if (!this.#starter) throw new Error("the examples project has not loaded");
    if (exists || (isTauri() && (await pathExistsAbs(absolute(where, PROJECT_FILE))))) {
      throw new Error(`${where} is already a project: open it instead`);
    }
    await createProject(write, name, this.#starter);
  }

  #use(project: Project): void {
    this.commit();
    for (const p of project.problems) console.warn(`project: ${p.path}: ${p.message}`);
    this.#set({ project });
    if (project.cells[0]) this.openExample(project.cells[0].id);
    else this.#set({ example: "" });
    toast({ title: `Opened ${project.manifest.name}`, description: project.location, tone: "success", duration: 2500 });
  }

  /** Save `item` into the open project's library (a design from the Optics workspace). */
  readonly saveLibraryItem = async (item: LibraryItem): Promise<boolean> => {
    const project = this.#state.project;
    if (!project) return false;
    try {
      const next = await writeLibraryItem(project, item);
      this.#set({ project: next });
      toast({
        title: `Saved ${item.name}`,
        description: `${project.manifest.name} library`,
        tone: "success",
        duration: 2500,
      });
      return true;
    } catch (e) {
      toast({
        title: "Could not save to the library",
        description: e instanceof Error ? e.message : String(e),
        tone: "error",
      });
      return false;
    }
  };

  /** Save the open scene and scenario as a new cell `id` of the open project. */
  readonly saveAsCell = async (id: string): Promise<boolean> => {
    this.commit();
    const current = this.#state.current;
    const project = this.#state.project;
    if (!current || !project) return false;
    try {
      const next = await addCell(project, id, current.loaded.scene, current.loaded.scenario);
      this.#saved = docOf(current);
      this.#set({ project: next, example: id });
      this.#publishHistory();
      toast({ title: `Saved as ${id}`, description: project.location, tone: "success", duration: 2500 });
      return true;
    } catch (e) {
      toast({
        title: "Could not save the cell",
        description: e instanceof Error ? e.message : String(e),
        tone: "error",
      });
      return false;
    }
  };

  /** Free the kernel session. */
  dispose(): void {
    this.#state.current?.session.free();
  }

  async #open(what: string, load: () => Promise<LoadedScene>, saveTo: SavePaths | null = null): Promise<void> {
    this.commit();
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

/** Where the desktop app remembers the last project folder. */
const PROJECT_KEY = "etendue-studio:project";

function folderName(dir: string): string {
  return normalize(dir).replace(/\/+$/, "").split("/").at(-1) ?? dir;
}

/** A project folder on disk, through the shell (which checks it and allows it). */
async function tauriProject(dir: string): Promise<Project> {
  await openProjectDir(dir);
  try {
    localStorage.setItem(PROJECT_KEY, dir);
  } catch {
    // Remembering the project is a convenience.
  }
  return loadProject(fsSource(dir, folderName(dir)), "", (path, text) => writeText(absolute(dir, path), text), dir);
}

/** A project folder opened in the browser. */
async function browserProject(handle: Parameters<typeof folderSource>[0]): Promise<Project> {
  const source = folderSource(handle);
  const project = await loadProject(source, "", source.write, handle.name);
  await rememberFolder(handle);
  return project;
}

async function pathExistsAbs(path: string): Promise<boolean> {
  const { root, relative } = splitAbsolute(path);
  return fsSource(root).has(relative);
}

function docOf(baked: Baked): Doc {
  return { scene: baked.loaded.scene, scenario: baked.loaded.scenario, robots: baked.loaded.robots };
}

function sameDoc(a: Doc, b: Doc): boolean {
  return a.scene === b.scene && a.scenario === b.scenario && a.robots === b.robots;
}

/** The studio's current state, re-rendering on change. */
export function useStudio(studio: Studio): StudioState {
  return useSyncExternalStore(studio.subscribe, studio.get, studio.get);
}

/** The cell named by `?example=`, if any (the project's first otherwise). */
export function initialExample(): string {
  return new URLSearchParams(window.location.search).get("example") ?? "";
}
