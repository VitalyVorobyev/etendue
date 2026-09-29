/**
 * The studio inside its Tauri shell (ADR 0007): files come from the filesystem through the
 * shell's commands, and assets (meshes, rendered images) through its asset protocol. In a
 * plain browser none of this exists and `isTauri()` is false.
 */

import { convertFileSrc, invoke } from "@tauri-apps/api/core";

import type { FileSource } from "./source";

/** Whether the page runs in the Tauri shell (its IPC internals are injected). */
export function isTauri(): boolean {
  return typeof window !== "undefined" && (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ != null;
}

/** `path` with `/` separators (Windows paths come back from dialogs with `\`). */
export function normalize(path: string): string {
  return path.replace(/\\/g, "/");
}

/**
 * An absolute path as a `FileSource` root and a path relative to it. POSIX paths hang off
 * `/`; Windows paths keep their drive (`C:/…`) in the relative part under an empty root, so
 * `join()` (which drops a leading `/`) never loses it.
 */
export function splitAbsolute(path: string): { root: string; relative: string } {
  const p = normalize(path);
  return p.startsWith("/") ? { root: "/", relative: p.replace(/^\/+/, "") } : { root: "", relative: p };
}

/** The absolute path of `relative` under `root` (the inverse of `splitAbsolute`). */
export function absolute(root: string, relative: string): string {
  if (root === "") return relative;
  return `${root.replace(/\/+$/, "")}/${relative}`;
}

/** The directory of an absolute path. */
export function parentDir(path: string): string {
  const p = normalize(path).replace(/\/+$/, "");
  const i = p.lastIndexOf("/");
  return i <= 0 ? (i === 0 ? "/" : p) : p.slice(0, i);
}

/** A readable file tree under `root` on the filesystem, read through the shell. */
export function fsSource(root: string, name = root === "/" || root === "" ? "filesystem" : root): FileSource {
  const abs = (path: string) => absolute(root, path);
  return {
    name,
    has: (path) => invoke<boolean>("path_exists", { path: abs(path) }),
    readText: (path) => invoke<string>("read_text", { path: abs(path) }),
    url: (path) => convertFileSrc(abs(path)),
    absolute: abs,
  };
}

/** A URL the webview can load an absolute file from (a rendered image, say). */
export function fileUrl(path: string): string {
  return convertFileSrc(normalize(path));
}
