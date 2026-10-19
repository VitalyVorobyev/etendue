import { Button, Dialog, Field, Input } from "@vitavision/ui";
import { useState } from "react";

/** A cell's folder name: lowercase letters, digits, `_` and `-`, not starting with `-`. */
const CELL = /^[a-z0-9_][a-z0-9_-]*$/;

/** Ask for the name of a new cell of project `project`, and save the open scene as it. */
export function CellDialog({
  open,
  onOpenChange,
  project,
  taken,
  onSave,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  project: string;
  /** Cells the project has already. */
  taken: readonly string[];
  onSave: (id: string) => Promise<boolean>;
}) {
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const error =
    name === ""
      ? null
      : !CELL.test(name)
        ? "Use lowercase letters, digits, _ and -."
        : taken.includes(name)
          ? `${project} already has a cell ${name}.`
          : null;
  const save = async () => {
    if (name === "" || error !== null) return;
    setBusy(true);
    const ok = await onSave(name);
    setBusy(false);
    if (ok) {
      setName("");
      onOpenChange(false);
    }
  };
  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title={`Save as a cell of ${project}`}
      description="The scene and its scenario go into a new folder of the project, listed in its manifest."
      footer={
        <Button onClick={() => void save()} disabled={name === "" || error !== null} loading={busy}>
          Save
        </Button>
      }
    >
      <Field label="Cell" error={error ?? undefined}>
        <Input
          id="cell-name"
          value={name}
          placeholder="ring3_bench"
          className="font-mono"
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void save();
          }}
        />
      </Field>
    </Dialog>
  );
}
