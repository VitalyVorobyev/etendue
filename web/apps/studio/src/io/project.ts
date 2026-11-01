/**
 * A project folder (ADR 0009): `etendue-project.json`, its cells and its component library, read
 * from a `FileSource`. The repository's `examples/` is the reference project, which the studio
 * opens at start, and its library is the built-in layer.
 *
 * The library a project shows is layered (ADR 0010): its own items, then the user's bank, then the
 * built-ins, each reference resolved from the highest layer that has it. Using a bank or built-in
 * item in the project copies it into the project's own library, with its origin.
 */

import {
  type BankManifest,
  type LibraryItem,
  type ProjectManifest,
  libraryItemHash,
  validateLibraryItem,
} from "@etendue/wasm";

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
  mount: "mounts",
  procedure: "procedures",
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

/** Where a library reference resolves (ADR 0010 §1), highest first. */
export type Layer = "project" | "bank" | "builtin";

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
  /**
   * Every library item the project sees, by reference (`cameras/imx174_lm25hc`): its own, then
   * the bank's, then the built-ins (`withLayers`). Without layers, its own.
   */
  library: Map<string, LibraryItem>;
  /** The project's own items (`library/` in its folder), that parsed and checked. */
  own: Map<string, LibraryItem>;
  /** The layer each reference of `library` resolves to, and the lower layers it hides. */
  layers: Map<string, { layer: Layer; hides: Layer[] }>;
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

  const own = await readLibrary(source, root, problems);
  const layers = new Map<string, { layer: Layer; hides: Layer[] }>(
    [...own.keys()].map((ref) => [ref, { layer: "project", hides: [] }]),
  );
  return { source, root, location, write, manifest, cells, library: own, own, layers, problems };
}

/**
 * Read the library under `root` in `source` (`library/<folder>/<id>.json`): every item that parses
 * and checks, by reference. What does not read is added to `problems`.
 */
export async function readLibrary(
  source: FileSource,
  root: string,
  problems: { path: string; message: string }[],
): Promise<Map<string, LibraryItem>> {
  const library = new Map<string, LibraryItem>();
  if (!source.list) return library;
  for (const folder of Object.values(LIBRARY_FOLDERS)) {
    const dir = join(root, `library/${folder}`);
    let names: string[];
    try {
      names = (await source.list(dir)).filter((n) => n.endsWith(".json"));
    } catch (e) {
      // A missing folder is an empty one (a bank need not have every kind).
      if (await source.has(join(root, `library/${folder}`))) {
        problems.push({ path: dir, message: e instanceof Error ? e.message : String(e) });
      }
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
  return library;
}

/** A user bank (ADR 0010): a folder with `etendue-bank.json` and a library. */
export interface Bank {
  source: FileSource;
  /** Where it is, for people. */
  location: string;
  /** How to write into it; `null` when it is read-only. */
  write: ProjectWriter | null;
  manifest: BankManifest;
  /** Its items, by reference. */
  library: Map<string, LibraryItem>;
  /** Library files that did not read, with why. */
  problems: { path: string; message: string }[];
}

/** The bank manifest's file name. */
export const BANK_FILE = "etendue-bank.json";

/** Read the bank at the root of `source`. A missing or unreadable manifest rejects. */
export async function loadBank(source: FileSource, write: ProjectWriter | null, location = source.name): Promise<Bank> {
  const manifest = JSON.parse(await source.readText(BANK_FILE)) as BankManifest;
  if (manifest.version !== 1)
    throw new Error(`${BANK_FILE}: bank version ${manifest.version}; this studio reads version 1`);
  const problems: Bank["problems"] = [];
  const library = await readLibrary(source, "", problems);
  return { source, location, write, manifest, library, problems };
}

/**
 * `project` seeing `bank` and `builtin` below its own items (ADR 0010 §1): `library` resolves each
 * reference from the highest layer that has it, and `layers` says which. The built-in project
 * itself (`isBuiltin`) is its own layer: its items are the built-ins.
 */
export function withLayers(
  project: Project,
  bank: Bank | null,
  builtin: Map<string, LibraryItem> | null,
  isBuiltin = false,
): Project {
  const stack: [Layer, Map<string, LibraryItem>][] = [
    [isBuiltin ? "builtin" : "project", project.own],
    ...(bank ? [["bank", bank.library] as [Layer, Map<string, LibraryItem>]] : []),
    ...(builtin && !isBuiltin ? [["builtin", builtin] as [Layer, Map<string, LibraryItem>]] : []),
  ];
  const library = new Map<string, LibraryItem>();
  const layers = new Map<string, { layer: Layer; hides: Layer[] }>();
  for (const [layer, items] of stack) {
    for (const [ref, item] of items) {
      const seen = layers.get(ref);
      if (seen) seen.hides.push(layer);
      else {
        library.set(ref, item);
        layers.set(ref, { layer, hides: [] });
      }
    }
  }
  return { ...project, library, layers };
}

/** Whether the item at `ref` is the project's own (not from the bank or the built-ins). */
export function isOwn(project: Project, ref: string): boolean {
  return project.own.has(ref);
}

/**
 * Copy the bank or built-in item at `ref` into the project's own library (ADR 0010 §3), with its
 * origin and the source's content hash. An item the project already owns is left as it is.
 * Returns the project with it.
 */
export async function useInProject(project: Project, ref: string): Promise<Project> {
  if (project.own.has(ref)) return project;
  const item = project.library.get(ref);
  const layer = project.layers.get(ref)?.layer;
  if (!item || !layer || layer === "project") throw new Error(`no library item \`${ref}\``);
  const copy: LibraryItem = {
    ...item,
    version: 2,
    origin: { layer, reference: ref, sha256: libraryItemHash(item) },
  };
  return saveLibraryItem(project, copy);
}

/** Whether the project's copy at `ref` differs from its source now (the source's hash changed). */
export function isStale(project: Project, sources: Map<string, LibraryItem>, ref: string): boolean {
  const origin = project.own.get(ref)?.origin;
  if (!origin) return false;
  const source = sources.get(origin.reference);
  return source !== undefined && libraryItemHash(source) !== origin.sha256;
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
  const ref = `${folder}/${item.id}`;
  const library = new Map(project.library);
  library.set(ref, item);
  const own = new Map(project.own);
  own.set(ref, item);
  const layers = new Map(project.layers);
  const below = project.layers.get(ref);
  layers.set(ref, {
    layer: "project",
    hides: below && below.layer !== "project" ? [below.layer, ...below.hides] : (below?.hides ?? []),
  });
  return { ...project, library, own, layers };
}

/** Write `item` into the bank's library, replacing an item with its id. Returns the bank with it. */
export async function saveToBank(bank: Bank, item: LibraryItem): Promise<Bank> {
  const folder = folderOf(item);
  if (!folder) throw new Error(`library item \`${item.id}\` holds no part`);
  if (!bank.write) throw new Error(`${bank.location} is read-only`);
  const stored = { ...item, version: 2 };
  delete stored.origin;
  validateLibraryItem(JSON.stringify(stored));
  await bank.write(`library/${folder}/${item.id}.json`, text(stored));
  const library = new Map(bank.library);
  library.set(`${folder}/${item.id}`, stored);
  return { ...bank, library };
}

/**
 * Make `write`'s folder a new project named `name`: a manifest whose defaults are `starter`'s, and
 * copies of the items those defaults name (ADR 0010 §3: a project owns what it uses; the rest of
 * the built-ins stay visible as their layer). No cells yet.
 */
export async function createProject(write: ProjectWriter, name: string, starter: Project): Promise<void> {
  const defaults = starter.manifest.defaults ?? {};
  const refs: (string | null | undefined)[] = [defaults.camera, defaults.laser, defaults.target];
  for (const ref of refs) {
    const item = ref ? starter.own.get(ref) : undefined;
    if (!item || !ref) continue;
    const copy: LibraryItem = {
      ...item,
      version: 2,
      origin: { layer: "builtin", reference: ref, sha256: libraryItemHash(item) },
    };
    await write(`library/${ref}.json`, text(copy));
  }
  const manifest: ProjectManifest = { version: 1, name, cells: [], defaults };
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
