import {
  Button,
  DropdownMenu,
  MenuItem,
  MenuLabel,
  MenuSeparator,
  SegmentedControl,
  ThemeToggle,
} from "@vitavision/ui";
import { FileDrop } from "@vitavision/workbench";
import {
  Aperture,
  Check,
  Download,
  FileInput,
  FolderOpen,
  FolderPlus,
  FolderSync,
  Menu as MenuIcon,
  Redo2,
  Save,
  SaveAll,
  Undo2,
} from "lucide-react";
import { useRef } from "react";
import type { Cell } from "../io/project";

import { isTauri } from "../io/tauri";
import { pickJsonFiles } from "../kernel/native";

/** The studio's workspaces. Each keeps the 3D view or the camera images as its main surface. */
export type Mode = "optics" | "rigs" | "setup" | "motion" | "images" | "dataset";

export const MODES: { value: Mode; label: string }[] = [
  { value: "optics", label: "Optics" },
  { value: "rigs", label: "Rigs" },
  { value: "setup", label: "Setup" },
  { value: "motion", label: "Motion" },
  { value: "images", label: "Images" },
  { value: "dataset", label: "Dataset" },
];

/** One entry of the File ▸ Export section. */
export interface ExportItem {
  label: string;
  run: () => void;
}

const MOD = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform) ? "⌘" : "Ctrl+";

/**
 * The top bar: the product, the open document (● while it has unsaved changes), the workspace
 * switch, undo and redo, the File menu and the theme. Everything that opens, saves or exports is
 * in the File menu, so the bar stays quiet.
 */
export function Header({
  label,
  description,
  dirty,
  mode,
  modes,
  onMode,
  undo,
  redo,
  onUndo,
  onRedo,
  example,
  project,
  cells,
  onExample,
  onProject,
  reopenable,
  onReopen,
  onSaveAsCell,
  onFiles,
  onPaths,
  onPoses,
  onSave,
  exports,
}: {
  label: string | null;
  description: string | null;
  dirty: boolean;
  mode: Mode;
  modes: Mode[];
  onMode: (mode: Mode) => void;
  /** What Undo and Redo would do, latest last; empty when there is nothing to do. */
  undo: string[];
  redo: string[];
  onUndo: () => void;
  onRedo: () => void;
  example: string;
  /** The open project: its name, and whether it can be saved into. */
  project: { name: string; writable: boolean } | null;
  /** The open project's cells. */
  cells: readonly Cell[];
  onExample: (id: string) => void;
  /** Open (or, with `create`, make) a project folder; `null` where folders cannot be opened. */
  onProject: ((create: boolean) => void) | null;
  /** A project of an earlier session the browser must be allowed again, by name. */
  reopenable: string | null;
  onReopen: () => void;
  /** Save the scene as a new cell of the open project; `null` when it cannot. */
  onSaveAsCell: (() => void) | null;
  onFiles: (files: File[]) => void;
  onPaths: (paths: string[]) => void;
  /** Import tool poses; `null` while nothing is loaded. */
  onPoses: ((path: string) => void) | null;
  /** Save (`as`: ask where); `null` while nothing is loaded. */
  onSave: ((as: boolean) => void) | null;
  exports: ExportItem[];
}) {
  const native = isTauri();
  const input = useRef<HTMLInputElement>(null);
  const lastUndo = undo.at(-1);
  const lastRedo = redo.at(-1);
  return (
    <div className="flex h-11 items-center gap-3 border-b border-line bg-surface px-3">
      <div className="flex shrink-0 items-center gap-2 text-sm font-semibold text-fg">
        <Aperture className="size-4 text-signal" aria-hidden />
        etendue
      </div>
      {label && (
        <div className="min-w-0 flex-1 truncate text-xs text-fg-muted" title={description ?? undefined}>
          <span className="font-mono text-fg" data-testid="document-label">
            {label}
          </span>
          {dirty && (
            <span className="ml-1 text-signal" title="Unsaved changes" data-testid="dirty">
              ●
            </span>
          )}
        </div>
      )}
      {!label && <div className="flex-1" />}
      <SegmentedControl
        value={mode}
        options={MODES.filter((m) => modes.includes(m.value))}
        onValueChange={(v) => v !== "" && onMode(v as Mode)}
        aria-label="Workspace"
      />
      <div className="flex flex-1 items-center justify-end gap-1">
        <Button
          variant="ghost"
          size="sm"
          icon={<Undo2 className="size-4" />}
          aria-label={lastUndo ? `Undo ${lastUndo}` : "Undo"}
          title={lastUndo ? `Undo ${lastUndo} (${MOD}Z)` : "Nothing to undo"}
          disabled={!lastUndo}
          onClick={onUndo}
        />
        <Button
          variant="ghost"
          size="sm"
          icon={<Redo2 className="size-4" />}
          aria-label={lastRedo ? `Redo ${lastRedo}` : "Redo"}
          title={lastRedo ? `Redo ${lastRedo} (${MOD}⇧Z)` : "Nothing to redo"}
          disabled={!lastRedo}
          onClick={onRedo}
        />
        <DropdownMenu
          align="end"
          trigger={
            <Button size="sm" icon={<MenuIcon className="size-4" />}>
              File
            </Button>
          }
        >
          {onProject && (
            <>
              <MenuItem icon={<FolderOpen className="size-4" />} onSelect={() => onProject(false)}>
                Open project…
              </MenuItem>
              <MenuItem icon={<FolderPlus className="size-4" />} onSelect={() => onProject(true)}>
                New project…
              </MenuItem>
              {reopenable !== null && (
                <MenuItem icon={<FolderSync className="size-4" />} onSelect={onReopen}>
                  Reopen {reopenable}
                </MenuItem>
              )}
              <MenuSeparator />
            </>
          )}
          {native ? (
            <MenuItem
              icon={<FolderOpen className="size-4" />}
              onSelect={() => void pickJsonFiles("Open a scene (and its scenario)", true).then(onPaths)}
            >
              Open scene…
            </MenuItem>
          ) : (
            <MenuItem icon={<FolderOpen className="size-4" />} onSelect={() => input.current?.click()}>
              Open files…
            </MenuItem>
          )}
          {native && (
            <MenuItem
              icon={<FileInput className="size-4" />}
              disabled={onPoses === null}
              onSelect={() => void pickJsonFiles("Tool poses", false).then(([p]) => p !== undefined && onPoses?.(p))}
            >
              Import tool poses…
            </MenuItem>
          )}
          <MenuSeparator />
          <MenuItem
            icon={<Save className="size-4" />}
            shortcut={`${MOD}S`}
            disabled={onSave === null}
            onSelect={() => onSave?.(false)}
          >
            {native || (project?.writable && cells.some((c) => c.id === example)) ? "Save" : "Download scene"}
          </MenuItem>
          {native && (
            <MenuItem shortcut={`${MOD}⇧S`} disabled={onSave === null} onSelect={() => onSave?.(true)}>
              Save as…
            </MenuItem>
          )}
          {onSaveAsCell && project && (
            <MenuItem icon={<SaveAll className="size-4" />} onSelect={onSaveAsCell}>
              Save as a cell of {project.name}…
            </MenuItem>
          )}
          {exports.length > 0 && (
            <>
              <MenuSeparator />
              <MenuLabel>Export</MenuLabel>
              {exports.map((e) => (
                <MenuItem key={e.label} icon={<Download className="size-4" />} onSelect={e.run}>
                  {e.label}
                </MenuItem>
              ))}
            </>
          )}
          <MenuSeparator />
          <MenuLabel>{project?.name ?? "Cells"}</MenuLabel>
          {cells.map((e) => (
            <MenuItem
              key={e.id}
              icon={e.id === example ? <Check className="size-4" /> : undefined}
              onSelect={() => onExample(e.id)}
            >
              {e.id}
            </MenuItem>
          ))}
        </DropdownMenu>
        <ThemeToggle />
      </div>
      {/* Drops anywhere open files; the menu's “Open files…” uses the input below. */}
      <FileDrop
        onFiles={onFiles}
        accept=".json,.urdf,.glb"
        multiple
        overlay
        className="hidden"
        overlayMessage="Drop a scene.json (with its scenario.json and robot assets)"
      />
      <input
        ref={input}
        type="file"
        accept=".json,.urdf,.glb"
        multiple
        hidden
        data-testid="open-files"
        onChange={(e) => {
          const files = [...(e.target.files ?? [])];
          e.target.value = "";
          if (files.length > 0) onFiles(files);
        }}
      />
    </div>
  );
}
