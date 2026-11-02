/**
 * Gates G14.1 and G14.2 (docs/pivot/PLAN.md P14), the studio's side, on the real kernel (the wasm
 * checks and hashes items; nothing is mocked). The CLI's side is in
 * `crates/etendue-cli/tests/project_check.rs`.
 *
 * - G14.1: every built-in item round-trips byte-identically through the studio's save path.
 * - G14.2: a project that uses user-bank items is complete without the bank: its copies hold
 *   everything, and they hash as their sources did.
 */

import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { readdir, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";

import { type LibraryItem, initSync, libraryItemHash } from "@etendue/wasm";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

import {
  BANK_FILE,
  PROJECT_FILE,
  createProject,
  loadBank,
  loadProject,
  saveLibraryItem,
  saveToBank,
  useInProject,
  withLayers,
} from "./project";
import type { FileSource } from "./source";

/** The checkout: vitest runs in the studio's package folder (`web/apps/studio`). */
const ROOT = resolve(process.cwd(), "../../..");
const EXAMPLES = join(ROOT, "examples");

/** A folder on disk as the studio reads it. */
function disk(root: string): FileSource {
  const at = (path: string) => join(root, path);
  return {
    name: root,
    url: at,
    has: async (path) => {
      try {
        await readFile(at(path));
        return true;
      } catch {
        return false;
      }
    },
    readText: (path) => readFile(at(path), "utf8"),
    list: async (dir) => (await readdir(at(dir))).sort(),
  };
}

/** Writes into a folder on disk. */
const writer = (root: string) => (path: string, text: string) => {
  mkdirSync(dirname(join(root, path)), { recursive: true });
  writeFileSync(join(root, path), text);
  return Promise.resolve();
};

/** A checkout on Windows may have CRLF line ends; the studio writes LF. */
const lf = (text: string) => text.replace(/\r\n/g, "\n");

beforeAll(() => {
  initSync({ module: readFileSync(join(ROOT, "crates/etendue-wasm/pkg/etendue_wasm_bg.wasm")) });
});

describe("G14.1: built-in items through the studio's save path", () => {
  it("writes every built-in item back byte for byte", async () => {
    const builtin = await loadProject(disk(EXAMPLES), "");
    expect(builtin.problems).toEqual([]);
    expect(builtin.own.size).toBeGreaterThan(20);
    const written = new Map<string, string>();
    const capture = {
      ...builtin,
      write: (path: string, text: string) => Promise.resolve(void written.set(path, text)),
    };
    for (const item of builtin.own.values()) await saveLibraryItem(capture, item);
    expect(written.size).toBe(builtin.own.size);
    // The files the studio would write that differ from the built-ins: none.
    const differ = [...written].filter(([path, text]) => text !== lf(readFileSync(join(EXAMPLES, path), "utf8")));
    expect(differ.map(([path]) => path)).toEqual([]);
  });
});

describe("G14.2: a project that uses bank items, without the bank", () => {
  let dir = "";
  beforeAll(() => {
    dir = mkdtempSync(join(tmpdir(), "etendue-g14-"));
  });
  afterAll(() => rmSync(dir, { recursive: true, force: true }));

  it("keeps complete copies that hash as their sources did", async () => {
    const builtin = await loadProject(disk(EXAMPLES), "");
    const bankDir = join(dir, "bank");
    const projectDir = join(dir, "project");

    // A bank with two items of its own, made from built-ins.
    await writer(bankDir)(BANK_FILE, `${JSON.stringify({ version: 1, name: "G14.2 bank" }, null, 2)}\n`);
    let bank = await loadBank(disk(bankDir), writer(bankDir));
    const fromBuiltin = (ref: string, id: string, name: string): LibraryItem => {
      const { origin: _origin, ...item } = builtin.own.get(ref)!;
      return { ...item, id, name };
    };
    bank = await saveToBank(bank, fromBuiltin("cameras/imx174_lm25hc", "g14_camera", "G14.2 camera"));
    bank = await saveToBank(bank, fromBuiltin("targets/chess_9x6_25mm", "g14_board", "G14.2 board"));
    const sources = new Map(bank.library);

    // A project that uses them: copies, and its defaults name them.
    await createProject(writer(projectDir), "G14.2", builtin);
    let project = withLayers(await loadProject(disk(projectDir), "", writer(projectDir)), bank, builtin.own);
    project = await useInProject(project, "cameras/g14_camera");
    project = await useInProject(project, "targets/g14_board");
    const manifest = {
      ...project.manifest,
      defaults: { ...project.manifest.defaults, camera: "cameras/g14_camera", target: "targets/g14_board" },
    };
    await writer(projectDir)(PROJECT_FILE, `${JSON.stringify(manifest, null, 2)}\n`);

    // The bank goes.
    rmSync(bankDir, { recursive: true });

    const alone = await loadProject(disk(projectDir), "");
    expect(alone.problems).toEqual([]);
    for (const ref of ["cameras/g14_camera", "targets/g14_board"]) {
      const copy = alone.own.get(ref);
      const source = sources.get(ref)!;
      expect(copy).toBeDefined();
      const { origin, version: _v, ...held } = copy!;
      const { version: _sv, ...expected } = source;
      expect(held).toEqual(expected);
      expect(origin).toEqual({ layer: "bank", reference: ref, sha256: libraryItemHash(source) });
      expect(libraryItemHash(copy!)).toBe(origin!.sha256);
    }
    const defaults = alone.manifest.defaults ?? {};
    expect([defaults.camera, defaults.laser, defaults.target].every((ref) => ref && alone.own.has(ref))).toBe(true);
  });
});
