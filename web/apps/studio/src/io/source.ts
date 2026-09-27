/**
 * Where scene files come from: the repository (served under `/repo/`), or files the user
 * dropped or picked. Paths are POSIX, relative to the source root.
 */

/** A readable tree of files. */
export interface FileSource {
  /** Human-readable name, for messages. */
  readonly name: string;
  /** Whether `path` exists (without reading it). */
  has(path: string): boolean | Promise<boolean>;
  /** The file's text. Rejects if it does not exist. */
  readText(path: string): Promise<string>;
  /** A URL the browser can fetch the file from (for meshes). */
  url(path: string): string;
}

/** Files served over HTTP under `base` (e.g. `/repo/`). */
export function urlSource(base: string, name = base): FileSource {
  const url = (path: string) => `${base.replace(/\/$/, "")}/${path}`;
  return {
    name,
    url,
    async has(path) {
      const res = await fetch(url(path), { method: "HEAD" });
      return res.ok;
    },
    async readText(path) {
      const res = await fetch(url(path));
      if (!res.ok) throw new Error(`${name}: ${path}: HTTP ${res.status}`);
      return res.text();
    },
  };
}

/** The path a dropped file is known by: its path inside a dropped folder, or its name. */
export function filePath(file: File): string {
  const relative = (file as File & { webkitRelativePath?: string }).webkitRelativePath;
  return relative !== undefined && relative !== "" ? relative : file.name;
}

/**
 * Dropped or picked files. A file is found by its exact path, or — for loose files dropped
 * without their folders — by its path's trailing components (`robots/ur5e/robot.json` finds
 * a dropped `robot.json` inside a `ur5e` folder, or a loose `robot.json` if it is the only one).
 */
export function filesSource(files: readonly File[]): FileSource & { paths: string[]; dispose(): void } {
  const byPath = new Map(files.map((f) => [filePath(f), f]));
  const urls = new Map<string, string>();
  const find = (path: string): File | undefined => {
    const exact = byPath.get(path);
    if (exact) return exact;
    const wanted = path.split("/");
    let best: File | undefined;
    let bestScore = 0;
    let tie = false;
    for (const [p, f] of byPath) {
      const have = p.split("/");
      let score = 0;
      while (score < have.length && score < wanted.length && have.at(-1 - score) === wanted.at(-1 - score)) score++;
      if (score > bestScore) {
        best = f;
        bestScore = score;
        tie = false;
      } else if (score === bestScore && score > 0) {
        tie = true;
      }
    }
    return bestScore > 0 && !tie ? best : undefined;
  };
  return {
    name: "dropped files",
    paths: [...byPath.keys()],
    has: (path) => find(path) !== undefined,
    async readText(path) {
      const f = find(path);
      if (!f) throw new Error(`dropped files: ${path} not found`);
      return f.text();
    },
    url(path) {
      const f = find(path);
      if (!f) return "";
      let u = urls.get(filePath(f));
      if (!u) {
        u = URL.createObjectURL(f);
        urls.set(filePath(f), u);
      }
      return u;
    },
    dispose() {
      for (const u of urls.values()) URL.revokeObjectURL(u);
      urls.clear();
    },
  };
}
