/*
 * What `FileDrop` does with a drop: which files match `accept`, and what a dropped folder
 * contains.
 *
 * The browser enforces `accept` only in the file picker, and only as a hint; a drop brings
 * whatever was dragged. So the filter is applied here, to both routes. And a dropped folder
 * arrives as a zero-byte "file" unless its entries are walked — which is how a scenario
 * folder (a scene, a scenario, its meshes) is opened in one gesture.
 */

/**
 * Whether a file matches an `accept` string, as `<input type="file" accept>` reads it:
 * comma-separated extensions (`.json`), MIME types (`application/json`) and MIME wildcards
 * (`image/*`). Matching is case-insensitive.
 *
 * @param file - The file's name and MIME type.
 * @param accept - The accept string; empty or absent accepts everything.
 * @returns Whether the file is accepted.
 */
export function acceptsFile(file: { name: string; type: string }, accept: string | undefined): boolean {
  const tokens = (accept ?? "")
    .split(",")
    .map((token) => token.trim().toLowerCase())
    .filter((token) => token !== "");
  if (tokens.length === 0) return true;
  const name = file.name.toLowerCase();
  const type = file.type.toLowerCase();
  return tokens.some((token) => {
    if (token.startsWith(".")) return name.endsWith(token);
    if (token.endsWith("/*")) return type.startsWith(token.slice(0, -1));
    return type === token;
  });
}

/**
 * Split files by `accept`.
 *
 * @param files - The candidates.
 * @param accept - The accept string.
 * @returns The accepted files and the rejected ones, each in their original order.
 */
export function partitionFiles(
  files: readonly File[],
  accept: string | undefined,
): { accepted: File[]; rejected: File[] } {
  const accepted: File[] = [];
  const rejected: File[] = [];
  for (const file of files) (acceptsFile(file, accept) ? accepted : rejected).push(file);
  return { accepted, rejected };
}

/**
 * Whether a drag carries files (rather than text or a link) — the only drags `FileDrop`
 * reacts to.
 *
 * @param dataTransfer - The drag's data, or `null`.
 * @returns Whether `Files` is among its types.
 */
export function carriesFiles(dataTransfer: Pick<DataTransfer, "types"> | null): boolean {
  return dataTransfer !== null && Array.from(dataTransfer.types).includes("Files");
}

/** The part of the File and Directory Entries API a drop walk needs (`webkitGetAsEntry`). */
export interface DropEntry {
  /** A file (`FileSystemFileEntry`). */
  isFile: boolean;
  /** A folder (`FileSystemDirectoryEntry`). */
  isDirectory: boolean;
  /** The entry's own name. */
  name: string;
  /** The entry's path from the drop root, with a leading `/`. */
  fullPath: string;
  /** A file entry's contents, as a `File`. */
  file?: (success: (file: File) => void, failure?: (error: unknown) => void) => void;
  /** A folder entry's reader; `readEntries` yields the children in batches, then an empty one. */
  createReader?: () => {
    readEntries: (success: (entries: DropEntry[]) => void, failure?: (error: unknown) => void) => void;
  };
}

/**
 * Every file in a drop, with dropped folders walked recursively.
 *
 * Files found inside a folder carry `webkitRelativePath` (`scene/meshes/base.glb`), as the
 * files of `<input webkitdirectory>` do, so both routes give the same shape. Where the entries
 * API is missing, the drop's plain file list is returned.
 *
 * @param dataTransfer - The drop's data.
 * @returns The files, in the order the browser listed them.
 */
export async function collectDroppedFiles(
  dataTransfer: Pick<DataTransfer, "files" | "items">,
): Promise<File[]> {
  // Everything is read before the first `await`: a drop's data is emptied once its event
  // handler returns.
  const plain = Array.from(dataTransfer.files);
  const entries = Array.from(dataTransfer.items ?? [])
    .filter((item) => item.kind === "file")
    .map((item) => (typeof item.webkitGetAsEntry === "function" ? (item.webkitGetAsEntry() as DropEntry | null) : null));
  // No folder among them (or no entries API): the plain list is already the answer.
  if (!entries.some((entry) => entry?.isDirectory)) return plain;
  const files: File[] = [];
  for (const entry of entries) if (entry) await walkEntry(entry, files);
  return files;
}

async function walkEntry(entry: DropEntry, out: File[]): Promise<void> {
  if (entry.isFile && entry.file) {
    const read = entry.file.bind(entry);
    const file = await new Promise<File>((resolve, reject) => read(resolve, reject));
    const path = entry.fullPath.replace(/^\//, "");
    out.push(path.includes("/") ? withRelativePath(file, path) : file);
  } else if (entry.isDirectory && entry.createReader) {
    const reader = entry.createReader();
    // `readEntries` returns a directory in batches (100 in Chromium) until an empty one.
    for (;;) {
      const batch = await new Promise<DropEntry[]>((resolve, reject) => reader.readEntries(resolve, reject));
      if (batch.length === 0) break;
      for (const child of batch) await walkEntry(child, out);
    }
  }
}

/**
 * The file, carrying `webkitRelativePath` — a read-only accessor on `File.prototype`,
 * shadowed here on the instance.
 */
function withRelativePath(file: File, path: string): File {
  Object.defineProperty(file, "webkitRelativePath", { value: path, configurable: true });
  return file;
}
