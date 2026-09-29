import { Button, Select, ThemeToggle } from "@vitavision/ui";
import { FileDrop } from "@vitavision/workbench";
import { Aperture, FileInput, FolderOpen } from "lucide-react";
import examples from "virtual:etendue-examples";

import { isTauri } from "../io/tauri";
import { pickJsonFiles } from "../kernel/native";

/**
 * The top bar: product name, what is loaded, the example picker, open files, theme. In the
 * Tauri shell also “Open scene…” (by path, so the native pipeline can use it) and “Import
 * poses…” (a scenario from tool poses).
 */
export function Header({
  label,
  description,
  example,
  onExample,
  onFiles,
  onPaths,
  onPoses,
}: {
  label: string | null;
  description: string | null;
  example: string;
  onExample: (id: string) => void;
  onFiles: (files: File[]) => void;
  onPaths: (paths: string[]) => void;
  /** Import tool poses; `null` while nothing is loaded. */
  onPoses: ((path: string) => void) | null;
}) {
  const native = isTauri();
  return (
    <div className="flex h-11 items-center gap-3 border-b border-line bg-surface px-3">
      <div className="flex items-center gap-2 text-sm font-semibold text-fg">
        <Aperture className="size-4 text-signal" aria-hidden />
        etendue studio
      </div>
      {label && (
        <div className="min-w-0 truncate text-xs text-fg-muted" title={description ?? undefined}>
          <span className="font-mono text-fg">{label}</span>
          {description && <span className="ml-2">{description}</span>}
        </div>
      )}
      <div className="ml-auto flex items-center gap-2">
        <Select
          value={example}
          onValueChange={onExample}
          options={examples.map((e) => ({ value: e.id, label: e.id, note: e.scenario ? "scene + scenario" : "scene" }))}
          placeholder="Open an example…"
          aria-label="Example scene"
          className="w-56"
        />
        {native && (
          <>
            <Button
              icon={<FolderOpen className="size-4" />}
              onClick={() => void pickJsonFiles("Open a scene (and its scenario)", true).then(onPaths)}
            >
              Open scene…
            </Button>
            <Button
              icon={<FileInput className="size-4" />}
              disabled={onPoses === null}
              title="A stop-and-shoot scenario from a JSON list of tool poses"
              onClick={() => void pickJsonFiles("Tool poses", false).then(([p]) => p !== undefined && onPoses?.(p))}
            >
              Import poses…
            </Button>
          </>
        )}
        <FileDrop
          onFiles={onFiles}
          accept=".json,.urdf,.glb"
          multiple
          overlay
          buttonLabel="Open files…"
          overlayMessage="Drop a scene.json (with its scenario.json and robot assets)"
        />
        <ThemeToggle />
      </div>
    </div>
  );
}
