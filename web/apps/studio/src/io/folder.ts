/**
 * A project folder the user opened in the browser, through the File System Access API
 * (Chromium): read and written in place (ADR 0009 §5). The folder's handle is remembered in
 * IndexedDB; the browser asks again for permission after a reload.
 */

import type { FileSource } from "./source";

/** A source the studio can also write to. */
export interface WritableSource extends FileSource {
  /** Write `text` to `path`, creating the folders it needs. */
  write: (path: string, text: string) => Promise<void>;
}

/** The File System Access surface used here (Chromium's). */
interface DirectoryHandle {
  readonly kind: "directory";
  readonly name: string;
  getDirectoryHandle(name: string, options?: { create?: boolean }): Promise<DirectoryHandle>;
  getFileHandle(name: string, options?: { create?: boolean }): Promise<FileHandle>;
  values(): AsyncIterable<DirectoryHandle | FileHandle>;
  queryPermission?(options: { mode: "read" | "readwrite" }): Promise<PermissionState>;
  requestPermission?(options: { mode: "read" | "readwrite" }): Promise<PermissionState>;
}

interface FileHandle {
  readonly kind: "file";
  readonly name: string;
  getFile(): Promise<File>;
  createWritable(): Promise<{ write(data: string): Promise<void>; close(): Promise<void> }>;
}

/** Whether this browser can open folders in place. */
export function canOpenFolders(): boolean {
  return typeof window !== "undefined" && "showDirectoryPicker" in window;
}

/** Ask the user for a folder to read and write. `null` if they cancel. */
export async function pickFolder(): Promise<DirectoryHandle | null> {
  const picker = window as unknown as { showDirectoryPicker(o: object): Promise<DirectoryHandle> };
  try {
    return await picker.showDirectoryPicker({ mode: "readwrite", id: "etendue-project" });
  } catch (e) {
    if (e instanceof DOMException && e.name === "AbortError") return null;
    throw e;
  }
}

const parts = (path: string) => path.split("/").filter((p) => p !== "" && p !== ".");

async function folderOf(root: DirectoryHandle, path: string[], create: boolean): Promise<DirectoryHandle> {
  let dir = root;
  for (const name of path) {
    if (name === "..") throw new Error(`${path.join("/")}: outside the project`);
    dir = await dir.getDirectoryHandle(name, { create });
  }
  return dir;
}

async function fileOf(root: DirectoryHandle, path: string, create = false): Promise<FileHandle> {
  const p = parts(path);
  const name = p.pop();
  if (name === undefined) throw new Error(`${path}: not a file`);
  return (await folderOf(root, p, create)).getFileHandle(name, { create });
}

/** The folder `root` as a source the studio reads and writes. */
export function folderSource(root: DirectoryHandle): WritableSource {
  return {
    name: root.name,
    async has(path) {
      try {
        await fileOf(root, path);
        return true;
      } catch {
        return false;
      }
    },
    async readText(path) {
      return (await (await fileOf(root, path)).getFile()).text();
    },
    // Meshes of robots kept in a browser-opened project are not served; robots resolve from the
    // app's library.
    url: () => "",
    async list(dir) {
      let folder: DirectoryHandle;
      try {
        folder = await folderOf(root, parts(dir), false);
      } catch {
        return [];
      }
      const names: string[] = [];
      for await (const entry of folder.values()) if (entry.kind === "file") names.push(entry.name);
      return names.sort();
    },
    write: async (path, text) => {
      const writable = await (await fileOf(root, path, true)).createWritable();
      await writable.write(text);
      await writable.close();
    },
  };
}

const DB = "etendue-studio";
const STORE = "handles";
const KEY = "project";

function db(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const open = indexedDB.open(DB, 1);
    open.onupgradeneeded = () => open.result.createObjectStore(STORE);
    open.onsuccess = () => resolve(open.result);
    open.onerror = () => reject(open.error ?? new Error("IndexedDB did not open"));
  });
}

/** Remember `handle` as the project to reopen. A convenience: failures are ignored. */
export async function rememberFolder(handle: DirectoryHandle): Promise<void> {
  try {
    const d = await db();
    d.transaction(STORE, "readwrite").objectStore(STORE).put(handle, KEY);
  } catch {
    // Private windows and blocked storage: nothing is remembered.
  }
}

/** The remembered project folder, if any, and whether it may be used without asking. */
export async function rememberedFolder(): Promise<{ handle: DirectoryHandle; granted: boolean } | null> {
  try {
    const d = await db();
    const handle = await new Promise<DirectoryHandle | undefined>((resolve, reject) => {
      const get = d.transaction(STORE).objectStore(STORE).get(KEY);
      get.onsuccess = () => resolve(get.result as DirectoryHandle | undefined);
      get.onerror = () => reject(get.error ?? new Error("IndexedDB read failed"));
    });
    if (!handle) return null;
    const granted = (await handle.queryPermission?.({ mode: "readwrite" })) === "granted";
    return { handle, granted };
  } catch {
    return null;
  }
}

/** Ask again for permission to `handle` (needs a user gesture). Whether it was granted. */
export async function regrant(handle: DirectoryHandle): Promise<boolean> {
  return (await handle.requestPermission?.({ mode: "readwrite" })) === "granted";
}

export type { DirectoryHandle };
