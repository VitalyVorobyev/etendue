import { Select, ThemeToggle } from "@vitavision/ui";
import { FileDrop } from "@vitavision/workbench";
import { Aperture } from "lucide-react";
import examples from "virtual:etendue-examples";

/** The top bar: product name, what is loaded, the example picker, open files, theme. */
export function Header({
  label,
  description,
  example,
  onExample,
  onFiles,
}: {
  label: string | null;
  description: string | null;
  example: string;
  onExample: (id: string) => void;
  onFiles: (files: File[]) => void;
}) {
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
