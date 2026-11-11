/**
 * Which procedure a calibration runs (P16): the choice by the dataset's shape, or a procedure
 * from the library (the project's, the bank's or a built-in), as it is or edited. An edited
 * procedure runs by value and can be kept in the library.
 */

import { type LibraryItem, type ProcedurePart, validateLibraryItem } from "@etendue/wasm";
import { Badge, Button, Field, Input, Select } from "@vitavision/ui";
import { Pencil, Save } from "lucide-react";
import { useState } from "react";

import { idProblem } from "../io/project";
import { ProcedureEditor } from "./ProcedureEditor";
import { addStage, problemForDataset, procedureIssues } from "./procedure";

/** The choice: a library reference (`null` for the choice by shape) and, once edited, the draft. */
export interface ProcedureState {
  ref: string | null;
  draft: ProcedurePart | null;
}

export const AUTO_PROCEDURE: ProcedureState = { ref: null, draft: null };

/** The procedure a run takes by value (`RunProcedure`), or `null` for the choice by shape. */
export function runProcedure(
  state: ProcedureState,
  procedures: ReadonlyMap<string, LibraryItem>,
): { id: string; procedure: ProcedurePart } | null {
  const item = state.ref ? procedures.get(state.ref) : undefined;
  if (state.draft) return { id: item ? `${item.id}_edited` : "edited", procedure: state.draft };
  return item?.procedure ? { id: item.id, procedure: item.procedure } : null;
}

/** Why the chosen procedure cannot run, if it cannot. */
export function procedureProblem(state: ProcedureState): string | null {
  if (!state.draft) return null;
  const issues = procedureIssues(state.draft, validateLibraryItem);
  return issues.length > 0 ? "The edited procedure does not check" : null;
}

export function ProcedureChoice({
  state,
  onChange,
  procedures,
  cameras,
  lasers,
  tilted,
  save,
  disabled,
}: {
  state: ProcedureState;
  onChange: (state: ProcedureState) => void;
  /** Every procedure the project sees, by reference. */
  procedures: ReadonlyMap<string, LibraryItem>;
  cameras: readonly string[];
  lasers: boolean;
  /** Whether the scene's cameras tilt. */
  tilted: boolean;
  /** Where an edited procedure can be kept, and how; `null` where nothing is writable. */
  save: { where: string; put: (item: LibraryItem) => Promise<boolean> } | null;
  disabled: boolean;
}) {
  const item = state.ref ? procedures.get(state.ref) : undefined;
  const [saveId, setSaveId] = useState("");
  const [saveName, setSaveName] = useState("");
  const startEditing = () =>
    onChange({
      ...state,
      draft: structuredClone(item?.procedure ?? addStage({ stages: [] }, problemForDataset(cameras.length, lasers))),
    });
  const idIssue = idProblem(saveId, "procedures", procedures);
  const issues = state.draft ? procedureIssues(state.draft, validateLibraryItem) : [];

  return (
    <div className="flex flex-col gap-3" data-testid="procedure-choice">
      <div className="flex flex-wrap items-end gap-2">
        <Field label="Procedure">
          <Select
            aria-label="Procedure"
            value={state.ref ?? "__auto"}
            options={[
              { value: "__auto", label: "By the dataset's shape" },
              ...[...procedures].map(([ref, p]) => ({ value: ref, label: p.name })),
            ]}
            onValueChange={(v) => onChange({ ref: v === "__auto" ? null : v, draft: null })}
            disabled={disabled}
          />
        </Field>
        {state.draft ? (
          <>
            <Badge tone="warning">edited</Badge>
            <Button size="sm" variant="ghost" onClick={() => onChange({ ...state, draft: null })} disabled={disabled}>
              Discard edits
            </Button>
          </>
        ) : (
          <Button
            size="sm"
            variant="ghost"
            icon={<Pencil className="size-3.5" />}
            onClick={startEditing}
            disabled={disabled}
          >
            Edit stages
          </Button>
        )}
      </div>
      {item?.description && !state.draft && <p className="text-xs text-fg-muted">{item.description}</p>}
      {state.draft && (
        <>
          <ProcedureEditor
            value={state.draft}
            onChange={(draft) => onChange({ ...state, draft })}
            cameras={cameras}
            tilted={tilted}
            disabled={disabled}
          />
          {save && (
            <div className="flex flex-wrap items-end gap-2" role="group" aria-label="Keep the procedure">
              <Field label="Id" error={saveId === "" ? undefined : (idIssue ?? undefined)}>
                <Input
                  aria-label="Procedure id"
                  className="w-40 font-mono"
                  placeholder={item ? `${item.id}_edited` : "my_procedure"}
                  value={saveId}
                  onChange={(e) => setSaveId(e.target.value)}
                />
              </Field>
              <Field label="Name">
                <Input
                  aria-label="Procedure name"
                  className="w-56"
                  placeholder={item ? `${item.name}, edited` : "My procedure"}
                  value={saveName}
                  onChange={(e) => setSaveName(e.target.value)}
                />
              </Field>
              <Button
                size="sm"
                variant="secondary"
                icon={<Save className="size-3.5" />}
                disabled={disabled || idIssue !== null || issues.length > 0}
                onClick={() => {
                  const draft = state.draft;
                  if (!draft) return;
                  const saved: LibraryItem = {
                    version: 2,
                    id: saveId,
                    name: saveName.trim() || saveId,
                    ...(item ? { description: `Edited from ${item.name}.` } : {}),
                    procedure: draft,
                  };
                  void save.put(saved).then((ok) => {
                    if (ok) onChange({ ref: `procedures/${saveId}`, draft: null });
                  });
                }}
              >
                Save to {save.where}
              </Button>
            </div>
          )}
        </>
      )}
    </div>
  );
}
