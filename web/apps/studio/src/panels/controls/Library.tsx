/** Controls shared by the design workspaces (Optics, Rigs): library pickers, saving, readouts. */

import { Button, Select } from "@vitavision/ui";
import { Save } from "lucide-react";
import { type ReactNode, useState } from "react";

import { type LibraryKind, type Project, itemsOf } from "../../io/project";
import { RowLabel } from "./Fold";

/**
 * A library picker: the items of `kind` (or of several kinds), by name. The design's own part
 * (`current`) is listed first when it is not a library item by that name.
 */
export function Pick({
  label,
  project,
  kind,
  current,
  onPick,
  extra = [],
}: {
  label: string;
  project: Project | null;
  kind: LibraryKind | LibraryKind[];
  current: string;
  onPick: (ref: string) => void;
  /** Choices that are not library items (`value` is passed to `onPick`). */
  extra?: { value: string; label: string }[];
}) {
  const kinds = Array.isArray(kind) ? kind : [kind];
  const items = project ? kinds.flatMap((k) => itemsOf(project, k)) : [];
  const CURRENT = "__current";
  const match = items.find(([, i]) => i.name === current)?.[0];
  return (
    <>
      <RowLabel>{label}</RowLabel>
      <Select
        aria-label={label}
        className="min-w-0 [&>span]:truncate [&>span]:whitespace-nowrap"
        value={match ?? CURRENT}
        options={[
          ...(match ? [] : [{ value: CURRENT, label: current }]),
          ...extra,
          ...items.map(([ref, i]) => ({ value: ref, label: i.name })),
        ]}
        onValueChange={(ref) => ref !== "" && ref !== CURRENT && onPick(ref)}
      />
    </>
  );
}

/** Save to the project's library; a note instead when the project is read-only (`onSave` null). */
export function SaveButton({ onSave, disabled }: { onSave: (() => Promise<boolean>) | null; disabled: boolean }) {
  const [busy, setBusy] = useState(false);
  if (!onSave) {
    return (
      <p className="text-xs text-fg-muted">
        The examples are read-only: open or create a project (File menu) to save designs.
      </p>
    );
  }
  return (
    <Button
      size="sm"
      icon={<Save className="size-3.5" />}
      loading={busy}
      disabled={disabled}
      onClick={() => {
        setBusy(true);
        void onSave().finally(() => setBusy(false));
      }}
    >
      Save to library
    </Button>
  );
}

/** A readout: a label and a right-aligned value, in a two-column grid. */
export function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <RowLabel title={label}>{label}</RowLabel>
      <span className="text-right font-mono text-xs tabular-nums">{children}</span>
    </>
  );
}
