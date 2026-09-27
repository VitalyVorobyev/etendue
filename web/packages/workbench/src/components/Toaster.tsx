/*
 * Where `toast()` shows up.
 *
 * Toasts are for news that does not need an answer — a file loaded, a bake finished, a
 * worker failed in the background. Anything that must be acknowledged is a `Dialog`, and
 * anything tied to one control is that control's `Field` error. So a toast never carries an
 * action beyond dismissing it, it goes away by itself, and it holds still while someone is
 * reading it (pointer over the stack, or focus inside it).
 *
 * The tones reuse the `Callout` vocabulary — the accent for a fact, the verdict colours for
 * success, caveat and failure — on an opaque `overlay` surface, since a toast floats over
 * whatever the viewport is drawing.
 */

import { CircleCheck, CircleX, Info, TriangleAlert, X } from "lucide-react";
import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import type { FocusEvent } from "react";

import { cn, focusRing } from "@vitavision/ui";

import { defaultToastStore, type ToastRecord, type ToastStore, type ToastTone } from "./toastStore";

const TONE: Record<ToastTone, { border: string; icon: string; Icon: typeof Info }> = {
  info: { border: "border-signal/30", icon: "text-signal", Icon: Info },
  success: { border: "border-normal/30", icon: "text-normal", Icon: CircleCheck },
  warn: { border: "border-warn/30", icon: "text-warn", Icon: TriangleAlert },
  error: { border: "border-defect/30", icon: "text-defect", Icon: CircleX },
};

const NO_TOASTS: readonly ToastRecord[] = [];

/** Props of `Toaster`. */
export interface ToasterProps {
  /** The store to show. Defaults to the one behind `toast()`. */
  store?: ToastStore | undefined;
  /** How many toasts are shown at once; older ones wait. Defaults to 5. */
  limit?: number | undefined;
  /** Merged with the stack's own classes through `cn` (it is fixed to the bottom-right corner). */
  className?: string | undefined;
}

/**
 * The stack of notifications raised with `toast()`. Mount it once, near the root.
 *
 * Fixed to the bottom-right corner, newest at the bottom. Each toast dismisses itself after
 * its `duration`; the countdown pauses while the pointer is over the stack or focus is inside
 * it. A `section` landmark ("Notifications") holding a polite live region, so new toasts are
 * announced; an `error` toast is a `role="alert"`, announced at once. Each toast carries
 * `data-tone` and a "Dismiss" button.
 */
export function Toaster({ store = defaultToastStore, limit = 5, className }: ToasterProps) {
  const toasts = useSyncExternalStore(store.subscribe, store.getSnapshot, () => NO_TOASTS);
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  const shown = toasts.slice(-Math.max(1, limit));

  const onBlur = (event: FocusEvent<HTMLElement>) => {
    if (!event.currentTarget.contains(event.relatedTarget)) setFocused(false);
  };

  return (
    <section
      aria-label="Notifications"
      onPointerEnter={() => setHovered(true)}
      onPointerLeave={() => setHovered(false)}
      onFocus={() => setFocused(true)}
      onBlur={onBlur}
      className={cn(
        "pointer-events-none fixed right-4 bottom-4 z-50 w-80 max-w-[calc(100vw-2rem)]",
        className,
      )}
    >
      <ol aria-live="polite" className="flex flex-col gap-2">
        {shown.map((record) => (
          <ToastItem
            key={record.id}
            record={record}
            paused={hovered || focused}
            onDismiss={() => store.dismiss(record.id)}
          />
        ))}
      </ol>
    </section>
  );
}

function ToastItem({
  record,
  paused,
  onDismiss,
}: {
  record: ToastRecord;
  paused: boolean;
  onDismiss: () => void;
}) {
  const { border, icon, Icon } = TONE[record.tone];
  // What is left of the countdown, carried across pauses; restarted when the toast is
  // replaced in place (a new record under the same id).
  const remainingRef = useRef({ record, ms: record.duration });
  const dismissRef = useRef(onDismiss);
  useEffect(() => {
    dismissRef.current = onDismiss;
  });

  useEffect(() => {
    if (remainingRef.current.record !== record) remainingRef.current = { record, ms: record.duration };
    if (paused || !Number.isFinite(remainingRef.current.ms)) return;
    const started = Date.now();
    const timer = setTimeout(() => dismissRef.current(), remainingRef.current.ms);
    return () => {
      clearTimeout(timer);
      remainingRef.current.ms = Math.max(0, remainingRef.current.ms - (Date.now() - started));
    };
  }, [paused, record]);

  return (
    <li
      role={record.tone === "error" ? "alert" : undefined}
      data-tone={record.tone}
      className={cn(
        "pointer-events-auto flex items-start gap-2.5 rounded-panel border bg-overlay px-3 py-2.5 text-sm leading-snug shadow-lg shadow-black/25",
        "transition-[opacity,translate] duration-200 starting:translate-y-2 starting:opacity-0",
        border,
      )}
    >
      <Icon className={cn("mt-0.5 size-4 shrink-0", icon)} aria-hidden />
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <p className="font-medium text-fg">{record.title}</p>
        {record.description && <p className="text-fg-muted">{record.description}</p>}
      </div>
      <button
        type="button"
        aria-label="Dismiss"
        title="Dismiss"
        onClick={onDismiss}
        className={cn(
          "-mr-1 grid size-6 shrink-0 place-items-center rounded-control text-fg-subtle transition-colors hover:bg-raised hover:text-fg",
          focusRing,
        )}
      >
        <X className="size-3.5" aria-hidden />
      </button>
    </li>
  );
}
