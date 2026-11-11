/**
 * Where the studio reads from: the repository's examples and robot library, picked or dropped
 * files, and project and bank folders (on disk through the desktop shell, or in the browser).
 */

import { type LoadedScene, documentKind, loadScene } from "../io/load";
import { BANK_FILE, type Bank, type Project, loadBank, loadProject } from "../io/project";
import { folderSource, rememberFolder } from "../io/folder";
import { type FileSource, filePath, filesSource, urlSource } from "../io/source";
import { absolute, fsSource, isTauri, normalize, parentDir, splitAbsolute } from "../io/tauri";
import { dataRoot, openProjectDir, removeFile, writeText } from "../kernel/native";

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
export function repository(): Promise<FileSource> {
  repoSource ??= isTauri()
    ? dataRoot()
        .then((data) => (data === null ? served : fsSource(data.root, "repository")))
        .catch(() => served)
    : Promise.resolve(served);
  return repoSource;
}

/** Read files picked in the Tauri shell by absolute path: one scene, at most one scenario. */
export async function loadPaths(paths: string[]): Promise<LoadedScene> {
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
export async function loadDropped(files: File[]): Promise<LoadedScene> {
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

/** Where the desktop app remembers the last project folder. */
export const PROJECT_KEY = "etendue-studio:project";

export function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** Where the browser remembers the bank folder's handle. */
export const BANK_KEY = "bank";

/** A bank folder opened in the browser: made a bank (its manifest written) when it is not one. */
export async function browserBank(handle: Parameters<typeof folderSource>[0]): Promise<Bank> {
  const source = folderSource(handle);
  if (!(await source.has(BANK_FILE))) {
    await source.write(BANK_FILE, `${JSON.stringify({ version: 1, name: handle.name }, null, 2)}\n`);
  }
  const bank = await loadBank(source, source.write, handle.name, source.remove);
  await rememberFolder(handle, BANK_KEY);
  return bank;
}

export function folderName(dir: string): string {
  return normalize(dir).replace(/\/+$/, "").split("/").at(-1) ?? dir;
}

/** A project folder on disk, through the shell (which checks it and allows it). */
export async function tauriProject(dir: string): Promise<Project> {
  await openProjectDir(dir);
  try {
    localStorage.setItem(PROJECT_KEY, dir);
  } catch {
    // Remembering the project is a convenience.
  }
  return loadProject(
    fsSource(dir, folderName(dir)),
    "",
    (path, text) => writeText(absolute(dir, path), text),
    dir,
    (path) => removeFile(absolute(dir, path)),
  );
}

/** A project folder opened in the browser. */
export async function browserProject(handle: Parameters<typeof folderSource>[0]): Promise<Project> {
  const source = folderSource(handle);
  const project = await loadProject(source, "", source.write, handle.name, source.remove);
  await rememberFolder(handle);
  return project;
}

export async function pathExistsAbs(path: string): Promise<boolean> {
  const { root, relative } = splitAbsolute(path);
  return fsSource(root).has(relative);
}
