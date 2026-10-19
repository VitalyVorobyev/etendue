/**
 * A project folder (ADR 0009): `etendue-project.json`, its cells and its component library, read
 * from a `FileSource`. The repository's `examples/` is the reference project, which the studio
 * opens at start.
 */

import { type LibraryItem, type ProjectManifest, validateLibraryItem } from "@etendue/wasm";

import type { FileSource } from "./source";

/** The manifest's file name. */
export const PROJECT_FILE = "etendue-project.json";

/** The library's folders, by kind (etendue-scene `KINDS`). */
export const LIBRARY_FOLDERS = {
  sensor: "sensors",
  lens: "lenses",
  laser: "lasers",
  target: "targets",
  camera: "cameras",
  head: "heads",
  rig: "rigs",
} as const;

export type LibraryKind = keyof typeof LIBRARY_FOLDERS;

/** A cell of the project: a folder with a scene and usually a scenario. */
export interface Cell {
  /** Its folder, relative to the project. */
  id: string;
  /** The scene's description. */
  description: string;
  /** Source paths. */
  scene: string;
  scenario: string | null;
}

/** Writes `text` to `path` (relative to the project folder), creating folders as needed. */
export type ProjectWriter = (path: string, text: string) => Promise<void>;

/** A project, read. */
export interface Project {
  source: FileSource;
  /** The project folder, as a path in `source` (`""` when the source is the folder). */
  root: string;
  /** Where it is, for people (a path, or a folder name). */
  location: string;
  /** How to write into it; `null` for a read-only project (the bundled examples). */
  write: ProjectWriter | null;
  manifest: ProjectManifest;
  cells: Cell[];
  /** Every library item that parsed and checked, by reference (`cameras/imx174_lm25hc`). */
  library: Map<string, LibraryItem>;
  /** Library files that did not, with why. */
  problems: { path: string; message: string }[];
}

const join = (root: string, path: string) => (root === "" ? path : `${root.replace(/\/$/, "")}/${path}`);

/**
 * Read the project at `root` in `source`. A cell or library item that does not read is left out
 * and reported in `problems`; a missing or unreadable manifest rejects.
 */
export async function loadProject(
  source: FileSource,
  root: string,
  write: ProjectWriter | null = null,
  location: string = root === "" ? source.name : root,
): Promise<Project> {
  const manifest = JSON.parse(await source.readText(join(root, PROJECT_FILE))) as ProjectManifest;
  if (manifest.version !== 1)
    throw new Error(`${PROJECT_FILE}: project version ${manifest.version}; this studio reads version 1`);
  const problems: Project["problems"] = [];
  const cells = (
    await Promise.all(
      (manifest.cells ?? []).map(async (id): Promise<Cell | null> => {
        const scene = join(root, `${id}/scene.json`);
        try {
          const doc = JSON.parse(await source.readText(scene)) as { description?: string | null };
          const scenarioPath = join(root, `${id}/scenario.json`);
          const scenario = (await source.has(scenarioPath)) ? scenarioPath : null;
          return { id, description: doc.description ?? "", scene, scenario };
        } catch (e) {
          problems.push({ path: scene, message: e instanceof Error ? e.message : String(e) });
          return null;
        }
      }),
    )
  ).filter((c): c is Cell => c !== null);

  const library = new Map<string, LibraryItem>();
  if (source.list) {
    for (const folder of Object.values(LIBRARY_FOLDERS)) {
      const dir = join(root, `library/${folder}`);
      let names: string[];
      try {
        names = (await source.list(dir)).filter((n) => n.endsWith(".json"));
      } catch (e) {
        problems.push({ path: dir, message: e instanceof Error ? e.message : String(e) });
        continue;
      }
      const items = await Promise.all(
        names.map(async (name) => {
          const path = `${dir}/${name}`;
          try {
            const item = validateLibraryItem(await source.readText(path));
            if (`${item.id}.json` !== name) throw new Error(`its id \`${item.id}\` is not its file name`);
            return [`${folder}/${item.id}`, item] as const;
          } catch (e) {
            problems.push({ path, message: e instanceof Error ? e.message : String(e) });
            return null;
          }
        }),
      );
      for (const entry of items) if (entry) library.set(entry[0], entry[1]);
    }
  }
  return { source, root, location, write, manifest, cells, library, problems };
}

/** The library folder an item belongs in, by the one part it holds. */
export function folderOf(item: LibraryItem): string | null {
  const kind = (Object.keys(LIBRARY_FOLDERS) as LibraryKind[]).find((k) => item[k] != null);
  return kind ? LIBRARY_FOLDERS[kind] : null;
}

const text = (doc: unknown) => `${JSON.stringify(doc, null, 2)}\n`;

function writable(project: Project): ProjectWriter {
  if (!project.write) throw new Error(`${project.location} is read-only: open or create a project to save into`);
  return project.write;
}

/**
 * Write `item` into the project's library (`library/<folder>/<id>.json`), replacing an item with
 * its id. Returns the project with it.
 */
export async function saveLibraryItem(project: Project, item: LibraryItem): Promise<Project> {
  const folder = folderOf(item);
  if (!folder) throw new Error(`library item \`${item.id}\` holds no part`);
  validateLibraryItem(JSON.stringify(item));
  await writable(project)(`library/${folder}/${item.id}.json`, text(item));
  const library = new Map(project.library);
  library.set(`${folder}/${item.id}`, item);
  return { ...project, library };
}

/**
 * Make `write`'s folder a new project named `name`: a manifest whose defaults are `starter`'s, and
 * a copy of `starter`'s library. No cells yet.
 */
export async function createProject(write: ProjectWriter, name: string, starter: Project): Promise<void> {
  for (const [ref, item] of starter.library) await write(`library/${ref}.json`, text(item));
  const manifest: ProjectManifest = { version: 1, name, cells: [], defaults: starter.manifest.defaults ?? {} };
  await write(PROJECT_FILE, text(manifest));
}

/**
 * Add cell `id` (a folder) with `scene` and, if given, `scenario` to the project, and list it in
 * the manifest. Returns the project with it.
 */
export async function addCell(project: Project, id: string, scene: unknown, scenario: unknown): Promise<Project> {
  const write = writable(project);
  if (project.manifest.cells?.includes(id)) throw new Error(`the project already has a cell \`${id}\``);
  await write(`${id}/scene.json`, text(scene));
  if (scenario != null) await write(`${id}/scenario.json`, text(scenario));
  const manifest = { ...project.manifest, cells: [...(project.manifest.cells ?? []), id] };
  await write(PROJECT_FILE, text(manifest));
  const description = (scene as { description?: string | null }).description ?? "";
  const cell: Cell = {
    id,
    description,
    scene: join(project.root, `${id}/scene.json`),
    scenario: scenario != null ? join(project.root, `${id}/scenario.json`) : null,
  };
  return { ...project, manifest, cells: [...project.cells, cell] };
}

/** The items of one kind, by reference, in name order. */
export function itemsOf(project: Project, kind: LibraryKind): [string, LibraryItem][] {
  const prefix = `${LIBRARY_FOLDERS[kind]}/`;
  return [...project.library]
    .filter(([ref, item]) => ref.startsWith(prefix) && item[kind] != null)
    .sort((a, b) => a[1].name.localeCompare(b[1].name));
}

/** The project's default item of `kind`, if it names one the library has. */
export function defaultOf(project: Project | null, kind: "camera" | "laser" | "target"): LibraryItem | undefined {
  const ref = project?.manifest.defaults?.[kind];
  return ref ? project?.library.get(ref) : undefined;
}
