/**
 * The lens search: every lens of the project's library optimised on the head, ranked, each one
 * applicable (lens and setting together).
 */

import { Button } from "@vitavision/ui";
import { Search } from "lucide-react";
import { useState } from "react";

import { type Project, itemsOf } from "../io/project";
import type { HeadParams } from "./design";
import { type LensCandidate, type LensMiss, searchLenses } from "./lenses";

export function LensSearch({
  params,
  project,
  tiltFree,
  onApply,
}: {
  params: HeadParams;
  project: Project | null;
  tiltFree: boolean;
  onApply: (p: HeadParams) => void;
}) {
  const [found, setFound] = useState<{
    for: HeadParams;
    tiltFree: boolean;
    ranked: LensCandidate[];
    misses: LensMiss[];
  } | null>(null);
  const lenses = project
    ? itemsOf(project, "lens").flatMap(([ref, item]) => (item.lens ? [{ ref, name: item.name, lens: item.lens }] : []))
    : [];
  if (lenses.length === 0) return null;
  const shown = found?.for === params && found.tiltFree === tiltFree ? found : null;
  return (
    <div className="flex flex-col gap-2">
      <Button
        size="sm"
        variant="ghost"
        icon={<Search className="size-3.5" />}
        onClick={() => setFound({ for: params, tiltFree, ...searchLenses(params, lenses, tiltFree) })}
        title="Optimise the head with every lens of the library, and rank them"
      >
        Search {lenses.length} lenses
      </Button>
      {shown && (
        <div className="flex flex-col gap-1 text-xs" data-testid="lens-search">
          <p className="text-fg-muted">Most of the window seen first, then the smallest worst blur.</p>
          <ol className="flex flex-col divide-y divide-line rounded-md border border-line">
            {shown.ranked.map((c, n) => (
              <li key={c.ref} className="flex items-center gap-2 px-2 py-1.5" data-lens={c.ref}>
                <span className="w-4 shrink-0 text-fg-muted tabular-nums">{n + 1}</span>
                <div className="flex min-w-0 flex-1 flex-col">
                  <span
                    className={`truncate ${c.params.lens.name === params.lens.name ? "font-medium" : ""}`}
                    title={c.name}
                  >
                    {c.name}
                  </span>
                  <span className="font-mono text-[11px] text-fg-muted tabular-nums">
                    {(c.coverage * 100).toFixed(1)} % · {c.worstBlurPx.toFixed(2)} px · ≤{" "}
                    {(c.worstDepthPerPx * 1e6).toFixed(0)} µm/px · f/{c.params.f_number.toFixed(1)}
                  </span>
                </div>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => {
                    onApply(c.params);
                    setFound(null);
                  }}
                >
                  Apply
                </Button>
              </li>
            ))}
          </ol>
          {shown.misses.map((m) => (
            <p key={m.ref} className="text-fg-muted">
              {m.name}: {m.reason}
            </p>
          ))}
        </div>
      )}
    </div>
  );
}
