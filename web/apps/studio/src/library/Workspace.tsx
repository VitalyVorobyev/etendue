/**
 * The Library workspace: every library item the open project sees — its own, the user's bank's,
 * the built-ins — to browse, edit, create, duplicate, rename, delete, import and export, and to
 * move between the layers (use a bank item in the project, save a project item to the bank,
 * update a copy whose source changed).
 */

import { type LibraryItem, isEtendueError, validateLibraryItem } from "@etendue/wasm";
import {
  Badge,
  Button,
  Callout,
  ConfirmDialog,
  Dialog,
  DropdownMenu,
  Empty,
  Field,
  Input,
  MenuItem,
  SegmentedControl,
  Select,
  Textarea,
  cn,
} from "@vitavision/ui";
import {
  ArrowDownToLine,
  ArrowUpFromLine,
  Copy,
  Download,
  FileInput,
  FolderInput,
  PencilLine,
  Plus,
  RefreshCw,
  Trash2,
} from "lucide-react";
import { type ReactNode, useMemo, useRef, useState } from "react";

import { exportFile, jsonText } from "../io/export";
import { canOpenFolders } from "../io/folder";
import { LIBRARY_FOLDERS, type Layer, type LibraryKind, idProblem } from "../io/project";
import { isTauri } from "../io/tauri";
import type { Store, Studio, StudioState } from "../state/studio";
import {
  type Entry,
  type Filter,
  KIND_LABEL,
  KINDS,
  LAYER_LABEL,
  NO_FILTER,
  byKind,
  deleteBlocker,
  entries,
  filterEntries,
  freeId,
  idFrom,
  reselect,
} from "./browse";
import { GeneralForm, LaserForm, LensForm, MountForm, SensorForm, SourceForm } from "./PartForms";
import { ProcedureEditor } from "../calibrate/ProcedureEditor";
import { newPart } from "./templates";

/** The kinds edited as JSON (their parts are nested documents). */
const JSON_KINDS: readonly LibraryKind[] = ["target", "camera", "head", "rig"];

type DialogState =
  | { mode: "new" }
  | { mode: "duplicate"; entry: Entry }
  | { mode: "rename"; entry: Entry }
  | { mode: "delete"; entry: Entry }
  | null;

/** Whether the bank and the project can be written, and why not. */
interface Writable {
  bank: string | null;
  project: string | null;
}

function writability(state: StudioState): Writable {
  const { bank, project } = state;
  const bankReason = bank
    ? bank.write
      ? null
      : `The bank (${bank.location}) is read-only.`
    : isTauri()
      ? "The bank did not open."
      : canOpenFolders()
        ? "Choose a bank folder first: File › Choose bank folder…"
        : "This browser cannot open folders: use the desktop app, or Chrome or Edge, for a bank.";
  const projectReason = !project
    ? "No project is open."
    : !project.write || (state.builtin !== null && project.own === state.builtin)
      ? "The examples are read-only: open or create a project (File menu) to add to it."
      : null;
  return { bank: bankReason, project: projectReason };
}

/** The Library workspace's panels: the list (left), the item (main) and the layers (right). */
export function useLibraryWorkspace(
  studio: Studio,
  state: StudioState,
): { left: ReactNode; main: ReactNode; right: ReactNode } {
  const { project, bank, builtin } = state;
  const all = useMemo(() => entries(project, bank, builtin), [project, bank, builtin]);
  const [filter, setFilter] = useState<Filter>(NO_FILTER);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [dialog, setDialog] = useState<DialogState>(null);
  const [importInto, setImportInto] = useState<Store>("bank");
  const input = useRef<HTMLInputElement>(null);
  const selected = reselect(all, selectedKey);
  // The open item's draft lives here, so it outlives its view (while another workspace shows).
  const [held, setHeld] = useState<{ key: string; draft: Draft } | null>(null);
  const draft = selected && held?.key === selected.key && held.draft.base === selected.item ? held.draft : null;
  // An item asked for while the open one has unsaved changes: the user is asked first.
  const [leaving, setLeaving] = useState<string | null>(null);
  const select = (key: string) => {
    if (selected && key !== selected.key && draft && evaluate(selected, draft).dirty) {
      setLeaving(key);
      return;
    }
    setHeld(null);
    setSelectedKey(key);
  };
  const writable = writability(state);
  const shown = filterEntries(all, filter);

  const startImport = (store: Store) => {
    setImportInto(store);
    input.current?.click();
  };

  const left = (
    <div className="flex h-full min-h-0 flex-col" data-testid="library-browser">
      <div className="space-y-2 border-b border-line p-2">
        <Input
          type="search"
          aria-label="Filter the library"
          placeholder="Filter by name, id, vendor…"
          value={filter.text}
          onChange={(e) => setFilter({ ...filter, text: e.target.value })}
        />
        <div className="grid grid-cols-2 gap-2">
          <Select
            aria-label="Kind"
            value={filter.kind}
            options={[
              { value: "all", label: "Every kind" },
              ...KINDS.map((k) => ({ value: k, label: KIND_LABEL[k][1] })),
            ]}
            onValueChange={(v) => setFilter({ ...filter, kind: (v || "all") as Filter["kind"] })}
          />
          <Select
            aria-label="Layer"
            value={filter.layer}
            options={[
              { value: "all", label: "Every layer" },
              { value: "project", label: "Project" },
              { value: "bank", label: "Bank" },
              { value: "builtin", label: "Built-in" },
            ]}
            onValueChange={(v) => setFilter({ ...filter, layer: (v || "all") as Filter["layer"] })}
          />
        </div>
        <div className="flex gap-2">
          <Button
            size="sm"
            icon={<Plus className="size-3.5" />}
            disabled={writable.bank !== null && writable.project !== null}
            title={writable.bank !== null && writable.project !== null ? writable.bank : undefined}
            onClick={() => setDialog({ mode: "new" })}
          >
            New…
          </Button>
          <DropdownMenu
            trigger={
              <Button size="sm" variant="ghost" icon={<FileInput className="size-3.5" />}>
                Import
              </Button>
            }
          >
            <MenuItem disabled={writable.bank !== null} onSelect={() => startImport("bank")}>
              Into the bank…
            </MenuItem>
            <MenuItem disabled={writable.project !== null} onSelect={() => startImport("project")}>
              Into the project…
            </MenuItem>
          </DropdownMenu>
          <input
            ref={input}
            type="file"
            accept=".json,application/json"
            multiple
            hidden
            data-testid="library-import"
            onChange={(e) => {
              const files = [...(e.target.files ?? [])];
              e.target.value = "";
              void Promise.all(files.map(async (f) => ({ name: f.name, text: await f.text() }))).then((docs) =>
                studio.importItems(importInto, docs),
              );
            }}
          />
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-auto" data-testid="library-list">
        {shown.length === 0 ? (
          <p className="p-3 text-xs text-fg-muted">
            {all.length === 0 ? "Nothing in the library yet." : "No item matches the filter."}
          </p>
        ) : (
          byKind(shown).map(([kind, items]) => (
            <section key={kind} aria-label={KIND_LABEL[kind][1]}>
              <h3 className="sticky top-0 border-b border-line bg-surface px-3 py-1 text-[11px] font-medium text-fg-muted">
                {KIND_LABEL[kind][1]} <span className="font-mono">{items.length}</span>
              </h3>
              {items.map((e) => (
                <Row key={e.key} entry={e} selected={e.key === selected?.key} onSelect={() => select(e.key)} />
              ))}
            </section>
          ))
        )}
      </div>
    </div>
  );

  const main = selected ? (
    <Detail
      key={selected.key}
      entry={selected}
      all={all}
      state={state}
      studio={studio}
      writable={writable}
      draft={draft}
      onDraft={(d) => setHeld({ key: selected.key, draft: d })}
      onSelect={select}
      onDialog={setDialog}
    />
  ) : (
    <div className="grid h-full place-items-center p-6">
      <Empty>{state.loading ? "Loading…" : "Choose an item, or make a new one."}</Empty>
    </div>
  );

  const right = <Layers state={state} studio={studio} writable={writable} />;

  const dialogs = (
    <>
      {dialog && dialog.mode !== "delete" && (
        <ItemDialog
          key={`${dialog.mode}:${"entry" in dialog ? dialog.entry.key : ""}`}
          dialog={dialog}
          state={state}
          writable={writable}
          kind={filter.kind === "all" ? "lens" : filter.kind}
          onClose={() => setDialog(null)}
          onDone={(key) => {
            setDialog(null);
            select(key);
          }}
          studio={studio}
        />
      )}
      <ConfirmDialog
        open={leaving !== null}
        onOpenChange={(open) => !open && setLeaving(null)}
        title="Discard unsaved changes?"
        description={
          selected ? `${selected.item.name} has changes that are not saved. Opening another item discards them.` : ""
        }
        confirmLabel="Discard"
        destructive
        onConfirm={() => {
          const key = leaving;
          setLeaving(null);
          setHeld(null);
          if (key !== null) setSelectedKey(key);
        }}
      />
      <ConfirmDialog
        open={dialog?.mode === "delete"}
        onOpenChange={(open) => !open && setDialog(null)}
        title="Delete the item?"
        description={
          dialog?.mode === "delete"
            ? `${dialog.entry.item.name} (${dialog.entry.ref}) is removed from the ${LAYER_LABEL[dialog.entry.layer]}. Projects that copied it keep their copies.`
            : ""
        }
        confirmLabel="Delete"
        destructive
        onConfirm={() => {
          if (dialog?.mode !== "delete") return;
          const { entry } = dialog;
          setDialog(null);
          if (entry.layer !== "builtin") void studio.deleteItem(entry.layer, entry.ref);
        }}
      />
    </>
  );

  return {
    left,
    main: (
      <>
        {main}
        {dialogs}
      </>
    ),
    right,
  };
}

function Row({ entry: e, selected, onSelect }: { entry: Entry; selected: boolean; onSelect: () => void }) {
  return (
    <button
      type="button"
      onClick={onSelect}
      aria-current={selected ? "true" : undefined}
      data-testid="library-row"
      data-layer={e.layer}
      data-ref={e.ref}
      className={cn(
        "flex w-full flex-col gap-0.5 border-b border-line px-3 py-1.5 text-left hover:bg-raised focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-signal",
        selected && "bg-raised",
        e.hiddenBy !== null && "opacity-60",
      )}
    >
      <span className="flex items-center gap-1.5">
        <span className="min-w-0 flex-1 truncate text-xs text-fg">{e.item.name}</span>
        {e.stale && <Badge tone="warning">stale</Badge>}
        <LayerBadge layer={e.layer} />
      </span>
      <span className="flex min-w-0 gap-2 font-mono text-[11px] text-fg-muted">
        <span className="truncate">{e.item.id}</span>
        {e.hides.length > 0 && <span className="shrink-0">hides {e.hides.map((l) => LAYER_LABEL[l]).join(", ")}</span>}
        {e.hiddenBy !== null && <span className="shrink-0">hidden by {LAYER_LABEL[e.hiddenBy]}</span>}
      </span>
    </button>
  );
}

function LayerBadge({ layer }: { layer: Layer }) {
  return (
    <Badge tone={layer === "project" ? "info" : layer === "bank" ? "normal" : "neutral"}>{LAYER_LABEL[layer]}</Badge>
  );
}

/** The kernel's verdict on `item`, as lines; empty when it checks. */
function problemsOf(item: LibraryItem): string[] {
  try {
    validateLibraryItem(JSON.stringify(item));
    return [];
  } catch (e) {
    if (isEtendueError(e) && e.issues && e.issues.length > 0) {
      return e.issues.map((i) => (i.path ? `${i.path}: ${i.message}` : i.message));
    }
    return [e instanceof Error ? e.message : String(e)];
  }
}

/** The item as a draft: the whole item, and for the JSON kinds the part's text as typed. */
interface Draft {
  base: LibraryItem;
  item: LibraryItem;
  text: string;
}

const partText = (item: LibraryItem, kind: LibraryKind) => jsonText(item[kind]);

/** A draft that starts from `item`, unchanged. */
const fresh = (item: LibraryItem, kind: LibraryKind): Draft => ({ base: item, item, text: partText(item, kind) });

/** What `draft` of `entry` would save, why it cannot (JSON that does not parse), and whether it differs. */
function evaluate(entry: Entry, draft: Draft): { toSave: LibraryItem; parseError: string | null; dirty: boolean } {
  let parseError: string | null = null;
  let edited = draft.item;
  if (JSON_KINDS.includes(entry.kind)) {
    try {
      edited = { ...draft.item, [entry.kind]: JSON.parse(draft.text) as unknown };
    } catch (e) {
      parseError = e instanceof Error ? e.message : String(e);
    }
  }
  // Saved as version 2 (`source` and `mount` need it).
  const toSave: LibraryItem = { ...edited, version: 2 };
  const dirty = parseError !== null || jsonText(toSave) !== jsonText({ ...entry.item, version: 2 });
  return { toSave, parseError, dirty };
}

function Detail({
  entry,
  all,
  state,
  studio,
  writable,
  draft: held,
  onDraft: setDraft,
  onSelect,
  onDialog,
}: {
  entry: Entry;
  all: readonly Entry[];
  state: StudioState;
  studio: Studio;
  writable: Writable;
  /** The draft kept for this item, if any; `null` when it is as saved. */
  draft: Draft | null;
  onDraft: (draft: Draft) => void;
  onSelect: (key: string) => void;
  onDialog: (d: DialogState) => void;
}) {
  const { kind, layer, ref } = entry;
  // A save (here or elsewhere) changes the item: the draft starts again from it.
  const draft = held ?? fresh(entry.item, kind);
  const [busy, setBusy] = useState(false);
  const readOnly = layer === "builtin" || (layer === "bank" ? writable.bank : writable.project) !== null;
  const asJson = JSON_KINDS.includes(kind);
  const { toSave, parseError, dirty } = evaluate(entry, draft);
  const problems = parseError ? [`Not JSON: ${parseError}`] : dirty ? problemsOf(toSave) : [];
  const setItem = (item: LibraryItem) => setDraft({ ...draft, item });

  const origin = layer === "project" ? entry.item.origin : null;
  const projectOwns = state.project?.own.has(ref) ?? false;
  const blocker = layer === "project" && state.project ? deleteBlocker(state.project, ref, all) : null;
  const run = (action: () => Promise<unknown>) => {
    setBusy(true);
    void action().finally(() => setBusy(false));
  };

  const actions: ReactNode[] = [];
  if (layer !== "project") {
    actions.push(
      <Button
        key="use"
        size="sm"
        icon={<ArrowDownToLine className="size-3.5" />}
        disabled={busy || writable.project !== null || projectOwns}
        title={writable.project ?? (projectOwns ? "The project has its own item with this id." : undefined)}
        onClick={() =>
          run(async () => {
            if (await studio.useItemInProject(layer, ref, entry.item)) onSelect(`project:${ref}`);
          })
        }
      >
        Use in project
      </Button>,
    );
  } else {
    actions.push(
      <Button
        key="bank"
        size="sm"
        icon={<ArrowUpFromLine className="size-3.5" />}
        disabled={busy || writable.bank !== null}
        title={
          writable.bank ??
          (state.bank?.library.has(ref) ? "Replaces the bank's item with this id." : "Copies it into the bank.")
        }
        onClick={() => run(() => studio.saveToBank(entry.item))}
      >
        Save to bank
      </Button>,
    );
    if (entry.stale) {
      actions.push(
        <Button
          key="update"
          size="sm"
          icon={<RefreshCw className="size-3.5" />}
          disabled={busy}
          onClick={() => run(() => studio.updateCopy(ref))}
        >
          Update copy
        </Button>,
      );
    }
  }
  actions.push(
    <Button
      key="duplicate"
      size="sm"
      variant="ghost"
      icon={<Copy className="size-3.5" />}
      disabled={busy || (writable.bank !== null && writable.project !== null)}
      onClick={() => onDialog({ mode: "duplicate", entry })}
    >
      Duplicate…
    </Button>,
  );
  if (layer !== "builtin") {
    actions.push(
      <Button
        key="rename"
        size="sm"
        variant="ghost"
        icon={<PencilLine className="size-3.5" />}
        disabled={busy || readOnly}
        onClick={() => onDialog({ mode: "rename", entry })}
      >
        Rename…
      </Button>,
      <Button
        key="delete"
        size="sm"
        variant="ghost"
        icon={<Trash2 className="size-3.5" />}
        disabled={busy || readOnly || blocker !== null}
        title={blocker ?? undefined}
        onClick={() => onDialog({ mode: "delete", entry })}
      >
        Delete…
      </Button>,
    );
  }
  actions.push(
    <Button
      key="export"
      size="sm"
      variant="ghost"
      icon={<Download className="size-3.5" />}
      onClick={() =>
        void exportFile("Export a library item", {
          name: `${entry.item.id}.json`,
          text: jsonText(entry.item),
          type: "application/json",
        })
      }
    >
      Export
    </Button>,
  );

  const sourceLayer = origin ? (origin.layer === "bank" ? "bank" : "built-ins") : null;
  return (
    <div className="h-full min-h-0 overflow-auto" data-testid="library-detail">
      <div className="mx-auto max-w-3xl space-y-4 p-4">
        <header className="space-y-2">
          <div className="flex items-center gap-2">
            <h2 className="min-w-0 flex-1 truncate text-sm font-semibold text-fg">{entry.item.name}</h2>
            {entry.stale && <Badge tone="warning">stale</Badge>}
            <LayerBadge layer={layer} />
          </div>
          <p className="font-mono text-xs text-fg-muted">
            {KIND_LABEL[kind][0]} · {ref}
            {entry.hides.length > 0 && ` · hides the ${entry.hides.map((l) => LAYER_LABEL[l]).join(" and ")} item`}
          </p>
          <div className="flex flex-wrap gap-1.5">{actions}</div>
        </header>
        {layer === "builtin" && (
          <Callout tone="info" title="Built-in items are read-only">
            Duplicate it into the bank or the project to change it.
          </Callout>
        )}
        {entry.hiddenBy !== null && (
          <Callout tone="info" title={`Hidden by the ${LAYER_LABEL[entry.hiddenBy]} item`}>
            The {LAYER_LABEL[entry.hiddenBy]} has an item with the same id; the project uses that one.
          </Callout>
        )}
        {layer !== "builtin" && readOnly && (
          <p className="text-xs text-fg-muted">{layer === "bank" ? writable.bank : writable.project}</p>
        )}
        {origin &&
          (entry.stale ? (
            <Callout
              tone="warning"
              title="Its source has changed"
              actions={
                <Button size="sm" disabled={busy} onClick={() => run(() => studio.updateCopy(ref))}>
                  Update copy
                </Button>
              }
            >
              This is a copy of <span className="font-mono">{origin.reference}</span> from the {sourceLayer}, which has
              changed since it was copied.
            </Callout>
          ) : (
            <p className="text-xs text-fg-muted" data-testid="library-origin">
              A copy of <span className="font-mono">{origin.reference}</span> from the {sourceLayer}.
            </p>
          ))}
        <section className="space-y-3" aria-label="General">
          <GeneralForm item={draft.item} onChange={setItem} disabled={readOnly} />
        </section>
        <section className="space-y-2" aria-label={KIND_LABEL[kind][0]}>
          <h3 className="text-xs font-medium text-fg">{KIND_LABEL[kind][0]}</h3>
          {asJson ? (
            <Field label="JSON" description="Checked as you type; it saves once it checks.">
              <Textarea
                mono
                minRows={10}
                maxRows={40}
                spellCheck={false}
                value={draft.text}
                readOnly={readOnly}
                invalid={problems.length > 0}
                onValueChange={(text) => setDraft({ ...draft, text })}
              />
            </Field>
          ) : (
            <PartForm item={draft.item} kind={kind} onChange={setItem} disabled={readOnly} />
          )}
        </section>
        <section className="space-y-2" aria-label="Source">
          <h3 className="text-xs font-medium text-fg">Source</h3>
          <SourceForm item={draft.item} onChange={setItem} disabled={readOnly} />
        </section>
        {problems.length > 0 && (
          <Callout tone="error" title="The item does not check">
            <ul className="list-disc space-y-0.5 pl-4 font-mono text-xs" data-testid="library-problems">
              {problems.map((p) => (
                <li key={p}>{p}</li>
              ))}
            </ul>
          </Callout>
        )}
        {!readOnly && (
          <div className="flex items-center gap-2 border-t border-line pt-3">
            <Button
              size="sm"
              variant="primary"
              disabled={!dirty || problems.length > 0 || busy}
              onClick={() =>
                run(async () => {
                  await studio.putItem(layer, toSave);
                })
              }
            >
              Save
            </Button>
            <Button
              size="sm"
              variant="ghost"
              disabled={!dirty || busy}
              onClick={() => setDraft(fresh(entry.item, kind))}
            >
              Revert
            </Button>
            <span className="text-xs text-fg-muted">Saves to the {LAYER_LABEL[layer]}.</span>
          </div>
        )}
      </div>
    </div>
  );
}

function PartForm({
  item,
  kind,
  onChange,
  disabled,
}: {
  item: LibraryItem;
  kind: LibraryKind;
  onChange: (i: LibraryItem) => void;
  disabled: boolean;
}) {
  if (kind === "sensor" && item.sensor)
    return <SensorForm part={item.sensor} onChange={(sensor) => onChange({ ...item, sensor })} disabled={disabled} />;
  if (kind === "lens" && item.lens)
    return <LensForm part={item.lens} onChange={(lens) => onChange({ ...item, lens })} disabled={disabled} />;
  if (kind === "laser" && item.laser)
    return <LaserForm part={item.laser} onChange={(laser) => onChange({ ...item, laser })} disabled={disabled} />;
  if (kind === "mount" && item.mount)
    return <MountForm part={item.mount} onChange={(mount) => onChange({ ...item, mount })} disabled={disabled} />;
  if (kind === "procedure" && item.procedure)
    return (
      <ProcedureEditor
        value={item.procedure}
        onChange={(procedure) => onChange({ ...item, procedure })}
        disabled={disabled}
        showIssues={false}
      />
    );
  return null;
}

/** New, Duplicate and Rename: an id (and a name, and where to put it). */
function ItemDialog({
  dialog,
  state,
  writable,
  kind: startKind,
  onClose,
  onDone,
  studio,
}: {
  dialog: Exclude<DialogState, null | { mode: "delete" }>;
  state: StudioState;
  writable: Writable;
  kind: LibraryKind;
  onClose: () => void;
  onDone: (key: string) => void;
  studio: Studio;
}) {
  const entry = dialog.mode === "new" ? null : dialog.entry;
  const renaming = dialog.mode === "rename";
  const preferred: Store =
    entry && entry.layer !== "builtin" && (entry.layer === "bank" ? writable.bank : writable.project) === null
      ? entry.layer
      : writable.bank === null
        ? "bank"
        : "project";
  const [store, setStore] = useState<Store>(preferred);
  const [kind, setKind] = useState<LibraryKind>(entry?.kind ?? startKind);
  const [name, setName] = useState(entry ? (renaming ? entry.item.name : `${entry.item.name} (copy)`) : "");
  const taken = (store === "bank" ? state.bank?.library : state.project?.own) ?? new Map<string, LibraryItem>();
  const folder = LIBRARY_FOLDERS[kind];
  const [typedId, setTypedId] = useState<string | null>(
    entry && !renaming ? freeId(entry.item.id, folder, taken) : null,
  );
  const id = typedId ?? (renaming ? (entry?.item.id ?? "") : idFrom(name));
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const problem = renaming && id === entry?.item.id ? "Give it a new id." : idProblem(id, folder, taken);
  const nameProblem = !renaming && name.trim() === "" ? "A name is needed." : null;
  const reason = store === "bank" ? writable.bank : writable.project;

  const submit = async () => {
    setBusy(true);
    setFailure(null);
    try {
      let ok: boolean;
      if (renaming && entry && entry.layer !== "builtin") {
        ok = await studio.renameItem(entry.layer, entry.ref, id);
      } else {
        const base: LibraryItem = entry
          ? { ...entry.item, id, name, version: 2 }
          : { version: 2, id, name, ...newPart(kind, state.project) };
        // A duplicate is a new item, not a copy of its source.
        delete base.origin;
        ok = await studio.putItem(store, base);
      }
      if (ok) onDone(`${renaming && entry ? entry.layer : store}:${folder}/${id}`);
    } catch (e) {
      setFailure(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const title = dialog.mode === "new" ? "New library item" : renaming ? "Rename the item" : "Duplicate the item";
  return (
    <Dialog
      open
      onOpenChange={(open) => !open && onClose()}
      title={title}
      description={
        renaming
          ? "The item is saved under its new id and the old file is removed."
          : dialog.mode === "duplicate"
            ? `A new item from ${entry?.item.name ?? ""}.`
            : "It starts from typical values; edit it once it is made."
      }
      footer={
        <>
          <Button size="sm" variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button
            size="sm"
            variant="primary"
            loading={busy}
            disabled={problem !== null || nameProblem !== null || reason !== null}
            onClick={() => void submit()}
          >
            {dialog.mode === "new" ? "Create" : renaming ? "Rename" : "Duplicate"}
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        {dialog.mode === "new" && (
          <Field label="Kind">
            <Select
              value={kind}
              options={KINDS.map((k) => ({ value: k, label: KIND_LABEL[k][0] }))}
              onValueChange={(v) => KINDS.includes(v as LibraryKind) && setKind(v as LibraryKind)}
            />
          </Field>
        )}
        {!renaming && (
          <Field label="Name" error={nameProblem ?? undefined}>
            <Input value={name} onChange={(e) => setName(e.target.value)} autoFocus />
          </Field>
        )}
        <Field label="Id" error={problem ?? undefined} annotation={`${folder}/${id || "…"}`}>
          <Input value={id} className="font-mono" onChange={(e) => setTypedId(e.target.value)} />
        </Field>
        {!renaming && (
          <Field label="Into" as="group" error={reason ?? undefined}>
            <SegmentedControl
              aria-label="Into"
              value={store}
              options={[
                { value: "bank", label: "Bank" },
                { value: "project", label: "Project" },
              ]}
              onValueChange={(v) => (v === "bank" || v === "project") && setStore(v)}
            />
          </Field>
        )}
        {failure && (
          <Callout tone="error" title="It did not work">
            {failure}
          </Callout>
        )}
      </div>
    </Dialog>
  );
}

/** The three layers: where each is, how many items it holds, what did not read. */
function Layers({ state, studio, writable }: { state: StudioState; studio: Studio; writable: Writable }) {
  const { project, bank, builtin } = state;
  const isBuiltin = project !== null && builtin !== null && project.own === builtin;
  const problems = [
    ...(project?.problems.filter((p) => p.path.includes("library/")) ?? []).map((p) => ({ ...p, layer: "project" })),
    ...(bank?.problems ?? []).map((p) => ({ ...p, layer: "bank" })),
  ];
  return (
    <div className="space-y-4 p-3 text-xs" data-testid="library-layers">
      <LayerCard
        title="Project"
        name={isBuiltin ? null : (project?.manifest.name ?? null)}
        location={isBuiltin ? null : (project?.location ?? null)}
        count={isBuiltin ? null : (project?.own.size ?? null)}
        note={writable.project}
      />
      <LayerCard
        title="Bank"
        name={bank?.manifest.name ?? null}
        location={bank?.location ?? null}
        count={bank?.library.size ?? null}
        note={writable.bank}
        action={
          !isTauri() && canOpenFolders() ? (
            <Button
              size="sm"
              variant="ghost"
              icon={<FolderInput className="size-3.5" />}
              onClick={() => void studio.chooseBank()}
            >
              {bank ? "Choose another…" : "Choose bank folder…"}
            </Button>
          ) : null
        }
      />
      <LayerCard
        title="Built-in"
        name="Shipped with the studio"
        location={null}
        count={builtin?.size ?? null}
        note="Read-only."
      />
      <p className="text-fg-muted">
        A project sees its own items first, then the bank&apos;s, then the built-ins. Using an item in a project copies
        it there, so the project keeps working without the bank.
      </p>
      {problems.length > 0 && (
        <Callout tone="warning" title="Files that did not read">
          <ul className="space-y-1 font-mono">
            {problems.map((p) => (
              <li key={`${p.layer}:${p.path}`}>
                {p.layer}: {p.path}: {p.message}
              </li>
            ))}
          </ul>
        </Callout>
      )}
    </div>
  );
}

function LayerCard({
  title,
  name,
  location,
  count,
  note,
  action,
}: {
  title: string;
  name: string | null;
  location: string | null;
  count: number | null;
  note: string | null;
  action?: ReactNode;
}) {
  return (
    <section className="space-y-1 rounded border border-line p-2" aria-label={title}>
      <div className="flex items-center gap-2">
        <h3 className="flex-1 font-medium text-fg">{title}</h3>
        {count !== null && <span className="font-mono text-fg-muted">{count === 1 ? "1 item" : `${count} items`}</span>}
      </div>
      {name && <p className="text-fg">{name}</p>}
      {location && (
        <p className="truncate font-mono text-[11px] text-fg-muted" title={location}>
          {location}
        </p>
      )}
      {note && <p className="text-fg-muted">{note}</p>}
      {action}
    </section>
  );
}
