/**
 * The library items of the bank and the open project (ADR 0010): saving designs, the Library
 * workspace's edits, copies into the project and imports. The studio store holds the bank and the
 * project; this changes them through its `host`.
 */

import { type LibraryItem, validateLibraryItem } from "@etendue/wasm";
import { toast } from "@vitavision/workbench";

import { canOpenFolders, pickFolder, rememberedFolder } from "../io/folder";
import {
  type Bank,
  copyOf,
  loadBank,
  refOf,
  removeFromBank,
  removeLibraryItem,
  renameInBank,
  renameLibraryItem,
  saveToBank,
  updateCopy,
  useInProject as copyIntoProject,
  saveLibraryItem as writeLibraryItem,
} from "../io/project";
import { absolute, fsSource, isTauri } from "../io/tauri";
import { openBankDir, removeFile, writeText } from "../kernel/native";
import { BANK_KEY, browserBank, folderName, message } from "./sources";
import type { Store, StudioState } from "./types";

/** What the library items need of the studio store: its state, and a way to change it. */
export interface LibraryHost {
  get(): StudioState;
  set(patch: Partial<StudioState>): void;
}

export class LibraryItems {
  readonly #host: LibraryHost;

  constructor(host: LibraryHost) {
    this.#host = host;
  }

  /** Open the user's bank: the desktop app's bank folder, or the browser folder remembered. */
  readonly openBank = async (): Promise<void> => {
    try {
      let bank: Bank | null = null;
      if (isTauri()) {
        const { dir } = await openBankDir();
        bank = await loadBank(
          fsSource(dir, folderName(dir)),
          (path, text) => writeText(absolute(dir, path), text),
          dir,
          (path) => removeFile(absolute(dir, path)),
        );
      } else if (canOpenFolders()) {
        const remembered = await rememberedFolder(BANK_KEY);
        if (remembered?.granted) bank = await browserBank(remembered.handle);
      }
      if (bank) {
        for (const p of bank.problems) console.warn(`bank: ${p.path}: ${p.message}`);
        this.#host.set({ bank });
      }
    } catch (e) {
      console.warn(`bank: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  /** Choose a folder as the bank (the browser; the desktop app's bank folder is fixed). */
  readonly chooseBank = async (): Promise<void> => {
    try {
      const handle = await pickFolder();
      if (handle === null) return;
      const bank = await browserBank(handle);
      this.#host.set({ bank });
      toast({ title: `Bank: ${bank.manifest.name}`, description: handle.name, tone: "success", duration: 2500 });
    } catch (e) {
      toast({
        title: "Could not open the bank",
        description: e instanceof Error ? e.message : String(e),
        tone: "error",
      });
    }
  };

  /** Save `item` into the bank. */
  readonly saveToBank = async (item: LibraryItem): Promise<boolean> => {
    const bank = this.#host.get().bank;
    if (!bank) return false;
    try {
      this.#host.set({ bank: await saveToBank(bank, item) });
      toast({
        title: `Saved ${item.name}`,
        description: `${bank.manifest.name} (bank)`,
        tone: "success",
        duration: 2500,
      });
      return true;
    } catch (e) {
      toast({
        title: "Could not save to the bank",
        description: e instanceof Error ? e.message : String(e),
        tone: "error",
      });
      return false;
    }
  };

  /** Copy the bank or built-in item at `ref` into the open project (ADR 0010 §3). */
  readonly copyIntoProject = async (ref: string): Promise<boolean> => {
    const project = this.#host.get().project;
    if (!project) return false;
    try {
      this.#host.set({ project: await copyIntoProject(project, ref) });
      return true;
    } catch (e) {
      toast({
        title: "Could not copy into the project",
        description: e instanceof Error ? e.message : String(e),
        tone: "error",
      });
      return false;
    }
  };

  /** Save `item` into the open project's library (a design from the Optics workspace). */
  readonly saveLibraryItem = async (item: LibraryItem): Promise<boolean> => {
    const project = this.#host.get().project;
    if (!project) return false;
    try {
      const next = await writeLibraryItem(project, item);
      this.#host.set({ project: next });
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

  /** Save `item` into `store` (the Library workspace's edits, new items, duplicates). */
  readonly putItem = (store: Store, item: LibraryItem): Promise<boolean> =>
    store === "bank" ? this.saveToBank(item) : this.saveLibraryItem(item);

  /** Delete the item at `ref` from `store`. */
  readonly deleteItem = async (store: Store, ref: string): Promise<boolean> => {
    const { bank, project } = this.#host.get();
    const name = (store === "bank" ? bank?.library : project?.own)?.get(ref)?.name ?? ref;
    try {
      if (store === "bank") {
        if (!bank) throw new Error("no bank is open");
        this.#host.set({ bank: await removeFromBank(bank, ref) });
      } else {
        if (!project) throw new Error("no project is open");
        this.#host.set({ project: await removeLibraryItem(project, ref) });
      }
      toast({ title: `Deleted ${name}`, tone: "success", duration: 2500 });
      return true;
    } catch (e) {
      toast({ title: `Could not delete ${name}`, description: message(e), tone: "error" });
      return false;
    }
  };

  /** Give the item at `ref` in `store` the id `id` (a new file; the old one removed). */
  readonly renameItem = async (store: Store, ref: string, id: string): Promise<boolean> => {
    const { bank, project } = this.#host.get();
    try {
      if (store === "bank") {
        if (!bank) throw new Error("no bank is open");
        this.#host.set({ bank: await renameInBank(bank, ref, id) });
      } else {
        if (!project) throw new Error("no project is open");
        this.#host.set({ project: await renameLibraryItem(project, ref, id) });
      }
      toast({ title: `Renamed to ${id}`, tone: "success", duration: 2500 });
      return true;
    } catch (e) {
      toast({ title: "Could not rename the item", description: message(e), tone: "error" });
      return false;
    }
  };

  /**
   * Copy `item`, the item at `ref` in the bank or the built-ins, into the open project (ADR 0010
   * §3), with its origin. A project item with its reference is replaced.
   */
  readonly useItemInProject = async (layer: "bank" | "builtin", ref: string, item: LibraryItem): Promise<boolean> => {
    const project = this.#host.get().project;
    if (!project) return false;
    try {
      this.#host.set({ project: await writeLibraryItem(project, copyOf(item, layer, ref)) });
      toast({
        title: `${item.name} is in the project`,
        description: `${project.manifest.name} library`,
        tone: "success",
        duration: 2500,
      });
      return true;
    } catch (e) {
      toast({ title: "Could not copy into the project", description: message(e), tone: "error" });
      return false;
    }
  };

  /** Replace the project's copy at `ref` with its source as it is now (keeping the copy's id). */
  readonly updateCopy = async (ref: string): Promise<boolean> => {
    const { project, bank, builtin } = this.#host.get();
    const origin = project?.own.get(ref)?.origin;
    if (!project || !origin) return false;
    const source = (origin.layer === "bank" ? bank?.library : builtin)?.get(origin.reference);
    try {
      if (!source)
        throw new Error(
          `its source \`${origin.reference}\` is not in the ${origin.layer === "bank" ? "bank" : "built-ins"}`,
        );
      this.#host.set({ project: await updateCopy(project, ref, source) });
      toast({
        title: `Updated ${source.name}`,
        description: `from the ${origin.layer === "bank" ? "bank" : "built-ins"}`,
        tone: "success",
        duration: 2500,
      });
      return true;
    } catch (e) {
      toast({ title: "Could not update the copy", description: message(e), tone: "error" });
      return false;
    }
  };

  /**
   * Import library item files into `store`: each is checked first, and one whose reference the
   * store has already is refused. Returns how many were imported.
   */
  readonly importItems = async (store: Store, files: { name: string; text: string }[]): Promise<number> => {
    let imported = 0;
    for (const file of files) {
      try {
        const item = validateLibraryItem(file.text);
        const ref = refOf(item);
        const { bank, project } = this.#host.get();
        const taken = store === "bank" ? bank?.library : project?.own;
        if (ref && taken?.has(ref)) throw new Error(`\`${ref}\` exists already: rename or delete it first`);
        if (!(await this.putItem(store, item))) continue;
        imported++;
      } catch (e) {
        toast({ title: `Could not import ${file.name}`, description: message(e), tone: "error" });
      }
    }
    return imported;
  };
}
