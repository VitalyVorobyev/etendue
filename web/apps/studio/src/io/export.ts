/**
 * Getting text out of the studio: in the Tauri shell a save dialog and a write through the shell;
 * in a browser a download.
 */

import { saveDialog, writeText } from "../kernel/native";
import { isTauri } from "./tauri";

/** A file to export. */
export interface ExportFile {
  /** Suggested file name, e.g. `scenario.json`. */
  name: string;
  text: string;
  /** MIME type of the download (browser). */
  type: string;
}

/** Download `file` (browser). */
export function download(file: ExportFile): void {
  const url = URL.createObjectURL(new Blob([file.text], { type: file.type }));
  const a = document.createElement("a");
  a.href = url;
  a.download = file.name;
  a.style.display = "none";
  document.body.append(a);
  a.click();
  a.remove();
  // The download has started from the click; the URL is no longer needed.
  setTimeout(() => URL.revokeObjectURL(url), 0);
}

/**
 * Export `file`: ask where (desktop) or download it (browser). Returns where it went (a path, or
 * the file name of a download), or `null` if the dialog was cancelled.
 */
export async function exportFile(title: string, file: ExportFile, dir: string | null = null): Promise<string | null> {
  if (!isTauri()) {
    download(file);
    return file.name;
  }
  const extension = file.name.includes(".") ? file.name.slice(file.name.lastIndexOf(".") + 1) : "";
  const path = await saveDialog(title, file.name, extension === "" ? [] : [extension], dir);
  if (path === null) return null;
  await writeText(path, file.text);
  return path;
}

/** A document as the studio writes it: pretty JSON with a final newline. */
export function jsonText(doc: unknown): string {
  return `${JSON.stringify(doc, null, 2)}\n`;
}
