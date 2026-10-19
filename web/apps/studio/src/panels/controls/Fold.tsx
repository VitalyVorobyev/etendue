import { ChevronRight } from "lucide-react";
import type { ReactNode } from "react";

/**
 * A titled group of the inspector that folds away. The header carries a one-line summary of
 * what is inside, so a folded group still says what it holds. A native `<details>`: the browser
 * owns the open state and the keyboard.
 */
export function Fold({
  title,
  summary,
  defaultOpen = true,
  children,
  testId,
}: {
  title: string;
  /** Shown at the header's right, in mono (a pose, a model name). */
  summary?: ReactNode;
  defaultOpen?: boolean;
  children: ReactNode;
  testId?: string | undefined;
}) {
  return (
    <details open={defaultOpen} className="group/fold border-t border-line" data-testid={testId}>
      <summary className="flex h-8 cursor-pointer list-none items-center gap-1.5 px-3 text-xs font-medium text-fg select-none hover:bg-raised focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-signal [&::-webkit-details-marker]:hidden">
        <ChevronRight
          className="size-3.5 shrink-0 text-fg-muted transition-transform group-open/fold:rotate-90"
          aria-hidden
        />
        <span className="shrink-0">{title}</span>
        {summary !== undefined && (
          <span className="ml-auto min-w-0 truncate font-mono text-[11px] font-normal text-fg-muted tabular-nums">
            {summary}
          </span>
        )}
      </summary>
      <div className="space-y-2 px-3 pt-1 pb-3">{children}</div>
    </details>
  );
}

/** Rows of label, control and value, with one label column for the whole group. */
export function Rows({ children }: { children: ReactNode }) {
  return <div className="grid grid-cols-[3.25rem_minmax(0,1fr)] items-center gap-x-2 gap-y-1">{children}</div>;
}

/** One row of {@link Rows}. */
export function RowLabel({ children, title }: { children: ReactNode; title?: string | undefined }) {
  return (
    <span className="truncate text-[11px] text-fg-muted" title={title}>
      {children}
    </span>
  );
}
