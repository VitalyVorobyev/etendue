/*
 * Notifications, as a store any module can write to.
 *
 * A toast is raised from wherever the news is — a file loader, a worker's error handler, a
 * bake that finished — which is rarely a component with a context in reach. So `toast()` is
 * a plain function over a module-level store, and `<Toaster />`, mounted once, is its only
 * reader. The store holds no timers and touches no DOM: auto-dismissal belongs to the
 * `Toaster`, which is what knows whether a reader is hovering it.
 */

/** A toast's kind: a fact (`info`), a completed action (`success`), a caveat (`warn`), a failure (`error`). */
export type ToastTone = "info" | "success" | "warn" | "error";

/** What `toast()` takes. */
export interface ToastOptions {
  /** One line: what happened. */
  title: string;
  /** More detail, and what to do about it. */
  description?: string | undefined;
  /** Defaults to `info`. */
  tone?: ToastTone | undefined;
  /**
   * How long it stays, in milliseconds, while not hovered or focused. Defaults to 5000
   * (8000 for `error`); `Infinity` keeps it until dismissed.
   */
  duration?: number | undefined;
  /** Replace the toast with this id instead of adding one — for progress that updates in place. */
  id?: string | undefined;
}

/** A toast in the store. */
export interface ToastRecord {
  /** Identifies the toast to `dismiss` and to `id` replacement. */
  id: string;
  /** One line: what happened. */
  title: string;
  /** More detail, if any. */
  description: string | undefined;
  /** Its kind. */
  tone: ToastTone;
  /** Milliseconds before it dismisses itself (`Infinity`: never). */
  duration: number;
}

/**
 * A toast store: what `toast()` writes to and `<Toaster />` reads. Every member may be called
 * detached.
 */
export interface ToastStore {
  /** Add a toast (or replace the one with `options.id`), returning its id. */
  readonly toast: (options: ToastOptions) => string;
  /** Remove one toast, or every toast when `id` is omitted. */
  readonly dismiss: (id?: string) => void;
  /** The toasts, oldest first. The same array until the store changes. */
  readonly getSnapshot: () => readonly ToastRecord[];
  /** Be told when the toasts change; returns the unsubscribe function. */
  readonly subscribe: (listener: () => void) => () => void;
}

const DEFAULT_DURATION = 5000;
const ERROR_DURATION = 8000;

/**
 * A new, empty toast store. The package keeps one (behind `toast()` and the default
 * `<Toaster />`); create another only to isolate a region or a test.
 *
 * @returns The store.
 */
export function createToastStore(): ToastStore {
  let toasts: readonly ToastRecord[] = [];
  let sequence = 0;
  const listeners = new Set<() => void>();
  const commit = (next: readonly ToastRecord[]) => {
    toasts = next;
    for (const listener of [...listeners]) listener();
  };

  return {
    toast: (options) => {
      const tone = options.tone ?? "info";
      sequence += 1;
      const record: ToastRecord = {
        id: options.id ?? `toast-${sequence}`,
        title: options.title,
        description: options.description,
        tone,
        duration: options.duration ?? (tone === "error" ? ERROR_DURATION : DEFAULT_DURATION),
      };
      const existing = toasts.findIndex((t) => t.id === record.id);
      commit(existing === -1 ? [...toasts, record] : toasts.map((t, i) => (i === existing ? record : t)));
      return record.id;
    },
    dismiss: (id) => {
      if (id === undefined) {
        if (toasts.length > 0) commit([]);
        return;
      }
      if (toasts.some((t) => t.id === id)) commit(toasts.filter((t) => t.id !== id));
    },
    getSnapshot: () => toasts,
    subscribe: (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}

/** The store behind `toast()` and a `<Toaster />` given no `store`. */
export const defaultToastStore: ToastStore = createToastStore();

/**
 * Raise a notification, from anywhere — shown by the mounted `<Toaster />`.
 *
 * `toast({ title: "Scenario loaded", tone: "success" })`. Pass `id` to update a toast in
 * place (a progress message that becomes a result). `toast.dismiss(id)` removes one early,
 * `toast.dismiss()` all of them.
 *
 * @param options - The message, its tone and its duration.
 * @returns The toast's id.
 */
export const toast: ((options: ToastOptions) => string) & {
  /** Remove one toast, or every toast when `id` is omitted. */
  dismiss: (id?: string) => void;
} = Object.assign((options: ToastOptions) => defaultToastStore.toast(options), {
  dismiss: (id?: string) => defaultToastStore.dismiss(id),
});
